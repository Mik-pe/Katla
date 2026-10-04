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
/// Adapts the canonical selected-entry decoder to set-zero compute buffer packets.
reflect :: proc(words:[]u32,entry:string,allocator:=context.allocator)->(Reflection,Error) {
    selected,err:=reflect_entry(words,entry,.Compute,allocator)
    if err!=.None { return {},err }; defer stage_destroy(&selected)
    reflection:=Reflection{buffers=make([dynamic]Buffer,allocator),local_size=selected.local_size}
    success:=false; defer { if !success { destroy(&reflection) } }
    for resource in selected.resources {
        if resource.kind!=.Buffer || resource.group!=0 || resource.binding>=32 || resource.array_count!=1 { return {},.Unsupported }
        append(&reflection.buffers,Buffer{resource.binding,resource.storage,resource.minimum_size})
    }
    success=true; return reflection,.None
}
