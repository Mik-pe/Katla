//! Authored executable buffer work and backend-independent resource preflight.
package gfx

import "core:mem"

/// Binds one actual resource use to a shader slot.
Buffer_Binding :: struct { group,slot:u32, access:Buffer_Access }
/// Dispatch dimensions are workgroup counts; shader local size belongs to its pipeline.
Indirect_Dispatch :: struct { enabled:bool, command:Buffer_Access }
Dispatch :: struct { pipeline:Pipeline_Handle, groups:[3]u32, bindings:[]Buffer_Binding, images:[]Image_Binding, samplers:[]Sampler_Binding, indirect:Indirect_Dispatch }
/// Transfers bytes between declared buffer resources without hidden copies.
Copy_Buffer :: struct { source,destination:Resource_Id, source_offset,destination_offset,size:u64 }
/// Exactly one operation per pass; more work uses another ordinary pass.
Fill_Buffer :: struct { destination:Resource_Id, offset,size:u64, value:u32 }
/// Filters an initialized base mip into the remaining explicitly selected chain.
Generate_Mips :: struct { resource:Image_Id, range:Image_Range }
Packet :: union { Dispatch, Copy_Buffer, Fill_Buffer, Render, Copy_Image_Buffer, Copy_Buffer_Image, Generate_Mips }
/// A reflected binding's native type, minimum span and offset alignment.
Binding_Requirement :: struct { group,slot:u32, usage:Buffer_Usage, minimum_size,alignment,maximum_size:u64, mode:Access_Mode }
/// Borrowed reflection remains valid while the pipeline registry retains its owner.
Pipeline_Info :: struct { bindings:[]Binding_Requirement, local_size:[3]u32, max_threads:u64, images:[]Image_Binding_Requirement, samplers:[]Sampler_Requirement }
/// Native identity detects graph roles that alias one physical allocation.
Buffer_Info :: struct { desc:Buffer_Desc, identity:rawptr }
/// Required preflight queries; a missing callback rejects execution explicitly.
Resource_Query :: struct {
    state:rawptr,
    buffer:proc(rawptr,Buffer_Handle)->(Buffer_Info,bool),
    pipeline:proc(rawptr,Pipeline_Handle)->(Pipeline_Info,bool),
    max_groups:[3]u32,
}
/// Selects an actual allocation for one graph resource in the current frame.
Buffer_Input :: struct { resource:Resource_Id, handle:Buffer_Handle }
/// Reports validation before any native encoder or queue mutation.
Packet_Error :: enum { None, Invalid_Pass, Invalid_Packet, Missing_Packet, Undeclared_Access, Invalid_Plan, Missing_Resource, Invalid_Buffer, Aliased_Resource, Invalid_Pipeline, Invalid_Dispatch, Missing_Binding, Invalid_Binding, Unsupported_Query }
/// Owns one frozen packet; resource and pipeline handles resolve again before native retention.
Prepared_Pass :: struct { id:Pass_Id, kind:Pass_Kind, packet:Packet }
/// Owns validated execution inputs, packets and compiled hazards for one recording.
Prepared_Graph :: struct { passes:[dynamic]Prepared_Pass, buffers:[]Buffer_Input, hazards:[]Hazard, allocator:mem.Allocator, textures:[]Texture_Input, images:[]Prepared_Image, image_hazards:[]Image_Hazard, aliases:[dynamic]Alias_Handoff }
@(private="package")
packet_destroy :: proc(packet:^Packet,allocator:mem.Allocator) {
    #partial switch p in packet^ {
    case Dispatch: delete(p.bindings,allocator); image_bindings_destroy(p.images,allocator); delete(p.samplers,allocator)
    case Render: delete(p.colors,allocator); delete(p.buffers,allocator); image_bindings_destroy(p.images,allocator); delete(p.samplers,allocator); for &phase in p.phases { phase_destroy(&phase,allocator) }; delete(p.phases,allocator); constants_destroy(p.constants,allocator)
    }
    packet^={}
}
@(private="package")
packet_clone :: proc(packet:Packet,allocator:mem.Allocator)->Packet {
    #partial switch p in packet {
    case Dispatch:
        copy_packet:=p; copy_packet.bindings=make([]Buffer_Binding,len(p.bindings),allocator); copy(copy_packet.bindings,p.bindings); copy_packet.images=image_bindings_clone(p.images,allocator); copy_packet.samplers=clone_slice(p.samplers,allocator)
        return copy_packet
    case Render:
        result:=p
        result.colors=clone_slice(p.colors,allocator); result.buffers=clone_slice(p.buffers,allocator); result.images=image_bindings_clone(p.images,allocator); result.samplers=clone_slice(p.samplers,allocator); result.phases=make([]Render_Phase,len(p.phases),allocator); for phase,i in p.phases { result.phases[i]=phase_clone(phase,allocator) }; result.constants=constants_clone(p.constants,allocator)
        return result
    }
    return packet
}
/// Frees packet copies and arrays without destroying caller-selected native resources.
prepared_graph_destroy :: proc(prepared:^Prepared_Graph) {
    for &pass in prepared.passes { packet_destroy(&pass.packet,prepared.allocator) }
    delete(prepared.textures,prepared.allocator); delete(prepared.images,prepared.allocator); delete(prepared.image_hazards,prepared.allocator); delete(prepared.aliases); delete(prepared.passes); delete(prepared.buffers,prepared.allocator); delete(prepared.hazards,prepared.allocator); prepared^={}
}
@(private="package")
access_declared :: proc(pass:Graph_Pass,access:Buffer_Access)->bool {
    for declared in pass.accesses {
        if declared.resource!=access.resource || declared.usage!=access.usage { continue }
        if declared.mode!=.Read_Write && declared.mode!=access.mode { continue }
        if access.range.offset<declared.range.offset { continue }
        relative:=access.range.offset-declared.range.offset
        if access.range.size>0 && relative<=declared.range.size && access.range.size<=declared.range.size-relative { return true }
    }
    return false
}
@(private="package")
packet_accesses :: proc(g:^Graph,packet:Packet,allocator:mem.Allocator)->[]Buffer_Access {
    #partial switch p in packet {
    case Dispatch:
        result:=make([]Buffer_Access,len(p.bindings)+int(p.indirect.enabled),allocator)
        for binding,i in p.bindings { result[i]=binding.access }
        if p.indirect.enabled { result[len(p.bindings)]=p.indirect.command }
        return result
    case Render:
        accesses:=make([dynamic]Buffer_Access,allocator); defer delete(accesses)
        for binding in p.buffers { append(&accesses,binding.access) }
        for phase in p.phases { for draw in phase.draws { draw_buffer_accesses(draw,&accesses) } }
        return clone_slice(accesses[:],allocator)
    case Fill_Buffer:
        return clone_slice([]Buffer_Access{{p.destination,{p.offset,p.size},.Write,.Transfer_Destination}},allocator)
    case Copy_Buffer_Image:
        desc:=g.images[p.destination.index].desc
        result:=make([]Buffer_Access,1,allocator)
        layout,_:=image_region_layout(p.region,desc)
        result[0]={p.source,{p.source_offset,layout.required_bytes},.Read,.Transfer_Source}
        return result
    case Copy_Image_Buffer:
        desc:=g.images[p.source.index].desc
        layout,_:=image_region_layout(p.region,desc)
        if layout.bytes_per_row==layout.row_bytes && layout.bytes_per_image==layout.row_bytes*layout.block_rows {
            return clone_slice([]Buffer_Access{{p.destination,{p.destination_offset,layout.required_bytes},.Write,.Transfer_Destination}},allocator)
        }
        result:=make([]Buffer_Access,int(layout.block_rows*u64(p.region.depth)),allocator)
        for z in 0..<u64(p.region.depth) { for row in 0..<layout.block_rows {
            offset:=p.destination_offset+z*layout.bytes_per_image+row*layout.bytes_per_row
            result[int(z*layout.block_rows+row)]={p.destination,{offset,layout.row_bytes},.Write,.Transfer_Destination}
        } }
        return result
    case Copy_Buffer:
        result:=make([]Buffer_Access,2,allocator)
        result[0]={p.source,{p.source_offset,p.size},.Read,.Transfer_Source}
        result[1]={p.destination,{p.destination_offset,p.size},.Write,.Transfer_Destination}
        return result
    }
    return nil
}
@(private="package")
validate_packet :: proc(g:^Graph,pass:Graph_Pass,packet:Packet)->Packet_Error {
    switch p in packet {
    case Dispatch:
        if pass.kind!=.Compute || len(p.bindings)+len(p.images)+len(p.samplers)>32 { return .Invalid_Packet }
        if p.indirect.enabled {
            for count in p.groups { if count!=0 { return .Invalid_Dispatch } }
            a:=p.indirect.command
            if a.usage!=.Indirect || a.mode!=.Read || a.range.size!=12 || a.range.offset%4!=0 { return .Invalid_Dispatch }
        } else { for count in p.groups { if count==0 { return .Invalid_Dispatch } } }
        compute_error:=validate_compute_bindings(p); if compute_error!=.None { return compute_error }
        for binding,i in p.bindings {
            for previous in p.bindings[:i] { if previous.group==binding.group && previous.slot==binding.slot { return .Invalid_Binding } }
        }
    case Fill_Buffer:
        if pass.kind!=.Transfer || p.size==0 || p.size%4!=0 || p.offset%4!=0 { return .Invalid_Packet }
    case Generate_Mips:
        if pass.kind!=.Transfer || p.resource.owner!=g || p.resource.index<0 || p.resource.index>=len(g.images) { return .Invalid_Packet }
        desc:=g.images[p.resource.index].desc
        if !image_range_valid(p.range,desc) || p.range.aspects!={.Color} || p.range.mip_count<2 || !texture_filterable_mips(desc.format) { return .Invalid_Packet }
    case Copy_Buffer:
        if pass.kind!=.Transfer || p.size==0 { return .Invalid_Packet }
        if p.source==p.destination && range_overlaps({p.source_offset,p.size},{p.destination_offset,p.size}) { return .Invalid_Packet }
    case Render:
        err:=validate_render_packet(g,pass,p); if err!=.None { return err }
    case Copy_Buffer_Image:
        if pass.kind!=.Transfer || p.destination.owner!=g || p.destination.index<0 || p.destination.index>=len(g.images) { return .Invalid_Packet }
        desc:=g.images[p.destination.index].desc
        if !image_region_valid(p.region,desc) || p.region.aspect==.Stencil { return .Invalid_Packet }
        layout,valid:=image_region_layout(p.region,desc); if !valid || layout.required_bytes>max(u64)-p.source_offset { return .Invalid_Packet }
    case Copy_Image_Buffer:
        if pass.kind!=.Transfer || p.source.owner!=g || p.source.index<0 || p.source.index>=len(g.images) { return .Invalid_Packet }
        desc:=g.images[p.source.index].desc
        if !image_region_valid(p.region,desc) { return .Invalid_Packet }
        layout,valid:=image_region_layout(p.region,desc); if !valid || layout.required_bytes>max(u64)-p.destination_offset || layout.block_rows*u64(p.region.depth)>u64(max(int)) { return .Invalid_Packet }
    case: return .Invalid_Packet
    }
    accesses:=packet_accesses(g,packet,g.allocator); defer delete(accesses,g.allocator)
    for access in accesses { if !access_declared(pass,access) { return .Undeclared_Access } }
    // Initialization and exports rely on writes actually covering their declaration.
    for declaration in pass.accesses {
        found:=false
        for access in accesses { if access==declaration { found=true; break } }
        if !found { return .Undeclared_Access }
    }
    image_accesses:=packet_image_accesses(g,packet,g.allocator); defer delete(image_accesses,g.allocator)
    for access in image_accesses { if !image_access_declared(pass,access) { return .Undeclared_Access } }
    for declaration in pass.images {
        found:=false
        for access in image_accesses { if access==declaration { found=true; break } }
        if !found { return .Undeclared_Access }
    }
    return .None
}
/// Validates replacement work before copying it; failure preserves the previous packet.
graph_set_packet :: proc(g:^Graph,id:Pass_Id,packet:Packet)->Packet_Error {
    if id.owner!=g || id.index<0 || id.index>=len(g.passes) { return .Invalid_Pass }
    pass:=&g.passes[id.index]
    err:=validate_packet(g,pass^,packet); if err!=.None { return err }
    candidate:=pass^; candidate.packet=packet; candidate.has_packet=true
    for access in pass.images {
        if image_access_stores_content(pass^,access)!=image_access_stores_content(candidate,access) { g.revision+=1; break }
    }
    owned:=packet_clone(packet,g.allocator)
    packet_destroy(&pass.packet,g.allocator); pass.packet=owned; pass.has_packet=true
    return .None
}
/// Replaces commands and their access contract atomically, invalidating old compiled plans.
graph_set_commands :: proc(g:^Graph,id:Pass_Id,packet:Packet,accesses:[]Buffer_Access,images:[]Image_Access)->(Packet_Error,Graph_Error) {
    if id.owner!=g || id.index<0 || id.index>=len(g.passes) { return .Invalid_Pass,.None }
    pass:=&g.passes[id.index]
    error:=validate_accesses(g,pass.kind,accesses); if error!=.None { return .Undeclared_Access,error }
    error=validate_image_accesses(g,pass.kind,images); if error!=.None { return .Undeclared_Access,error }
    candidate:=pass^; candidate.accesses=accesses; candidate.images=images
    packet_error:=validate_packet(g,candidate,packet); if packet_error!=.None { return packet_error,.None }
    owned_accesses:=clone_slice(accesses,g.allocator); owned_images:=clone_slice(images,g.allocator)
    owned_packet:=packet_clone(packet,g.allocator)
    delete(pass.accesses,g.allocator); delete(pass.images,g.allocator); packet_destroy(&pass.packet,g.allocator)
    pass.accesses=owned_accesses; pass.images=owned_images; pass.packet=owned_packet; pass.has_packet=true
    g.revision+=1
    return .None,.None
}
/// Resolves a graph resource from a prepared recording's immutable input list.
prepared_buffer :: proc(prepared:^Prepared_Graph,id:Resource_Id)->(Buffer_Handle,bool) {
    for input in prepared.buffers { if input.resource==id { return input.handle,true } }
    return {},false
}
@(private="package")
input_buffer :: proc(inputs:[]Buffer_Input,id:Resource_Id,query:Resource_Query)->(Buffer_Info,bool) {
    for input in inputs { if input.resource==id { return query.buffer(query.state,input.handle) } }
    return {},false
}
/// Freezes live work after checking real resources and every reflected shader binding.
graph_prepare :: proc(g:^Graph,plan:^Compiled_Graph,inputs:[]Buffer_Input,query:Resource_Query,textures:[]Texture_Input=nil,graphics:Graphics_Query={})->(Prepared_Graph,Packet_Error) {
    if plan.owner!=g || plan.revision!=g.revision { return {},.Invalid_Plan }
    if len(inputs)>0 && query.buffer==nil { return {},.Unsupported_Query }
    prepared:=Prepared_Graph{passes=make([dynamic]Prepared_Pass,g.allocator),allocator=g.allocator}
    success:=false; defer { if !success { prepared_graph_destroy(&prepared) } }
    image_error:=prepare_images(&prepared,g,plan,textures,graphics)
    if image_error!=.None { return {},image_error }
    for input,i in inputs {
        id:=input.resource
        if id.owner!=g || id.index<0 || id.index>=len(g.buffers) { return {},.Missing_Resource }
        info,ok:=query.buffer(query.state,input.handle)
        if !ok || info.identity==nil { return {},.Invalid_Buffer }
        desc:=g.buffers[id.index].desc
        if info.desc.memory!=desc.memory || info.desc.size<desc.size || desc.usage&info.desc.usage!=desc.usage { return {},.Invalid_Buffer }
        for previous in inputs[:i] {
            if previous.resource==id { return {},.Invalid_Buffer }
            previous_info,_:=query.buffer(query.state,previous.handle)
            if previous_info.identity==info.identity {
                err:=prepare_buffer_alias(&prepared,g,plan,previous.resource,id); if err!=.None { return {},err }
            }
        }
    }
    for buffer in inputs {
        buffer_info,_:=query.buffer(query.state,buffer.handle)
        for texture in textures {
            texture_info,_:=graphics.texture(graphics.state,texture.handle)
            if buffer_info.identity!=texture_info.identity { continue }
            err:=prepare_alias(&prepared,g,plan,buffer.resource,texture.resource); if err!=.None { return {},err }
        }
    }
    for id in plan.order {
        if id.owner!=g || id.index<0 || id.index>=len(g.passes) { return {},.Invalid_Plan }
        pass:=g.passes[id.index]
        if !pass.has_packet { return {},.Missing_Packet }
        for access in pass.accesses {
            info,ok:=input_buffer(inputs,access.resource,query)
            if !ok { return {},.Missing_Resource }
            if !range_valid(access.range,info.desc.size) { return {},.Invalid_Buffer }
        }
        #partial switch dispatch in pass.packet {
        case Dispatch:
            if query.pipeline==nil { return {},.Unsupported_Query }
            pipeline,ok:=query.pipeline(query.state,dispatch.pipeline)
            if !ok || len(pipeline.bindings)!=len(dispatch.bindings) { return {},.Invalid_Pipeline }
            threads:u64=1
            for count,i in dispatch.groups {
                local:=pipeline.local_size[i]
                if (!dispatch.indirect.enabled && (count==0 || count>query.max_groups[i])) || local==0 || u64(local)>max(u64)/threads { return {},.Invalid_Dispatch }
                threads*=u64(local)
            }
            if threads>pipeline.max_threads { return {},.Invalid_Dispatch }
            for requirement in pipeline.bindings {
                found:=false
                for binding in dispatch.bindings {
                    if binding.group!=requirement.group || binding.slot!=requirement.slot { continue }
                    found=true
                    access:=binding.access
                    if access.usage!=requirement.usage || !access_covers(access.mode,requirement.mode) || access.range.size<requirement.minimum_size || access.range.size>requirement.maximum_size || requirement.alignment==0 || access.range.offset%requirement.alignment!=0 { return {},.Invalid_Binding }
                }
                if !found { return {},.Missing_Binding }
            }
        }
        err:=prepare_packet_bindings(g,pass.packet,inputs,query,textures,graphics); if err!=.None { return {},err }
        append(&prepared.passes,Prepared_Pass{id,pass.kind,packet_prepare_clone(pass.packet,g.allocator)})
    }
    prepared.buffers=make([]Buffer_Input,len(inputs),g.allocator); copy(prepared.buffers,inputs)
    prepared.hazards=make([]Hazard,len(plan.hazards),g.allocator); copy(prepared.hazards,plan.hazards[:])
    success=true; return prepared,.None
}
