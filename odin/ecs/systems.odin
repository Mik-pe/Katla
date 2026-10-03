//! Ordered conflict batches, bounded worker parameters and exclusive caller-thread systems.
package ecs

import "core:mem"
import "core:thread"

/// Reports registration, preparation or recoverable tick failures.
System_Error :: enum { None, Invalid_Access, Missing_Resource, Failed }
FIRST :: min(i32)
EARLY :: i32(-1000)
NORMAL :: i32(0)
LATE :: i32(1000)
LAST :: max(i32)
@(private="package")
System_Entry :: struct {
    state,adapter,prepared:rawptr,
    params:[dynamic]Param_Entry,
    claims:[dynamic]Claim,
    order:i32,
    enabled:bool,
    exclusive:bool,
    run:proc(^System_Entry,^World,f32)->System_Error,
    destroy:proc(^System_Entry,mem.Allocator),
    error:System_Error,
}
@(private="package")
Typed_Adapter :: struct($S:typeid,$P:typeid) { run:proc(^S,^P,f32)->System_Error, shutdown:proc(^S) }
@(private="package")
Exclusive_Adapter :: struct($S:typeid) { run:proc(^S,^World,f32)->System_Error, shutdown:proc(^S) }
/// Identifies a registered system until its World clears systems.
System_Handle :: distinct ^System_Entry

/// Derives claims from P; duplicate writes/read-write aliases reject registration.
register_typed_system :: proc(w:^World,state:$S,$P:typeid,callback:proc(^S,^P,f32)->System_Error,
                              order:=NORMAL,initialize:proc(^S)=nil,shutdown:proc(^S)=nil) -> (System_Handle,System_Error) {
    assert(!w.execution_active && !w.frozen)
    s:=new(System_Entry,w.allocator)
    s^=System_Entry{order=order,enabled=true,params=make([dynamic]Param_Entry,w.allocator),claims=make([dynamic]Claim,w.allocator)}
    s.prepared=allocate(size_of(P),align_of(P),w.allocator)
    mem.zero(s.prepared,size_of(P))
    if !params_init(w,s,P) { params_destroy(s,w.allocator); delete(s.claims); free(s,w.allocator); return nil,.Invalid_Access }
    st:=new(S,w.allocator); st^=state; s.state=st
    adapter:=new(Typed_Adapter(S,P),w.allocator)
    adapter^=Typed_Adapter(S,P){callback,shutdown}; s.adapter=adapter
    s.run=proc(entry:^System_Entry,_:^World,dt:f32)->System_Error {
        a:=cast(^Typed_Adapter(S,P))entry.adapter
        return a.run(cast(^S)entry.state,cast(^P)entry.prepared,dt)
    }
    s.destroy=proc(entry:^System_Entry,allocator:mem.Allocator) {
        a:=cast(^Typed_Adapter(S,P))entry.adapter
        if a.shutdown!=nil { a.shutdown(cast(^S)entry.state) }
        free(cast(^S)entry.state,allocator); free(a,allocator)
    }
    if initialize!=nil { initialize(st) }
    insert_system(w,s)
    return System_Handle(s),.None
}
/// Exclusive callbacks remain on the caller thread and may mutate structure.
register_exclusive_system :: proc(w:^World,state:$S,callback:proc(^S,^World,f32)->System_Error,
                                  order:=NORMAL,initialize:proc(^S)=nil,shutdown:proc(^S)=nil) -> System_Handle {
    assert(!w.execution_active && !w.frozen)
    s:=new(System_Entry,w.allocator)
    s^=System_Entry{order=order,enabled=true,exclusive=true}
    st:=new(S,w.allocator); st^=state; s.state=st
    a:=new(Exclusive_Adapter(S),w.allocator); a^=Exclusive_Adapter(S){callback,shutdown}; s.adapter=a
    s.run=proc(entry:^System_Entry,w:^World,dt:f32)->System_Error {
        a:=cast(^Exclusive_Adapter(S))entry.adapter
        return a.run(cast(^S)entry.state,w,dt)
    }
    s.destroy=proc(entry:^System_Entry,allocator:mem.Allocator) {
        a:=cast(^Exclusive_Adapter(S))entry.adapter
        if a.shutdown!=nil { a.shutdown(cast(^S)entry.state) }
        free(cast(^S)entry.state,allocator); free(a,allocator)
    }
    if initialize!=nil { initialize(st) }
    insert_system(w,s)
    return System_Handle(s)
}
/// Enables or disables a handle that remains registered in its World.
set_system_enabled :: proc(handle:System_Handle,enabled:bool) { entry:=cast(^System_Entry)handle; entry.enabled=enabled }
@(private="package")
insert_system :: proc(w:^World,s:^System_Entry) {
    append(&w.systems,s)
    i:=len(w.systems)-1
    for i>0 && w.systems[i-1].order>s.order { w.systems[i]=w.systems[i-1]; i-=1 }
    w.systems[i]=s
}
clear_systems :: proc(w:^World) {
    context.allocator=w.allocator
    if w.execution_active { w.clear_systems_requested=true; return }
    for s in w.systems {
        s.destroy(s,w.allocator)
        if !s.exclusive { params_destroy(s,w.allocator); delete(s.claims) }
        free(s,w.allocator)
    }
    clear(&w.systems); w.clear_systems_requested=false
}
@(private="package")
systems_conflict :: proc(a,b:^System_Entry) -> bool {
    if a.order!=b.order || a.exclusive || b.exclusive { return true }
    for x in a.claims { for y in b.claims { if claim_conflicts(x,y) { return true } } }
    return false
}
@(private="package")
System_Job :: struct { system:^System_Entry, dt:f32 }
@(private="package")
system_worker :: proc(task:thread.Task) {
    job:=cast(^System_Job)task.data
    job.system.error=job.system.run(job.system,nil,job.dt)
}
// Uses the same ordered schedule for sequential and parallel execution.
/// Recoverable callback failure joins workers and discards unapplied batch commands.
world_update :: proc(w:^World,dt:f32,parallel:=false) -> System_Error {
    assert(!w.execution_active && !w.frozen)
    w.execution_active=true
    defer {
        w.frozen=false; w.execution_active=false
        if w.clear_systems_requested { clear_systems(w) }
    }
    done:=make([]bool,len(w.systems),w.allocator)
    defer delete(done,w.allocator)
    batch:=make([dynamic]^System_Entry,w.allocator)
    defer delete(batch)
    jobs:=make([dynamic]System_Job,w.allocator)
    defer delete(jobs)
    remaining:=len(w.systems)
    for remaining>0 {
        clear(&batch)
        for s,i in w.systems {
            if done[i] { continue }
            blocked:=false
            for j in 0..<i { if !done[j] && systems_conflict(w.systems[j],s) { blocked=true; break } }
            if !blocked { append(&batch,s) }
        }
        assert(len(batch)>0)
        clear(&jobs)
        for s in batch {
            if !s.enabled { continue }
            if s.exclusive {
                err:=s.run(s,w,dt)
                if err!=.None { return err }
            } else {
                if !params_prepare(w,s) {
                    for other in batch { params_commands(other,w,false) }
                    return .Missing_Resource
                }
                append(&jobs,System_Job{s,dt})
            }
        }
        w.frozen=true
        use_pool:=parallel && w.pool!=nil && len(jobs)>1 && w.live_count*len(jobs)>=w.parallel_work_threshold
        if use_pool {
            for &job in jobs { thread.pool_add_task(w.pool,w.allocator,system_worker,&job) }
            completed:=0
            for completed<len(jobs) {
                _,ok:=thread.pool_pop_done(w.pool)
                if ok { completed+=1 } else { thread.yield() }
            }
        } else {
            for job in jobs { job.system.error=job.system.run(job.system,nil,dt) }
        }
        w.frozen=false
        err:=System_Error.None
        for job in jobs { if job.system.error!=.None && err==.None { err=job.system.error } }
        for s in batch { params_commands(s,w,err==.None) }
        if err!=.None { return err }
        if w.clear_systems_requested { break }
        for s,i in w.systems {
            for member in batch { if s==member { done[i]=true; remaining-=1; break } }
        }
    }
    clear(&w.entity_events); clear(&w.component_events); clear_changed(w)
    return .None
}
