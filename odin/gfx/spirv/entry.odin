//! Selected-entry SPIR-V resource contracts for native graphics and compute pipelines.
package spirv

/// SPIR-V execution models supported by the ordinary native pipeline registry.
Stage :: enum { Vertex, Fragment, Compute }
/// Resources preserve logical descriptor-set and binding identities.
Resource_Kind :: enum { Buffer, Image, Sampler }
/// Selected-entry memory access, separate from resource declaration permissions.
Access :: enum { Read, Write, Read_Write, None }
/// Numeric scalar sampled from an image.
Sample_Type :: enum { None, Float, Sint, Uint }
/// Image dimensionality from OpTypeImage.
Dimension :: enum { D1, D2, D3, Cube, Rect, Buffer, Subpass }
/// One selected-stage descriptor from the actual compiled SPIR-V.
Resource :: struct {
    group,binding:u32,
    kind:Resource_Kind,
    storage:bool,
    minimum_size:u64,
    access,declared_access:Access,
    dimension:Dimension,
    arrayed,multisampled,depth,comparison:bool,
    array_count:u32,
    format:u32,
    sample_type:Sample_Type,
}
/// Owns one exact entry's native requirements and fragment color locations.
Stage_Reflection :: struct { resources:[dynamic]Resource, color_outputs:[dynamic]u32, local_size:[3]u32 }
/// Releases selected-entry arrays using their construction allocator.
stage_destroy :: proc(reflection:^Stage_Reflection) { delete(reflection.resources); delete(reflection.color_outputs); reflection^={} }

import "core:mem"
import "core:slice"

@(private="package")
Binary_Entry :: struct { model,id:u32, name:string, interface:[]u32 }
@(private="package")
Parsed_Module :: struct { using module:Module, entries:[dynamic]Binary_Entry, modes:[dynamic]Node, functions:map[u32][dynamic]Node }
@(private="package")
parsed_destroy :: proc(module:^Parsed_Module) {
    for _,body in module.functions { delete(body) }
    delete(module.functions); delete(module.nodes); delete(module.decorations); delete(module.entries); delete(module.modes)
    module^={}
}
@(private="package")
parse_module :: proc(words:[]u32,allocator:mem.Allocator)->(Parsed_Module,Error) {
    if len(words)<5 || words[0]!=0x07230203 || words[3]==0 || words[4]!=0 { return {},.Invalid_Module }
    if words[1]<0x00010000 || words[1]>0x00010600 || words[1]&0xff!=0 { return {},.Unsupported }
    module:=Parsed_Module{module=Module{make(map[u32]Node,allocator),make([dynamic]Decoration,allocator)},entries=make([dynamic]Binary_Entry,allocator),modes=make([dynamic]Node,allocator),functions=make(map[u32][dynamic]Node,allocator)}
    success:=false; defer { if !success { parsed_destroy(&module) } }
    current_function:u32
    for cursor:=5; cursor<len(words); {
        count:=int(words[cursor]>>16); opcode:=words[cursor]&0xffff
        if count==0 || count>len(words)-cursor { return {},.Invalid_Module }
        args:=words[cursor+1:cursor+count]; cursor+=count
        if opcode==54 {
            if current_function!=0 || len(args)!=4 { return {},.Invalid_Module }
            current_function=args[1]
            module.functions[current_function]=make([dynamic]Node,allocator)
        } else if opcode==56 {
            if current_function==0 || len(args)!=0 { return {},.Invalid_Module }
            current_function=0
        } else if current_function!=0 {
            body:=module.functions[current_function]
            append(&body,Node{opcode,args}); module.functions[current_function]=body
        }
        if opcode==15 {
            if len(args)<3 { return {},.Invalid_Module }
            name,valid:=entry_name(args[2:]); if !valid { return {},.Invalid_Module }
            name_words:=(len(name)+1+3)/4
            if len(name)==0 || 2+name_words>len(args) { return {},.Invalid_Module }
            append(&module.entries,Binary_Entry{args[0],args[1],name,args[2+name_words:]})
        } else if opcode==16 || opcode==331 {
            if len(args)<2 { return {},.Invalid_Module }
            append(&module.modes,Node{opcode,args})
        } else if opcode==71 || opcode==72 {
            member:=opcode==72
            if len(args)<(3 if member else 2) { return {},.Invalid_Module }
            kind:=args[2] if member else args[1]
            count_no_value:=3 if member else 2
            needs_value:=kind==6 || kind==7 || kind==11 || kind==30 || kind==33 || kind==34 || kind==35
            if needs_value && len(args)!=count_no_value+1 { return {},.Invalid_Module }
            if !needs_value && (kind==2 || kind==3 || kind==4 || kind==5 || kind==24 || kind==25) && len(args)!=count_no_value { return {},.Invalid_Module }
            d:=Decoration{target=args[0],kind=kind,is_member=member}
            if member { d.member=args[1] }
            if len(args)>count_no_value { d.value=args[count_no_value] }
            for prior in module.decorations { if prior.target==d.target && prior.member==d.member && prior.kind==d.kind && prior.is_member==d.is_member { return {},.Invalid_Module } }
            append(&module.decorations,d)
        } else {
            id:u32; result:=false
            if opcode>=19 && opcode<=39 { if len(args)<1 { return {},.Invalid_Module }; id=args[0]; result=true }
            else if opcode==41 || opcode==42 || opcode==43 || opcode==44 || opcode==46 || opcode==48 || opcode==49 || opcode==50 || opcode==51 || opcode==52 || opcode==54 || opcode==59 {
                if len(args)<2 { return {},.Invalid_Module }; id=args[1]; result=true
            }
            if result {
                if id==0 || id>=words[3] { return {},.Invalid_Module }
                if _,exists:=module.nodes[id]; exists { return {},.Invalid_Module }
                module.nodes[id]={opcode,args}
            }
        }
    }
    if current_function!=0 || len(module.entries)==0 { return {},.Invalid_Module }
    for entry in module.entries {
        function,present:=module.nodes[entry.id]
        if !present || function.opcode!=54 { return {},.Invalid_Module }
        for id in entry.interface { if id==0 || id>=words[3] { return {},.Invalid_Module } }
    }
    success=true; return module,.None
}
@(private="package")
referenced_globals :: proc(module:^Parsed_Module,function:u32,visited:^map[u32]bool,used:^map[u32]bool,depth:int)->bool {
    if depth>64 { return false }
    if visited[function] { return true }
    visited[function]=true
    instructions,present:=module.functions[function]; if !present { return false }
    for instruction in instructions {
        args:=instruction.args
        operands:[]u32
        switch instruction.opcode {
        case 57:
            if len(args)<3 || !referenced_globals(module,args[2],visited,used,depth+1) { return false }
            operands=args[3:]
        case 61,65,66,67,70,83:
            if len(args)<3 { return false }; operands=args[2:3]
        case 62,63,64:
            if len(args)<2 { return false }; operands=args[:2]
        case 68:
            if len(args)!=4 { return false }; operands=args[2:3]
        case 227,229,230,231,232,233,234,235,236,237,238,239,240,241,242:
            if len(args)<3 { return false }; operands=args[2:3]
        case 228:
            if len(args)<1 { return false }; operands=args[:1]
        case 169:
            if len(args)!=5 { return false }; operands=args[3:]
        case 245:
            if len(args)<4 || len(args)%2!=0 { return false }
            for i:=2;i<len(args);i+=2 { used[args[i]]=true }
        }
        for id in operands { used[id]=true }
    }
    return true
}
@(private="package")
all_members_decorated :: proc(module:^Module,type_id,kind:u32)->bool {
    T,exists:=module.nodes[type_id]
    if !exists || T.opcode!=30 || len(T.args)<2 { return false }
    for _,i in T.args[1:] { _,present:=decoration(module,type_id,kind,u32(i),true); if !present { return false } }
    return true
}
@(private="package")
resource_access :: proc(module:^Module,variable,type_id:u32)->Access {
    _,no_write:=decoration(module,variable,24)
    _,type_no_write:=decoration(module,type_id,24)
    _,no_read:=decoration(module,variable,25)
    _,type_no_read:=decoration(module,type_id,25)
    no_write=no_write || type_no_write || all_members_decorated(module,type_id,24)
    no_read=no_read || type_no_read || all_members_decorated(module,type_id,25)
    if no_write { return .Read }
    if no_read { return .Write }
    return .Read_Write
}
@(private="package")
reflect_resource :: proc(module:^Module,id:u32,variable:Node)->(Resource,Error) {
    args:=variable.args
    if len(args)<3 { return {},.Invalid_Module }
    pointer,exists:=module.nodes[args[0]]
    if !exists || pointer.opcode!=32 || len(pointer.args)!=3 || pointer.args[1]!=args[2] { return {},.Invalid_Module }
    group,has_group:=decoration(module,id,34)
    binding,has_binding:=decoration(module,id,33)
    if !has_group || !has_binding { return {},.Invalid_Module }
    result:=Resource{group=group,binding=binding,array_count=1,access=.Read}
    T,found:=module.nodes[pointer.args[2]]; if !found { return {},.Invalid_Module }
    type_id:=pointer.args[2]
    if T.opcode==28 {
        if len(T.args)!=3 { return {},.Invalid_Module }
        count,valid:=constant(module,T.args[2]); if !valid || count==0 || count>u64(max(u32)) { return {},.Unsupported }
        result.array_count=u32(count); type_id=T.args[1]
        T,found=module.nodes[type_id]; if !found { return {},.Invalid_Module }
    } else if T.opcode==29 { return {},.Unsupported }
    storage:=args[2]
    if storage==2 || storage==12 {
        if T.opcode!=30 { return {},.Unsupported }
        _,block:=decoration(module,type_id,2)
        _,buffer_block:=decoration(module,type_id,3)
        if !block && !buffer_block { return {},.Unsupported }
        size,valid:=span(module,type_id,0); if !valid { return {},.Unsupported }
        result.kind=.Buffer; result.storage=storage==12 || buffer_block; result.minimum_size=size
        if result.storage { result.access=resource_access(module,id,type_id) }
        return result,.None
    }
    if storage!=0 { return {},.Unsupported }
    switch T.opcode {
    case 25:
        if len(T.args)!=8 && len(T.args)!=9 { return {},.Invalid_Module }
        if T.args[2]>6 || T.args[3]>2 || T.args[4]>1 || T.args[5]>1 || T.args[6]>2 { return {},.Invalid_Module }
        result.kind=.Image; result.dimension=Dimension(T.args[2]); result.depth=T.args[3]==1
        result.arrayed=T.args[4]==1; result.multisampled=T.args[5]==1; result.storage=T.args[6]==2
        result.format=T.args[7]
        scalar,scalar_exists:=module.nodes[T.args[1]]
        if !scalar_exists { return {},.Invalid_Module }
        if scalar.opcode==22 && len(scalar.args)==2 { result.sample_type=.Float }
        else if scalar.opcode==21 && len(scalar.args)==3 { result.sample_type=.Sint if scalar.args[2]==1 else .Uint }
        else { return {},.Unsupported }
        if result.storage { result.access=resource_access(module,id,type_id) }
    case 26:
        if len(T.args)!=1 { return {},.Invalid_Module }
        result.kind=.Sampler
    case 27: return {},.Unsupported
    case: return {},.Unsupported
    }
    return result,.None
}
@(private="package")
Usage :: struct { read,write:bool }
@(private="package")
record_usage :: proc(usages:^map[u32]Usage,id:u32,read,write:bool) {
    if id==0 { return }
    old:=usages[id]; usages[id]={old.read || read,old.write || write}
}
@(private="package")
resource_pointer_type :: proc(module:^Parsed_Module,type_id:u32)->bool {
    T,present:=module.nodes[type_id]
    if !present { return false }
    return T.opcode==32 && len(T.args)==3 && (T.args[1]==0 || T.args[1]==2 || T.args[1]==12)
}
@(private="package")
origin :: proc(module:^Parsed_Module,origins:map[u32]u32,id:u32)->u32 {
    node,present:=module.nodes[id]
    if present && node.opcode==59 && len(node.args)>=3 && (node.args[2]==0 || node.args[2]==2 || node.args[2]==12) { return id }
    return origins[id]
}
@(private="package")
comparison_samplers :: proc(module:^Parsed_Module,function:u32,parameters:[]u32,compared:^map[u32]bool,usages:^map[u32]Usage,budget:^int,depth:int,allocator:mem.Allocator)->(u32,bool) {
    if depth>64 { return 0,false }
    body,present:=module.functions[function]; if !present { return 0,false }
    origins:=make(map[u32]u32,allocator); sampled:=make(map[u32]u32,allocator)
    defer delete(origins); defer delete(sampled)
    parameter_index:=0; returned:u32; has_return:=false
    for instruction in body {
        budget^-=1; if budget^<0 { return 0,false }
        args:=instruction.args
        switch instruction.opcode {
        case 55:
            if len(args)!=2 || parameter_index>=len(parameters) { return 0,false }
            origins[args[1]]=parameters[parameter_index]; parameter_index+=1
        case 61,65,66,67,70,83,60:
            if len(args)<3 { return 0,false }
            root:=origin(module,origins,args[2])
            origins[args[1]]=root
            if instruction.opcode==61 {
                variable,exists:=module.nodes[root]
                if exists && variable.opcode==59 && (variable.args[2]==2 || variable.args[2]==12) { record_usage(usages,root,true,false) }
            }
        case 62,63,64:
            if len(args)<2 { return 0,false }
            record_usage(usages,origin(module,origins,args[0]),false,true)
            if instruction.opcode!=62 { record_usage(usages,origin(module,origins,args[1]),true,false) }
        case 227,229,230,231,232,233,234,235,236,237,238,239,240,241,242:
            if len(args)<3 { return 0,false }
            record_usage(usages,origin(module,origins,args[2]),true,instruction.opcode!=227)
        case 228:
            if len(args)<1 { return 0,false }
            record_usage(usages,origin(module,origins,args[0]),false,true)
        case 169:
            if len(args)!=5 { return 0,false }
            left,right:=origin(module,origins,args[3]),origin(module,origins,args[4])
            if left==right { origins[args[1]]=left }
            else if resource_pointer_type(module,args[0]) { return 0,false }
        case 245:
            if len(args)<4 || len(args)%2!=0 { return 0,false }
            first:=origin(module,origins,args[2]); same:=true
            for i:=4;i<len(args);i+=2 { if origin(module,origins,args[i])!=first { same=false } }
            if same { origins[args[1]]=first }
            else if resource_pointer_type(module,args[0]) { return 0,false }
        case 57:
            if len(args)<3 { return 0,false }
            passed:=make([]u32,len(args)-3,allocator); defer delete(passed,allocator)
            for argument,i in args[3:] { passed[i]=origin(module,origins,argument) }
            value,valid:=comparison_samplers(module,args[2],passed,compared,usages,budget,depth+1,allocator)
            if !valid { return 0,false }; origins[args[1]]=value
        case 86:
            if len(args)!=4 { return 0,false }
            sampled[args[1]]=origin(module,origins,args[3])
            origins[args[1]]=origin(module,origins,args[2])
        case 87,88,91,92,95,96,98,305,306,309,310,313,314:
            if len(args)<3 { return 0,false }
            record_usage(usages,origin(module,origins,args[2]),true,false)
        case 99:
            if len(args)<3 { return 0,false }
            record_usage(usages,origin(module,origins,args[0]),false,true)
        case 89,90,93,94,97,307,308,311,312,315:
            if len(args)<3 { return 0,false }
            sampler:=sampled[args[2]]
            if sampler==0 { return 0,false }
            compared[sampler]=true
            record_usage(usages,origin(module,origins,args[2]),true,false)
        case 254:
            if len(args)!=1 { return 0,false }
            value:=origin(module,origins,args[0])
            if has_return && returned!=value { returned=0 }
            else { returned=value; has_return=true }
        }
    }
    return returned,true
}
/// Reflects only resources used by the selected entry and validates native descriptor identities.
reflect_entry :: proc(words:[]u32,entry:string,stage:Stage,allocator:=context.allocator)->(Stage_Reflection,Error) {
    if len(entry)==0 { return {},.Invalid_Module }
    module,parse_error:=parse_module(words,allocator)
    if parse_error!=.None { return {},parse_error }; defer parsed_destroy(&module)
    model:u32
    switch stage {
    case .Vertex: model=0
    case .Fragment: model=4
    case .Compute: model=5
    }
    selected:Binary_Entry; count:=0
    for candidate in module.entries { if candidate.model==model && candidate.name==entry { selected=candidate; count+=1 } }
    if count!=1 { return {},.Invalid_Module }
    result:=Stage_Reflection{resources=make([dynamic]Resource,allocator),color_outputs=make([dynamic]u32,allocator)}
    success:=false; defer { if !success { stage_destroy(&result) } }
    used:=make(map[u32]bool,allocator); visited:=make(map[u32]bool,allocator); defer delete(used); defer delete(visited)
    if !referenced_globals(&module,selected.id,&visited,&used,0) { return {},.Invalid_Module }
    for id in selected.interface { used[id]=true }
    compared:=make(map[u32]bool,allocator); defer delete(compared)
    usages:=make(map[u32]Usage,allocator); defer delete(usages)
    budget:=1_000_000
    _,comparison_valid:=comparison_samplers(&module,selected.id,nil,&compared,&usages,&budget,0,allocator)
    if !comparison_valid { return {},.Unsupported }
    if stage==.Compute {
        matched:=0
        for mode in module.modes {
            if mode.args[0]!=selected.id { continue }
            if !((mode.opcode==16 && mode.args[1]==17) || (mode.opcode==331 && mode.args[1]==38)) { continue }
            if len(mode.args)!=5 { return {},.Invalid_Module }
            matched+=1
            for i in 0..<3 {
                value:=u64(mode.args[i+2]); valid:=true
                if mode.opcode==331 { value,valid=constant(&module.module,mode.args[i+2]) }
                if !valid { return {},.Unsupported }
                if value==0 || value>u64(max(u32)) { return {},.Invalid_Module }
                result.local_size[i]=u32(value)
            }
        }
        if matched!=1 { return {},.Invalid_Module }
    }
    for id,variable in module.nodes {
        if variable.opcode!=59 || !used[id] { continue }
        if len(variable.args)<3 { return {},.Invalid_Module }
        storage:=variable.args[2]
        if storage==0 || storage==2 || storage==12 {
            resource,err:=reflect_resource(&module.module,id,variable); if err!=.None { return {},err }
            resource.declared_access=resource.access
            actual:=usages[id]
            if (resource.declared_access==.Read && actual.write) || (resource.declared_access==.Write && actual.read) { return {},.Invalid_Module }
            if actual.read && actual.write { resource.access=.Read_Write }
            else if actual.read { resource.access=.Read }
            else if actual.write { resource.access=.Write }
            else { resource.access=.None }
            if resource.kind==.Sampler { resource.comparison=compared[id] }
            for previous in result.resources { if previous.group==resource.group && previous.binding==resource.binding { return {},.Invalid_Module } }
            append(&result.resources,resource)
        } else if storage==9 { return {},.Unsupported }
        else if stage==.Fragment && storage==3 {
            location,present:=decoration(&module.module,id,30)
            if present {
                for existing in result.color_outputs { if existing==location { return {},.Invalid_Module } }
                append(&result.color_outputs,location)
            } else {
                pointer,found:=module.nodes[variable.args[0]]; if !found || pointer.opcode!=32 || len(pointer.args)!=3 { return {},.Invalid_Module }
                T,exists:=module.nodes[pointer.args[2]]
                if exists && T.opcode==30 { for _,i in T.args[1:] {
                    member_location,has_location:=decoration(&module.module,pointer.args[2],30,u32(i),true)
                    if has_location { for existing in result.color_outputs { if existing==member_location { return {},.Invalid_Module } }; append(&result.color_outputs,member_location) }
                } }
            }
        }
    }
    slice.sort_by(result.resources[:],proc(a,b:Resource)->bool { return a.group<b.group || (a.group==b.group && a.binding<b.binding) })
    slice.sort(result.color_outputs[:])
    success=true; return result,.None
}
