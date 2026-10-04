//! Bounded asynchronous WGSL replacement candidates with latest-revision publication.
package shader

import "core:mem"
import "core:strings"
import "core:sync"
import "core:thread"

/// Submission errors leave both the current pipeline and the latest accepted request intact.
Service_Error :: enum { None, Invalid_Request, Full, Closed, Thread_Failed, Busy }
/// An accepted immutable replacement candidate; ownership transfers to the scene/render owner.
Replacement :: struct { key,revision:u64, compiled:Compiled, error:Error, owner:^Service }
@(private="package")
Job :: struct { key,revision:u64, source:string, selections:[]Selection, constants:[]Constant }
/// Stationary bounded compiler worker; its allocator must support concurrent calls.
Service :: struct {
    compiler:^Compiler,
    worker:^thread.Thread,
    mutex:sync.Mutex,
    changed:sync.Cond,
    jobs:[dynamic]Job,
    results:[dynamic]Replacement,
    latest:map[u64]u64,
    capacity,outstanding,borrowed:int,
    next_revision:u64,
    closed:bool,
    allocator:mem.Allocator,
}
@(private="package")
job_destroy :: proc(job:^Job,allocator:mem.Allocator) {
    delete(job.source,allocator)
    for selected in job.selections { delete(selected.name,allocator) }
    for constant in job.constants { delete(constant.name,allocator) }
    delete(job.selections,allocator); delete(job.constants,allocator); job^={}
}
/// Releases a candidate after native preparation, failed preparation or stale discard.
replacement_destroy :: proc(replacement:^Replacement) {
    if replacement.owner!=nil {
        service:=replacement.owner
        sync.mutex_lock(&service.mutex)
        latest,present:=service.latest[replacement.key]
        if present && latest==replacement.revision { delete_key(&service.latest,replacement.key) }
        service.borrowed-=1
        sync.mutex_unlock(&service.mutex)
    }
    compiled_destroy(&replacement.compiled); replacement^={}
}
@(private="package")
worker_main :: proc(th:^thread.Thread) {
    service:=cast(^Service)th.data
    context.allocator=service.allocator
    for {
        sync.mutex_lock(&service.mutex)
        for len(service.jobs)==0 && !service.closed { sync.cond_wait(&service.changed,&service.mutex) }
        if len(service.jobs)==0 { sync.mutex_unlock(&service.mutex); break }
        job:=service.jobs[0]
        ordered_remove(&service.jobs,0)
        latest,present:=service.latest[job.key]
        if !present || latest!=job.revision {
            service.outstanding-=1
            sync.mutex_unlock(&service.mutex)
            job_destroy(&job,service.allocator)
            continue
        }
        sync.mutex_unlock(&service.mutex)
        artifact,err:=compile(service.compiler,job.source,job.selections,job.constants,service.allocator)
        result:=Replacement{key=job.key,revision=job.revision,compiled=artifact,error=err}
        job_destroy(&job,service.allocator)
        sync.mutex_lock(&service.mutex)
        append(&service.results,result)
        sync.cond_broadcast(&service.changed)
        sync.mutex_unlock(&service.mutex)
    }
}
/// Starts one worker using an already-loaded compiler; accepted results reserve their capacity.
service_init :: proc(service:^Service,compiler:^Compiler,capacity:int=32,allocator:=context.allocator)->Service_Error {
    if service.worker!=nil || compiler==nil || capacity<1 || capacity>256 { return .Invalid_Request }
    context.allocator=allocator
    service.compiler=compiler; service.capacity=capacity; service.allocator=allocator
    service.jobs=make([dynamic]Job,0,capacity,allocator)
    service.results=make([dynamic]Replacement,0,capacity,allocator)
    service.latest=make(map[u64]u64,allocator)
    worker:=thread.create(worker_main,name="WGSL compiler")
    if worker==nil { service_destroy(service); return .Thread_Failed }
    worker.data=service; service.worker=worker; thread.start(worker)
    return .None
}
/// Clones one bounded request; revisions are monotonic and only accepted requests supersede work.
service_submit :: proc(service:^Service,key:u64,source:string,selections:[]Selection,constants:[]Constant=nil)->(u64,Service_Error) {
    if key==0 || len(source)==0 || len(source)>MAX_REQUEST || len(selections)==0 || len(selections)>16 || len(constants)>1024 { return 0,.Invalid_Request }
    for selected,i in selections {
        if len(selected.name)==0 { return 0,.Invalid_Request }
        for previous in selections[:i] { if selected==previous { return 0,.Invalid_Request } }
    }
    for constant,i in constants {
        if len(constant.name)==0 || !(constant.value>= -max(f64) && constant.value<=max(f64)) { return 0,.Invalid_Request }
        for previous in constants[:i] { if constant.name==previous.name { return 0,.Invalid_Request } }
    }
    sync.mutex_lock(&service.mutex); defer sync.mutex_unlock(&service.mutex)
    if service.closed || service.worker==nil { return 0,.Closed }
    if service.outstanding>=service.capacity { return 0,.Full }
    if service.next_revision==max(u64) { return 0,.Closed }
    service.next_revision+=1
    job:=Job{key=key,revision=service.next_revision,source=strings.clone(source,service.allocator),selections=make([]Selection,len(selections),service.allocator),constants=make([]Constant,len(constants),service.allocator)}
    for selected,i in selections { job.selections[i]={strings.clone(selected.name,service.allocator),selected.stage} }
    for constant,i in constants { job.constants[i]={strings.clone(constant.name,service.allocator),constant.value} }
    service.latest[key]=job.revision
    append(&service.jobs,job); service.outstanding+=1
    sync.cond_signal(&service.changed)
    return job.revision,.None
}
/// Invalidates queued or compiling candidates for a key; published native work remains untouched.
service_cancel :: proc(service:^Service,key:u64)->bool {
    sync.mutex_lock(&service.mutex); defer sync.mutex_unlock(&service.mutex)
    _,present:=service.latest[key]
    if present { delete_key(&service.latest,key) }
    return present
}
/// Tests a candidate again immediately before the owner publishes its native replacement.
service_is_latest :: proc(service:^Service,key,revision:u64)->bool {
    sync.mutex_lock(&service.mutex); defer sync.mutex_unlock(&service.mutex)
    latest,present:=service.latest[key]
    return present && latest==revision
}
/// Transfers the next current candidate and silently releases superseded artifacts.
service_take_result :: proc(service:^Service)->(Replacement,bool) {
    sync.mutex_lock(&service.mutex); defer sync.mutex_unlock(&service.mutex)
    for len(service.results)>0 {
        result:=service.results[0]; ordered_remove(&service.results,0); service.outstanding-=1
        latest,present:=service.latest[result.key]
        if present && latest==result.revision { service.borrowed+=1; result.owner=service; return result,true }
        replacement_destroy(&result)
    }
    return {},false
}
/// Closes admission while the worker finishes already accepted candidates.
service_close :: proc(service:^Service) {
    sync.mutex_lock(&service.mutex); service.closed=true
    sync.cond_broadcast(&service.changed); sync.mutex_unlock(&service.mutex)
}
/// Joins the actual worker before releasing requests, results or its compiler pointer.
service_destroy :: proc(service:^Service)->Service_Error {
    sync.mutex_lock(&service.mutex)
    if service.borrowed!=0 { sync.mutex_unlock(&service.mutex); return .Busy }
    sync.mutex_unlock(&service.mutex)
    service_close(service)
    if service.worker!=nil { thread.join(service.worker); thread.destroy(service.worker); service.worker=nil }
    for &job in service.jobs { job_destroy(&job,service.allocator) }
    for &result in service.results { replacement_destroy(&result) }
    delete(service.jobs); delete(service.results); delete(service.latest)
    service.compiler=nil; service.jobs=nil; service.results=nil; service.latest=nil
    return .None
}
