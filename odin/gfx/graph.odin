//! Buffer graph compilation validates declared hazards without backend feature dispatch.
package gfx

import "core:mem"
import "core:strings"

/// Logical identities remain stable while passes are appended.
Resource_Id :: struct { owner:rawptr, index:int }
/// Identifies one declaration within its originating graph.
Pass_Id :: struct { owner:rawptr, index:int }
/// Distinguishes initialization writes from consumers and in-place work.
Access_Mode :: enum { Read, Write, Read_Write, None }
/// Buffer workloads available in this first graph compiler.
Pass_Kind :: enum { Compute, Transfer, Graphics }
/// Actual byte range and stage usage required by one pass.
Buffer_Access :: struct { resource:Resource_Id, range:Buffer_Range, mode:Access_Mode, usage:Buffer_Usage }
/// A dependency is a real overlapping hazard between two authored passes.
Hazard :: struct { before,after:Pass_Id, resource:Resource_Id, source,destination:Buffer_Access }
/// Typed graph rejection before mutation or native encoding.
Graph_Error :: enum { None, Invalid_Name, Invalid_Resource, Invalid_Range, Invalid_Usage, Duplicate_Access, Uninitialized_Read }
@(private="package")
Graph_Buffer :: struct { desc:Buffer_Desc, imported,exported:bool }
@(private="package")
Graph_Pass :: struct { name:string, kind:Pass_Kind, accesses:[]Buffer_Access, side_effect:bool, packet:Packet, has_packet:bool, images:[]Image_Access }
/// Owns declarations and authored packet copies; native allocations remain backend-owned.
Graph :: struct { buffers:[dynamic]Graph_Buffer, images:[dynamic]Graph_Image, passes:[dynamic]Graph_Pass, allocator:mem.Allocator, revision:u64 }
/// Owns deterministic live pass order and exact range hazards.
Compiled_Graph :: struct { order:[dynamic]Pass_Id, hazards:[dynamic]Hazard, image_hazards:[dynamic]Image_Hazard, owner:^Graph, revision:u64 }
/// Initializes a stationary graph independent of ECS, math, editor or scene resources.
graph_init :: proc(g:^Graph,allocator:=context.allocator) {
    g.allocator=allocator; g.buffers=make([dynamic]Graph_Buffer,allocator); g.images=make([dynamic]Graph_Image,allocator); g.passes=make([dynamic]Graph_Pass,allocator)
}
/// Releases names, accesses and packets after all compiled plan consumers finish.
graph_destroy :: proc(g:^Graph) {
    for &pass in g.passes { delete(pass.name,g.allocator); delete(pass.accesses,g.allocator); delete(pass.images,g.allocator); packet_destroy(&pass.packet,g.allocator) }
    delete(g.buffers); delete(g.images); delete(g.passes); g^={}
}
/// Releases a plan without modifying its graph or native resources.
compiled_graph_destroy :: proc(plan:^Compiled_Graph) { delete(plan.order); delete(plan.hazards); delete(plan.image_hazards); plan^={} }
/// Imports initialized bytes or declares an uninitialized transient buffer.
graph_buffer :: proc(g:^Graph,desc:Buffer_Desc,imported,exported:bool)->(Resource_Id,Graph_Error) {
    if !buffer_desc_valid(desc) { return {},.Invalid_Resource }
    id:=Resource_Id{g,len(g.buffers)}; append(&g.buffers,Graph_Buffer{desc,imported,exported}); g.revision+=1; return id,.None
}
@(private="package")
validate_accesses :: proc(g:^Graph,kind:Pass_Kind,accesses:[]Buffer_Access)->Graph_Error {
    for access,i in accesses {
        id:=access.resource
        if id.owner!=g || id.index<0 || id.index>=len(g.buffers) { return .Invalid_Resource }
        buffer:=g.buffers[id.index]
        if !range_valid(access.range,buffer.desc.size) { return .Invalid_Range }
        if !(access.usage in buffer.desc.usage) { return .Invalid_Usage }
        if kind==.Transfer {
            if !((access.mode==.Read && access.usage==.Transfer_Source) || (access.mode==.Write && access.usage==.Transfer_Destination)) { return .Invalid_Usage }
        } else if kind==.Compute && access.usage!=.Storage && !((access.mode==.Read || access.mode==.None) && (access.usage==.Uniform || access.usage==.Indirect)) { return .Invalid_Usage }
        else if kind==.Graphics && access.usage!=.Storage && !((access.mode==.Read || access.mode==.None) && (access.usage==.Uniform || access.usage==.Vertex || access.usage==.Index || access.usage==.Indirect)) { return .Invalid_Usage }
        for previous in accesses[:i] {
            if previous.resource==id && range_overlaps(previous.range,access.range) { return .Duplicate_Access }
        }
    }
    return .None
}
/// Validates fully before adding an owned pass declaration.
graph_pass :: proc(g:^Graph,name:string,kind:Pass_Kind,accesses:[]Buffer_Access,side_effect:=false,images:[]Image_Access=nil)->(Pass_Id,Graph_Error) {
    if len(name)==0 { return {},.Invalid_Name }
    for pass in g.passes { if pass.name==name { return {},.Invalid_Name } }
    err:=validate_accesses(g,kind,accesses); if err!=.None { return {},err }
    err=validate_image_accesses(g,kind,images); if err!=.None { return {},err }
    owned:=make([]Buffer_Access,len(accesses),g.allocator); copy(owned,accesses)
    owned_images:=make([]Image_Access,len(images),g.allocator); copy(owned_images,images)
    id:=Pass_Id{g,len(g.passes)}
    append(&g.passes,Graph_Pass{name=strings.clone(name,g.allocator),kind=kind,accesses=owned,side_effect=side_effect,images=owned_images})
    g.revision+=1
    return id,.None
}
/// Removes the exact last declaration, releases its owned packet and invalidates compiled plans.
/// Earlier pass and resource identities remain stable; retained native recordings own separate copies.
graph_remove_last_pass :: proc(g:^Graph,id:Pass_Id)->Graph_Error {
    if g==nil || id.owner!=g || id.index<0 || id.index!=len(g.passes)-1 { return .Invalid_Resource }
    pass:=&g.passes[id.index]
    delete(pass.name,g.allocator); delete(pass.accesses,g.allocator); delete(pass.images,g.allocator)
    packet_destroy(&pass.packet,g.allocator)
    pop(&g.passes); g.revision+=1
    return .None
}
@(private="package")
range_initialized :: proc(g:^Graph,pass_index:int,access:Buffer_Access)->bool {
    if g.buffers[access.resource.index].imported { return true }
    cursor:=access.range.offset; end:=cursor+access.range.size
    for cursor<end {
        next:=cursor
        for pass in g.passes[:pass_index] {
            for previous in pass.accesses {
                if previous.resource==access.resource && access_writes(previous.mode) && previous.range.offset<=cursor {
                    next=max(next,min(end,previous.range.offset+previous.range.size))
                }
            }
        }
        if next==cursor { return false }
        cursor=next
    }
    return true
}
/// Compiles authored order, culls dead work and retains every overlapping live hazard.
graph_compile :: proc(g:^Graph)->(Compiled_Graph,Graph_Error) {
    plan:=Compiled_Graph{order=make([dynamic]Pass_Id,g.allocator),hazards=make([dynamic]Hazard,g.allocator),image_hazards=make([dynamic]Image_Hazard,g.allocator),owner=g,revision=g.revision}
    success:=false; defer { if !success { compiled_graph_destroy(&plan) } }
    live:=make([]bool,len(g.passes),g.allocator); defer delete(live,g.allocator)
    for pass,i in g.passes {
        if error:=validate_accesses(g,pass.kind,pass.accesses); error!=.None { return {},error }
        if error:=validate_image_accesses(g,pass.kind,pass.images); error!=.None { return {},error }
        live[i]=pass.side_effect
        for access in pass.images {
            if g.images[access.resource.index].exported && access_writes(access.mode) { live[i]=true }
            if access_reads(access.mode) && !image_initialized(g,i,access) { return {},.Uninitialized_Read }
        }
        for access in pass.accesses {
            if g.buffers[access.resource.index].exported && access_writes(access.mode) { live[i]=true }
            if access_reads(access.mode) && !range_initialized(g,i,access) { return {},.Uninitialized_Read }
        }
    }
    for buffer,i in g.buffers {
        if buffer.exported && !range_initialized(g,len(g.passes),Buffer_Access{{g,i},{0,buffer.desc.size},.Read,.Readback}) { return {},.Uninitialized_Read }
    }
    for image,i in g.images {
        if image.exported && !image_initialized(g,len(g.passes),Image_Access{{g,i},image_full_range(image.desc),.Read,.Transfer_Source}) { return {},.Uninitialized_Read }
    }
    compile_liveness(g,live)
    for pass,i in g.passes {
        if !live[i] { continue }
        id:=Pass_Id{g,i}; append(&plan.order,id)
        for j in 0..<i {
            if !live[j] { continue }
            for a in g.passes[j].images {
                for b in pass.images {
                    if a.resource==b.resource && image_ranges_overlap(a.range,b.range) && access_conflict(a.mode,b.mode) { append(&plan.image_hazards,Image_Hazard{{g,j},id,a.resource,a,b}) }
                }
            }
            for a in g.passes[j].accesses {
                for b in pass.accesses {
                    if a.resource==b.resource && range_overlaps(a.range,b.range) && access_conflict(a.mode,b.mode) {
                        append(&plan.hazards,Hazard{{g,j},id,a.resource,a,b})
                    }
                }
            }
        }
    }
    success=true; return plan,.None
}

/// Query-only shader use retains an allocation without reading its contents.
access_reads :: proc(mode:Access_Mode)->bool { return mode==.Read || mode==.Read_Write }
/// Write participation is explicit rather than inferred from absence of reads.
access_writes :: proc(mode:Access_Mode)->bool { return mode==.Write || mode==.Read_Write }
/// Declared access must cover every actual reflected entry operation.
access_covers :: proc(declared,required:Access_Mode)->bool { return (!access_reads(required) || access_reads(declared)) && (!access_writes(required) || access_writes(declared)) }

/// Query-only bindings have no contents hazard with another access.
access_conflict :: proc(a,b:Access_Mode)->bool { return a!=.None && b!=.None && (access_writes(a) || access_writes(b)) }
