//! Owner-thread publication retains old native pipelines until their exact snapshots release.
package shader

import "core:mem"
import "core:strings"
import "core:sync"

/// Native preparation must finish before publication and own everything borrowed from Compiled.
Publisher :: struct {
    state:rawptr,
    prepare:proc(rawptr,^Compiled)->(rawptr,bool),
    destroy:proc(rawptr,rawptr),
}
/// A candidate can fail without altering its currently published pipeline.
Publication_Error :: enum { None, Compile_Failed, Prepare_Failed, Superseded, Closed, Invalid_Snapshot, Busy }
/// One publication outcome owns its optional compiler diagnostic.
Publication :: struct { key,revision:u64, error:Publication_Error, compiler_error:Error, message:string, allocator:mem.Allocator }
@(private="package")
Version :: struct { key,revision:u64, pipeline:rawptr, references:int }
/// Owner-thread native registry; worker threads only see its separate Service mailbox.
Registry :: struct {
    service:^Service,
    publisher:Publisher,
    current:map[u64]^Version,
    snapshots:map[u64]^Version,
    next_snapshot:u64,
    allocator:mem.Allocator,
}
/// Exact retained version for frame authoring; copied or stale snapshots cannot release twice.
Snapshot :: struct { owner:^Registry, token:u64, key,revision:u64, pipeline:rawptr }
/// Initializes a native owner with mandatory fallible preparation and destruction callbacks.
registry_init :: proc(registry:^Registry,service:^Service,publisher:Publisher,allocator:=context.allocator)->Publication_Error {
    if service==nil || publisher.prepare==nil || publisher.destroy==nil || registry.service!=nil { return .Closed }
    registry.service=service; registry.publisher=publisher; registry.allocator=allocator
    registry.current=make(map[u64]^Version,allocator); registry.snapshots=make(map[u64]^Version,allocator)
    return .None
}
@(private="package")
version_release :: proc(registry:^Registry,version:^Version) {
    version.references-=1
    if version.references==0 { registry.publisher.destroy(registry.publisher.state,version.pipeline); free(version,registry.allocator) }
}
/// Frees an outcome without touching published pipelines.
publication_destroy :: proc(result:^Publication) { delete(result.message,result.allocator); result^={} }
/// Publishes one latest successful native candidate, preserving old work on every failure.
registry_poll :: proc(registry:^Registry)->(Publication,bool) {
    if registry.service==nil { return Publication{error=.Closed},false }
    candidate,available:=service_take_result(registry.service)
    if !available { return {},false }; defer replacement_destroy(&candidate)
    result:=Publication{key=candidate.key,revision=candidate.revision,allocator=registry.allocator}
    if candidate.error!=.None {
        result.error=.Compile_Failed; result.compiler_error=candidate.error
        result.message=strings.clone(candidate.compiled.message,registry.allocator)
        return result,true
    }
    if !service_is_latest(registry.service,candidate.key,candidate.revision) { result.error=.Superseded; return result,true }
    native,ready:=registry.publisher.prepare(registry.publisher.state,&candidate.compiled)
    if !ready || native==nil {
        if native!=nil { registry.publisher.destroy(registry.publisher.state,native) }
        result.error=.Prepare_Failed; return result,true
    }
    version:=new(Version,registry.allocator)
    version^={candidate.key,candidate.revision,native,1}
    sync.mutex_lock(&registry.service.mutex)
    latest,current:=registry.service.latest[candidate.key]
    if !current || latest!=candidate.revision {
        sync.mutex_unlock(&registry.service.mutex)
        free(version,registry.allocator); registry.publisher.destroy(registry.publisher.state,native)
        result.error=.Superseded; return result,true
    }
    previous,present:=registry.current[candidate.key]
    registry.current[candidate.key]=version
    sync.mutex_unlock(&registry.service.mutex)
    if present { version_release(registry,previous) }
    return result,true
}
/// Retains the actual current native version while a graph or frame borrows its identity.
registry_acquire :: proc(registry:^Registry,key:u64)->(Snapshot,Publication_Error) {
    if registry.service==nil || registry.next_snapshot==max(u64) { return {},.Closed }
    version,present:=registry.current[key]; if !present { return {},.Closed }
    registry.next_snapshot+=1; version.references+=1
    registry.snapshots[registry.next_snapshot]=version
    return Snapshot{registry,registry.next_snapshot,key,version.revision,version.pipeline},.None
}
/// Releases one exact snapshot; native destruction follows the final retained version owner.
registry_release :: proc(registry:^Registry,snapshot:^Snapshot)->Publication_Error {
    if snapshot.owner!=registry || snapshot.token==0 { return .Invalid_Snapshot }
    version,present:=registry.snapshots[snapshot.token]
    if !present || version.key!=snapshot.key || version.revision!=snapshot.revision || version.pipeline!=snapshot.pipeline { return .Invalid_Snapshot }
    delete_key(&registry.snapshots,snapshot.token); version_release(registry,version)
    snapshot^={}; return .None
}
/// Rejects destruction until every authored-frame snapshot has released its version.
registry_destroy :: proc(registry:^Registry)->Publication_Error {
    if len(registry.snapshots)!=0 { return .Busy }
    for key,version in registry.current {
        service_cancel(registry.service,key)
        version_release(registry,version)
    }
    delete(registry.current); delete(registry.snapshots)
    registry.service=nil; registry.publisher={}; registry.current=nil; registry.snapshots=nil
    return .None
}
