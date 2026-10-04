//! Native requirements and compiled live intervals define real transient allocation groups.
package gfx

import "core:mem"

/// Native memory requirements are queried before placement or handle publication.
Memory_Requirements :: struct { size,alignment:u64, memory_types:u32, domain:Memory_Domain }
Buffer_Allocation :: struct { resource:Resource_Id, desc:Buffer_Desc }
Image_Allocation :: struct { resource:Image_Id, desc:Texture_Desc }
Allocation_Request :: union { Buffer_Allocation, Image_Allocation }
Allocation_Handle :: union { Buffer_Handle, Texture_Handle }
/// A placement group owns overlapping native storage at offset zero.
Allocation_Group :: struct { size,alignment:u64, memory_types:u32, domain:Memory_Domain, resources:[]int }
Planned_Allocation :: struct { request:Allocation_Request, requirements:Memory_Requirements, first,last,group:int }
/// A stationary graph and revision identify the exact resource descriptions being allocated.
Allocation_Plan :: struct { owner:^Graph, revision:u64, resources:[dynamic]Planned_Allocation, groups:[dynamic]Allocation_Group, allocator:mem.Allocator }
/// Required native queries provide actual allocation sizes, alignment and compatible memory classes.
Allocation_Query :: struct { state:rawptr, buffer:proc(rawptr,Buffer_Desc)->(Memory_Requirements,Gpu_Error), texture:proc(rawptr,Texture_Desc)->(Memory_Requirements,Gpu_Error) }
/// Allocation transfers its resource handles and array allocator to the generic owner.
Allocation_Result :: struct { handles:[]Allocation_Handle, allocator:mem.Allocator }
Allocation_API :: struct {
    state:rawptr,
    allocate:proc(rawptr,Allocation_Group,[]Allocation_Request)->(Allocation_Result,Gpu_Error),
    destroy_buffer:proc(rawptr,Buffer_Handle)->Gpu_Error,
    destroy_texture:proc(rawptr,Texture_Handle)->Gpu_Error,
}
Allocated_Resource :: struct { request:Allocation_Request, handle:Allocation_Handle }
/// One owner supplies a slot's actual transient mappings; resize removes old public identities.
Graph_Allocations :: struct { resources:[dynamic]Allocated_Resource, buffers:[dynamic]Buffer_Input, textures:[dynamic]Texture_Input, api:Allocation_API, allocator:mem.Allocator }
/// Releases a CPU plan without changing allocated native resources.
allocation_plan_destroy :: proc(plan:^Allocation_Plan) {
    for group in plan.groups { delete(group.resources,plan.allocator) }
    delete(plan.groups); delete(plan.resources); plan^={}
}
@(private="package")
requirements_valid :: proc(requirements:Memory_Requirements)->bool { return requirements.size>0 && requirements.alignment>0 && requirements.memory_types!=0 }
@(private="package")
allocation_resource_id :: proc(request:Allocation_Request)->Alias_Resource {
    switch resource in request {
    case Buffer_Allocation: return resource.resource
    case Image_Allocation: return resource.resource
    }
    return {}
}
@(private="package")
plan_allocation :: proc(plan:^Allocation_Plan,compiled:^Compiled_Graph,request:Allocation_Request,requirements:Memory_Requirements)->Gpu_Error {
    if !requirements_valid(requirements) { return .Unsupported }
    first,last,_,_,used:=resource_lifetime(plan.owner,compiled,allocation_resource_id(request)); if !used { return .None }
    group_index:=-1
    for group,i in plan.groups {
        if group.domain!=requirements.domain || group.memory_types&requirements.memory_types==0 { continue }
        disjoint:=true
        for index in group.resources { prior:=plan.resources[index]; if !(prior.last<first || last<prior.first) { disjoint=false; break } }
        if disjoint { group_index=i; break }
    }
    if group_index<0 { group_index=len(plan.groups); append(&plan.groups,Allocation_Group{domain=requirements.domain,memory_types=requirements.memory_types}) }
    group:=&plan.groups[group_index]
    group.size=max(group.size,requirements.size); group.alignment=max(group.alignment,requirements.alignment); group.memory_types&=requirements.memory_types
    index:=len(plan.resources); append(&plan.resources,Planned_Allocation{request,requirements,first,last,group_index})
    members:=make([]int,len(group.resources)+1,plan.allocator); copy(members,group.resources); members[len(group.resources)]=index
    delete(group.resources,plan.allocator); group.resources=members
    return .None
}
/// Compiles native-compatible placement groups from the exact live transient resource intervals.
graph_allocation_plan :: proc(g:^Graph,compiled:^Compiled_Graph,query:Allocation_Query)->(Allocation_Plan,Gpu_Error) {
    if compiled.owner!=g || compiled.revision!=g.revision { return {},.Invalid_Graph }
    plan:=Allocation_Plan{owner=g,revision=g.revision,allocator=g.allocator,resources=make([dynamic]Planned_Allocation,g.allocator),groups=make([dynamic]Allocation_Group,g.allocator)}
    success:=false; defer { if !success { allocation_plan_destroy(&plan) } }
    for buffer,i in g.buffers {
        if buffer.imported { continue }
        request:=Buffer_Allocation{{g,i},buffer.desc}
        _,_,_,_,used:=resource_lifetime(g,compiled,request.resource); if !used { continue }
        if query.buffer==nil { return {},.Unsupported }
        requirements,error:=query.buffer(query.state,buffer.desc); if error!=.None { return {},error }
        if requirements.domain!=buffer.desc.memory { return {},.Unsupported }
        error=plan_allocation(&plan,compiled,request,requirements); if error!=.None { return {},error }
    }
    for image,i in g.images {
        if image.imported { continue }
        request:=Image_Allocation{{g,i},image.desc}
        _,_,_,_,used:=resource_lifetime(g,compiled,request.resource); if !used { continue }
        if query.texture==nil { return {},.Unsupported }
        requirements,error:=query.texture(query.state,image.desc); if error!=.None { return {},error }
        if requirements.domain!=.GPU_Private { return {},.Unsupported }
        error=plan_allocation(&plan,compiled,request,requirements); if error!=.None { return {},error }
    }
    for &group in plan.groups {
        remainder:=group.size%group.alignment
        if remainder!=0 {
            extra:=group.alignment-remainder
            if extra>max(u64)-group.size { return {},.Allocation_Failed }
            group.size+=extra
        }
    }
    success=true; return plan,.None
}
/// Removes owned handles; a failed removal leaves the remaining owner available for retry.
graph_allocations_destroy :: proc(allocations:^Graph_Allocations)->Gpu_Error {
    for &resource in allocations.resources {
        #partial switch handle in resource.handle {
        case Buffer_Handle:
            error:=allocations.api.destroy_buffer(allocations.api.state,handle); if error!=.None { return error }
        case Texture_Handle:
            error:=allocations.api.destroy_texture(allocations.api.state,handle); if error!=.None { return error }
        }
        resource.handle={}
    }
    delete(allocations.resources); delete(allocations.buffers); delete(allocations.textures); allocations^={}
    return .None
}
/// Allocates one independent slot owner; a failed rollback returns its remaining owner for cleanup.
graph_allocate :: proc(plan:^Allocation_Plan,api:Allocation_API)->(Graph_Allocations,Gpu_Error) {
    if plan.owner==nil || plan.revision!=plan.owner.revision { return {},.Invalid_Graph }
    if api.allocate==nil || api.destroy_buffer==nil || api.destroy_texture==nil { return {},.Unsupported }
    owner:=Graph_Allocations{api=api,allocator=plan.allocator,resources=make([dynamic]Allocated_Resource,plan.allocator),buffers=make([dynamic]Buffer_Input,plan.allocator),textures=make([dynamic]Texture_Input,plan.allocator)}
    error:Gpu_Error
    for group in plan.groups {
        requests:=make([]Allocation_Request,len(group.resources),plan.allocator)
        for index,i in group.resources { requests[i]=plan.resources[index].request }
        result,native_error:=api.allocate(api.state,group,requests)
        if native_error!=.None || len(result.handles)!=len(requests) { error=native_error==.None ? .Native_Failure : native_error }
        for handle,i in result.handles {
            request:Allocation_Request
            if i>=len(requests) { error=.Native_Failure } else { request=requests[i] }
            valid:=false
            #partial switch h in handle {
            case Buffer_Handle: valid=h.owner!=nil
            case Texture_Handle: valid=h.owner!=nil
            }
            if !valid { error=.Native_Failure; continue }
            append(&owner.resources,Allocated_Resource{request,handle})
            switch resource in request {
            case Buffer_Allocation:
                buffer,ok:=handle.(Buffer_Handle); if !ok || buffer.owner==nil { error=.Native_Failure; continue }
                append(&owner.buffers,Buffer_Input{resource.resource,buffer})
            case Image_Allocation:
                texture,ok:=handle.(Texture_Handle); if !ok || texture.owner==nil { error=.Native_Failure; continue }
                append(&owner.textures,Texture_Input{resource.resource,texture})
            }
        }
        delete(result.handles,result.allocator); delete(requests,plan.allocator)
        if error!=.None { break }
    }
    if error!=.None {
        rollback:=graph_allocations_destroy(&owner)
        if rollback!=.None { return owner,rollback }
        return {},error
    }
    return owner,.None
}
