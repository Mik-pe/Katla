//! Authored executable buffer work and backend-independent resource preflight.
package gfx

import "core:mem"

/// Binds one actual resource use to a shader slot.
Buffer_Binding :: struct { slot:u32, access:Buffer_Access }
/// Dispatch dimensions are workgroup counts; shader local size belongs to its pipeline.
Dispatch :: struct { pipeline:Pipeline_Handle, groups:[3]u32, bindings:[]Buffer_Binding }
/// Transfers bytes between declared buffer resources without hidden copies.
Copy_Buffer :: struct { source,destination:Resource_Id, source_offset,destination_offset,size:u64 }
/// Exactly one operation per pass; more work uses another ordinary pass.
Packet :: union { Dispatch, Copy_Buffer }
/// A reflected binding's native type, minimum span and offset alignment.
Binding_Requirement :: struct { slot:u32, usage:Buffer_Usage, minimum_size,alignment,maximum_size:u64 }
/// Borrowed reflection remains valid while the pipeline registry retains its owner.
Pipeline_Info :: struct { bindings:[]Binding_Requirement, local_size:[3]u32, max_threads:u64 }
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
Prepared_Graph :: struct { passes:[dynamic]Prepared_Pass, buffers:[]Buffer_Input, hazards:[]Hazard, allocator:mem.Allocator }
@(private="package")
packet_destroy :: proc(packet:^Packet,allocator:mem.Allocator) {
    #partial switch p in packet^ {
    case Dispatch: delete(p.bindings,allocator)
    }
    packet^={}
}
@(private="package")
packet_clone :: proc(packet:Packet,allocator:mem.Allocator)->Packet {
    #partial switch p in packet {
    case Dispatch:
        copy_packet:=p; copy_packet.bindings=make([]Buffer_Binding,len(p.bindings),allocator); copy(copy_packet.bindings,p.bindings)
        return copy_packet
    }
    return packet
}
/// Frees packet copies and arrays without destroying caller-selected native resources.
prepared_graph_destroy :: proc(prepared:^Prepared_Graph) {
    for &pass in prepared.passes { packet_destroy(&pass.packet,prepared.allocator) }
    delete(prepared.passes); delete(prepared.buffers,prepared.allocator); delete(prepared.hazards,prepared.allocator); prepared^={}
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
packet_accesses :: proc(packet:Packet,allocator:mem.Allocator)->[]Buffer_Access {
    switch p in packet {
    case Dispatch:
        result:=make([]Buffer_Access,len(p.bindings),allocator)
        for binding,i in p.bindings { result[i]=binding.access }
        return result
    case Copy_Buffer:
        result:=make([]Buffer_Access,2,allocator)
        result[0]={p.source,{p.source_offset,p.size},.Read,.Transfer_Source}
        result[1]={p.destination,{p.destination_offset,p.size},.Write,.Transfer_Destination}
        return result
    }
    return nil
}
/// Validates replacement work before copying it; failure preserves the previous packet.
graph_set_packet :: proc(g:^Buffer_Graph,id:Pass_Id,packet:Packet)->Packet_Error {
    if id.owner!=g || id.index<0 || id.index>=len(g.passes) { return .Invalid_Pass }
    pass:=&g.passes[id.index]
    switch p in packet {
    case Dispatch:
        if pass.kind!=.Compute || len(p.bindings)>32 { return .Invalid_Packet }
        for count in p.groups { if count==0 { return .Invalid_Dispatch } }
        for binding,i in p.bindings {
            for previous in p.bindings[:i] { if previous.slot==binding.slot { return .Invalid_Binding } }
        }
    case Copy_Buffer:
        if pass.kind!=.Transfer || p.size==0 { return .Invalid_Packet }
        if p.source==p.destination && range_overlaps({p.source_offset,p.size},{p.destination_offset,p.size}) { return .Invalid_Packet }
    case: return .Invalid_Packet
    }
    accesses:=packet_accesses(packet,g.allocator); defer delete(accesses,g.allocator)
    for access in accesses { if !access_declared(pass^,access) { return .Undeclared_Access } }
    // Initialization and exports rely on writes actually covering their declaration.
    for declaration in pass.accesses {
        found:=false
        for access in accesses { if access==declaration { found=true; break } }
        if !found { return .Undeclared_Access }
    }
    owned:=packet_clone(packet,g.allocator)
    packet_destroy(&pass.packet,g.allocator); pass.packet=owned; pass.has_packet=true
    return .None
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
graph_prepare :: proc(g:^Buffer_Graph,plan:^Compiled_Graph,inputs:[]Buffer_Input,query:Resource_Query)->(Prepared_Graph,Packet_Error) {
    if plan.owner!=g || plan.revision!=g.revision { return {},.Invalid_Plan }
    if query.buffer==nil || query.pipeline==nil { return {},.Unsupported_Query }
    prepared:=Prepared_Graph{passes=make([dynamic]Prepared_Pass,g.allocator),allocator=g.allocator}
    success:=false; defer { if !success { prepared_graph_destroy(&prepared) } }
    for input,i in inputs {
        id:=input.resource
        if id.owner!=g || id.index<0 || id.index>=len(g.buffers) { return {},.Missing_Resource }
        info,ok:=query.buffer(query.state,input.handle)
        if !ok || info.identity==nil { return {},.Invalid_Buffer }
        desc:=g.buffers[id.index].desc
        if info.desc.size<desc.size || desc.usage&info.desc.usage!=desc.usage { return {},.Invalid_Buffer }
        for previous in inputs[:i] {
            if previous.resource==id { return {},.Invalid_Buffer }
            previous_info,_:=query.buffer(query.state,previous.handle)
            if previous_info.identity==info.identity { return {},.Aliased_Resource }
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
            pipeline,ok:=query.pipeline(query.state,dispatch.pipeline)
            if !ok || len(pipeline.bindings)!=len(dispatch.bindings) { return {},.Invalid_Pipeline }
            threads:u64=1
            for count,i in dispatch.groups {
                local:=pipeline.local_size[i]
                if count==0 || count>query.max_groups[i] || local==0 || u64(local)>max(u64)/threads { return {},.Invalid_Dispatch }
                threads*=u64(local)
            }
            if threads>pipeline.max_threads { return {},.Invalid_Dispatch }
            for requirement in pipeline.bindings {
                found:=false
                for binding in dispatch.bindings {
                    if binding.slot!=requirement.slot { continue }
                    found=true
                    access:=binding.access
                    if access.usage!=requirement.usage || access.range.size<requirement.minimum_size || access.range.size>requirement.maximum_size || requirement.alignment==0 || access.range.offset%requirement.alignment!=0 { return {},.Invalid_Binding }
                }
                if !found { return {},.Missing_Binding }
            }
        }
        append(&prepared.passes,Prepared_Pass{id,pass.kind,packet_clone(pass.packet,g.allocator)})
    }
    prepared.buffers=make([]Buffer_Input,len(inputs),g.allocator); copy(prepared.buffers,inputs)
    prepared.hazards=make([]Hazard,len(plan.hazards),g.allocator); copy(prepared.hazards,plan.hazards[:])
    success=true; return prepared,.None
}
