//! Checked SPIR-V buffer layout reflection; unsupported shader shapes fail explicitly.
package spirv

import "core:mem"

/// Reflection distinguishes malformed modules from unsupported resource contracts.
Error :: enum { None, Invalid_Module, Unsupported }
/// One set-zero buffer descriptor and the byte span its declared type requires.
Buffer :: struct { slot:u32, storage:bool, minimum_size:u64 }
/// Owns reflection arrays; source instruction operands remain borrowed during reflection only.
Reflection :: struct { buffers:[dynamic]Buffer, local_size:[3]u32 }
@(private="package")
Node :: struct { opcode:u32, args:[]u32 }
@(private="package")
Decoration :: struct { target,member,kind,value:u32, is_member:bool }
@(private="package")
Module :: struct { nodes:map[u32]Node, decorations:[dynamic]Decoration }
/// Releases only the owned reflected descriptors.
destroy :: proc(reflection:^Reflection) { delete(reflection.buffers); reflection^={} }
@(private="package")
decoration :: proc(module:^Module,id,kind:u32,member:u32=0,is_member:=false)->(u32,bool) {
    for d in module.decorations { if d.target==id && d.kind==kind && d.is_member==is_member && (!is_member || d.member==member) { return d.value,true } }
    return 0,false
}
@(private="package")
constant :: proc(module:^Module,id:u32)->(u64,bool) {
    node,found:=module.nodes[id]
    if !found || node.opcode!=43 || len(node.args)<3 { return 0,false }
    T,ok:=module.nodes[node.args[0]]
    if !ok || T.opcode!=21 || len(T.args)!=3 || (T.args[1]!=32 && T.args[1]!=64) { return 0,false }
    if T.args[1]==64 {
        if len(node.args)!=4 { return 0,false }
        return u64(node.args[2])|u64(node.args[3])<<32,true
    }
    if len(node.args)!=3 { return 0,false }
    return u64(node.args[2]),true
}
@(private="package")
span :: proc(module:^Module,id:u32,depth:int,matrix_stride:u32=0,row_major:=false)->(u64,bool) {
    if depth>32 { return 0,false }
    node,exists:=module.nodes[id]; if !exists { return 0,false }
    args:=node.args
    switch node.opcode {
    case 21,22:
        if len(args)<2 || (args[1]!=32 && args[1]!=64) { return 0,false }
        return u64(args[1]/8),true
    case 23:
        if len(args)!=3 || args[2]==0 || args[2]>4 { return 0,false }
        element,ok:=span(module,args[1],depth+1); if !ok { return 0,false }
        return element*u64(args[2]),true
    case 24:
        if len(args)!=3 || args[2]==0 || args[2]>4 || matrix_stride==0 { return 0,false }
        vector,ok:=module.nodes[args[1]]; if !ok || vector.opcode!=23 || len(vector.args)!=3 { return 0,false }
        size,valid:=span(module,args[1],depth+1); if !valid { return 0,false }
        count:=args[2]
        if row_major {
            scalar,scalar_ok:=span(module,vector.args[1],depth+1); if !scalar_ok { return 0,false }
            size=scalar*u64(args[2]); count=vector.args[2]
        }
        if size>u64(matrix_stride) { return 0,false }
        return u64(count-1)*u64(matrix_stride)+size,true
    case 28,29:
        if (node.opcode==28 && len(args)!=3) || (node.opcode==29 && len(args)!=2) { return 0,false }
        stride,present:=decoration(module,id,6); if !present || stride==0 { return 0,false }
        element,ok:=span(module,args[1],depth+1,matrix_stride,row_major)
        if !ok || element>u64(stride) { return 0,false }
        if node.opcode==29 { return u64(stride),true }
        count,valid:=constant(module,args[2]); if !valid || count==0 || count>max(u64)/u64(stride) { return 0,false }
        return count*u64(stride),true
    case 30:
        size:u64
        for member,i in args[1:] {
            offset,has_offset:=decoration(module,id,35,u32(i),true); if !has_offset { return 0,false }
            stride,_:=decoration(module,id,7,u32(i),true)
            _,row:=decoration(module,id,4,u32(i),true)
            member_size,ok:=span(module,member,depth+1,stride,row)
            if !ok || member_size>max(u64)-u64(offset) { return 0,false }
            size=max(size,u64(offset)+member_size)
        }
        return size,size>0
    }
    return 0,false
}
@(private="package")
entry_name :: proc(words:[]u32)->(string,bool) {
    bytes:=mem.slice_to_bytes(words)
    for ch,i in bytes { if ch==0 { return string(bytes[:i]),true } }
    return "",false
}
/// Reflects a single compute entry, fixed local sizes and set-zero scalar/vector/matrix buffers.
reflect :: proc(words:[]u32,entry:string,allocator:=context.allocator)->(Reflection,Error) {
    if len(words)<5 || words[0]!=0x07230203 || words[3]==0 || words[4]!=0 || len(entry)==0 { return {},.Invalid_Module }
    module:=Module{nodes=make(map[u32]Node,allocator),decorations=make([dynamic]Decoration,allocator)}
    defer delete(module.nodes); defer delete(module.decorations)
    reflection:=Reflection{buffers=make([dynamic]Buffer,allocator)}
    success:=false; defer { if !success { destroy(&reflection) } }
    compute_entries:=0; function_id:u32
    mode_args:[]u32; mode_ids:=false; function_open:=false
    for cursor:=5; cursor<len(words); {
        count:=int(words[cursor]>>16); opcode:=words[cursor]&0xffff
        if count==0 || count>len(words)-cursor { return {},.Invalid_Module }
        args:=words[cursor+1:cursor+count]
        cursor+=count
        if opcode==54 {
            if function_open || len(args)!=4 { return {},.Invalid_Module }; function_open=true
        } else if opcode==56 {
            if !function_open || len(args)!=0 { return {},.Invalid_Module }; function_open=false
        }
        if opcode==15 {
            if len(args)<3 { return {},.Invalid_Module }
            name,valid:=entry_name(args[2:]); if !valid { return {},.Invalid_Module }
            if args[0]!=5 { return {},.Unsupported }
            compute_entries+=1
            if name==entry { function_id=args[1] }
        } else if opcode==16 || opcode==331 {
            if len(args)<2 { return {},.Invalid_Module }
            if (opcode==16 && args[1]==17) || (opcode==331 && args[1]==38) {
                if len(args)!=5 { return {},.Invalid_Module }
                if mode_args!=nil { return {},.Unsupported }
                mode_args=args; mode_ids=opcode==331
            }
        } else if opcode==71 {
            if len(args)<2 { return {},.Invalid_Module }
            if (args[1]==6 || args[1]==33 || args[1]==34) && len(args)!=3 { return {},.Invalid_Module }
            value:u32
            if len(args)>2 { value=args[2] }
            append(&module.decorations,Decoration{target=args[0],kind=args[1],value=value})
        } else if opcode==72 {
            if len(args)<3 { return {},.Invalid_Module }
            if (args[2]==7 || args[2]==35) && len(args)!=4 { return {},.Invalid_Module }
            value:u32
            if len(args)>3 { value=args[3] }
            append(&module.decorations,Decoration{target=args[0],member=args[1],kind=args[2],value=value,is_member=true})
        } else {
            id:u32; has_id:=false
            if opcode>=19 && opcode<=33 { if len(args)<1 { return {},.Invalid_Module }; id=args[0]; has_id=true }
            else if opcode==43 || opcode==59 || opcode==54 { if len(args)<3 { return {},.Invalid_Module }; id=args[1]; has_id=true }
            if has_id {
                if id==0 || id>=words[3] { return {},.Invalid_Module }
                if _,exists:=module.nodes[id]; exists { return {},.Invalid_Module }
                module.nodes[id]={opcode,args}
            }
        }
    }
    if function_open { return {},.Invalid_Module }
    if words[1]<0x00010000 || words[1]>0x00010600 || words[1]&0xff!=0 { return {},.Unsupported }
    if compute_entries!=1 { return {},.Unsupported }
    if function_id==0 || mode_args==nil || mode_args[0]!=function_id { return {},.Invalid_Module }
    function,has_function:=module.nodes[function_id]
    if !has_function || function.opcode!=54 { return {},.Invalid_Module }
    for i in 0..<3 {
        value:=u64(mode_args[i+2])
        if mode_ids { value_ok:bool; value,value_ok=constant(&module,mode_args[i+2]); if !value_ok { return {},.Unsupported } }
        if value==0 || value>u64(max(u32)) { return {},.Invalid_Module }
        reflection.local_size[i]=u32(value)
    }
    for _,variable in module.nodes {
        if variable.opcode!=59 { continue }
        args:=variable.args
        if len(args)<3 { return {},.Invalid_Module }
        storage:=args[2]
        if storage!=2 && storage!=12 {
            if storage==0 || storage==9 { return {},.Unsupported }
            continue
        }
        pointer,exists:=module.nodes[args[0]]
        if !exists || pointer.opcode!=32 || len(pointer.args)!=3 || pointer.args[1]!=storage { return {},.Invalid_Module }
        slot,has_slot:=decoration(&module,args[1],33)
        set,has_set:=decoration(&module,args[1],34)
        if !has_slot || !has_set { return {},.Invalid_Module }
        if set!=0 || slot>=32 { return {},.Unsupported }
        _,block:=decoration(&module,pointer.args[2],2)
        if !block { return {},.Unsupported }
        size,valid:=span(&module,pointer.args[2],0); if !valid { return {},.Unsupported }
        for previous in reflection.buffers { if previous.slot==slot { return {},.Invalid_Module } }
        append(&reflection.buffers,Buffer{slot,storage==12,size})
    }
    success=true; return reflection,.None
}
