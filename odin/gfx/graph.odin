//! Buffer graph compilation validates declared hazards without backend feature dispatch.
package gfx

import "core:mem"
import "core:strings"

/// Logical identities remain stable while passes are appended.
Resource_Id :: struct { owner:rawptr, index:int }
/// Identifies one declaration within its originating graph.
Pass_Id :: struct { owner:rawptr, index:int }
/// Distinguishes initialization writes from consumers and in-place work.
Access_Mode :: enum { Read, Write, Read_Write }
/// Buffer workloads available in this first graph compiler.
Pass_Kind :: enum { Compute, Transfer }
/// Actual byte range and stage usage required by one pass.
Buffer_Access :: struct { resource:Resource_Id, range:Buffer_Range, mode:Access_Mode, usage:Buffer_Usage }
/// A dependency is a real overlapping hazard between two authored passes.
Hazard :: struct { before,after:Pass_Id, resource:Resource_Id, source,destination:Buffer_Access }
/// Typed graph rejection before mutation or native encoding.
Graph_Error :: enum { None, Invalid_Name, Invalid_Resource, Invalid_Range, Invalid_Usage, Duplicate_Access, Uninitialized_Read }
@(private="package")
Graph_Buffer :: struct { desc:Buffer_Desc, imported,exported:bool }
@(private="package")
Graph_Pass :: struct { name:string, kind:Pass_Kind, accesses:[]Buffer_Access, side_effect:bool, packet:Packet, has_packet:bool }
/// Owns declarations and authored packet copies; native allocations remain backend-owned.
Buffer_Graph :: struct { buffers:[dynamic]Graph_Buffer, passes:[dynamic]Graph_Pass, allocator:mem.Allocator, revision:u64 }
/// Owns deterministic live pass order and exact range hazards.
Compiled_Graph :: struct { order:[dynamic]Pass_Id, hazards:[dynamic]Hazard, owner:^Buffer_Graph, revision:u64 }
/// Initializes a stationary graph independent of ECS, math, editor or scene resources.
graph_init :: proc(g:^Buffer_Graph,allocator:=context.allocator) {
    g.allocator=allocator; g.buffers=make([dynamic]Graph_Buffer,allocator); g.passes=make([dynamic]Graph_Pass,allocator)
}
/// Releases names, accesses and packets after all compiled plan consumers finish.
graph_destroy :: proc(g:^Buffer_Graph) {
    for &pass in g.passes { delete(pass.name,g.allocator); delete(pass.accesses,g.allocator); packet_destroy(&pass.packet,g.allocator) }
    delete(g.buffers); delete(g.passes); g^={}
}
/// Releases a plan without modifying its graph or native resources.
compiled_graph_destroy :: proc(plan:^Compiled_Graph) { delete(plan.order); delete(plan.hazards); plan^={} }
/// Imports initialized bytes or declares an uninitialized transient buffer.
graph_buffer :: proc(g:^Buffer_Graph,desc:Buffer_Desc,imported,exported:bool)->(Resource_Id,Graph_Error) {
    if desc.size==0 || desc.usage=={} { return {},.Invalid_Resource }
    id:=Resource_Id{g,len(g.buffers)}; append(&g.buffers,Graph_Buffer{desc,imported,exported}); g.revision+=1; return id,.None
}
@(private="package")
validate_accesses :: proc(g:^Buffer_Graph,kind:Pass_Kind,accesses:[]Buffer_Access)->Graph_Error {
    for access,i in accesses {
        id:=access.resource
        if id.owner!=g || id.index<0 || id.index>=len(g.buffers) { return .Invalid_Resource }
        buffer:=g.buffers[id.index]
        if !range_valid(access.range,buffer.desc.size) { return .Invalid_Range }
        if !(access.usage in buffer.desc.usage) { return .Invalid_Usage }
        if kind==.Transfer {
            if !((access.mode==.Read && access.usage==.Transfer_Source) || (access.mode==.Write && access.usage==.Transfer_Destination)) { return .Invalid_Usage }
        } else if access.usage!=.Storage && !(access.mode==.Read && access.usage==.Uniform) { return .Invalid_Usage }
        for previous in accesses[:i] {
            if previous.resource==id && range_overlaps(previous.range,access.range) { return .Duplicate_Access }
        }
    }
    return .None
}
/// Validates fully before adding an owned pass declaration.
graph_pass :: proc(g:^Buffer_Graph,name:string,kind:Pass_Kind,accesses:[]Buffer_Access,side_effect:=false)->(Pass_Id,Graph_Error) {
    if len(name)==0 { return {},.Invalid_Name }
    for pass in g.passes { if pass.name==name { return {},.Invalid_Name } }
    err:=validate_accesses(g,kind,accesses); if err!=.None { return {},err }
    owned:=make([]Buffer_Access,len(accesses),g.allocator); copy(owned,accesses)
    id:=Pass_Id{g,len(g.passes)}
    append(&g.passes,Graph_Pass{name=strings.clone(name,g.allocator),kind=kind,accesses=owned,side_effect=side_effect})
    g.revision+=1
    return id,.None
}
@(private="package")
range_initialized :: proc(g:^Buffer_Graph,pass_index:int,access:Buffer_Access)->bool {
    if g.buffers[access.resource.index].imported { return true }
    cursor:=access.range.offset; end:=cursor+access.range.size
    for cursor<end {
        next:=cursor
        for pass in g.passes[:pass_index] {
            for previous in pass.accesses {
                if previous.resource==access.resource && previous.mode!=.Read && previous.range.offset<=cursor {
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
graph_compile :: proc(g:^Buffer_Graph)->(Compiled_Graph,Graph_Error) {
    plan:=Compiled_Graph{order=make([dynamic]Pass_Id,g.allocator),hazards=make([dynamic]Hazard,g.allocator),owner=g,revision=g.revision}
    success:=false; defer { if !success { compiled_graph_destroy(&plan) } }
    live:=make([]bool,len(g.passes),g.allocator); defer delete(live,g.allocator)
    for pass,i in g.passes {
        live[i]=pass.side_effect
        for access in pass.accesses {
            if g.buffers[access.resource.index].exported && access.mode!=.Read { live[i]=true }
            if access.mode!=.Write && !range_initialized(g,i,access) { return {},.Uninitialized_Read }
        }
    }
    for buffer,i in g.buffers {
        if buffer.exported && !range_initialized(g,len(g.passes),Buffer_Access{{g,i},{0,buffer.desc.size},.Read,.Readback}) { return {},.Uninitialized_Read }
    }
    for i:=len(g.passes)-1; i>=0; i-=1 {
        if !live[i] { continue }
        for j in 0..<i {
            for a in g.passes[j].accesses {
                for b in g.passes[i].accesses {
                    if a.resource==b.resource && range_overlaps(a.range,b.range) && (a.mode!=.Read || b.mode!=.Read) {
                        live[j]=true
                    }
                }
            }
        }
    }
    for pass,i in g.passes {
        if !live[i] { continue }
        id:=Pass_Id{g,i}; append(&plan.order,id)
        for j in 0..<i {
            if !live[j] { continue }
            for a in g.passes[j].accesses {
                for b in pass.accesses {
                    if a.resource==b.resource && range_overlaps(a.range,b.range) && (a.mode!=.Read || b.mode!=.Read) {
                        append(&plan.hazards,Hazard{{g,j},id,a.resource,a,b})
                    }
                }
            }
        }
    }
    success=true; return plan,.None
}
