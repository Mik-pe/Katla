//! A reset stages a new GPU pool and publishes it only with the shared accepted particle submission.
package render

import gfx "../../gfx"
import "core:log"

/// Queues one global clear for the consumer shared by all views. Repeated unprepared requests coalesce.
/// A reset frame consumes no authored bursts or emission time; following frames resume unchanged emitters.
particle_reset_all :: proc(consumer:^Particle_Consumer($R))->Particle_Error {
    if consumer==nil || consumer.renderer==nil || len(consumer.slots)==0 { return {code=.Invalid_Configuration} }
    if consumer.pending.ready { return {gpu=.Busy} }
    consumer.reset_requested=true; return {}
}
@(private="package")
particle_reset_cleanup_error :: proc(error:gfx.Gpu_Error) { log.error("Particle reset native pool cleanup failed",error) }
@(private="package")
particle_reset_release :: proc(consumer:^Particle_Consumer($R),handle:gfx.Buffer_Handle) {
    if handle.owner==nil { return }
    error:=consumer.operations.destroy_buffer(consumer.renderer,handle)
    if error!=.None { particle_reset_cleanup_error(error) }
}
@(private="package")
particle_reset_prepare :: proc(consumer:^Particle_Consumer($R))->(Particle_Preparation,Particle_Error) {
    result:=Particle_Preparation{reset=true,states=make([]Particle_Emitter_State,len(consumer.states),consumer.allocator)}
    copy(result.states,consumer.states)
    success:=false
    defer { if !success { particle_reset_release(consumer,result.reset_data); particle_reset_release(consumer,result.reset_dead); particle_reset_release(consumer,result.reset_counters); particle_preparation_destroy(&result,consumer.allocator) } }
    zero:=make([]byte,int(consumer.capacity)*64,consumer.allocator); defer delete(zero,consumer.allocator)
    indices:=make([]u32,int(consumer.capacity),consumer.allocator); defer delete(indices,consumer.allocator)
    for &index,i in indices { index=u32(i) }
    usage:=gfx.Buffer_Usages{.Storage,.Transfer_Source,.Transfer_Destination}
    error:gfx.Gpu_Error
    result.reset_data,error=consumer.operations.create_buffer(consumer.renderer,{size=u64(len(zero)),usage=usage,memory=.GPU_Private},zero); if error!=.None { return {},{gpu=error} }
    result.reset_dead,error=consumer.operations.create_buffer(consumer.renderer,{size=u64(consumer.capacity)*4,usage=usage,memory=.GPU_Private},particle_indices_bytes(indices)); if error!=.None { return {},{gpu=error} }
    counters:=[1]Particle_Counters{{dead=consumer.capacity}}
    result.reset_counters,error=consumer.operations.create_buffer(consumer.renderer,{size=16,usage=usage,memory=.GPU_Private},particle_counters_bytes(counters[:])); if error!=.None { return {},{gpu=error} }
    success=true; return result,{}
}
@(private="package")
particle_reset_discard :: proc(consumer:^Particle_Consumer($R)) {
    particle_reset_release(consumer,consumer.pending.reset_data); particle_reset_release(consumer,consumer.pending.reset_dead); particle_reset_release(consumer,consumer.pending.reset_counters)
    consumer.pending.reset_data={}; consumer.pending.reset_dead={}; consumer.pending.reset_counters={}
}
@(private="package")
particle_reset_commit :: proc(consumer:^Particle_Consumer($R)) {
    prepared:=&consumer.pending
    if !prepared.reset { return }
    old_data,old_dead,old_counters:=consumer.data,consumer.dead,consumer.rollover_counters
    consumer.data,consumer.dead,consumer.rollover_counters=prepared.reset_data,prepared.reset_dead,prepared.reset_counters
    prepared.reset_data={}; prepared.reset_dead={}; prepared.reset_counters={}; consumer.reset_requested=false
    consumer.observed_sequence=consumer.sequence-1; consumer.observed_alive=0; consumer.alive_upper=0
    clear(&consumer.records)
    particle_reset_release(consumer,old_data); particle_reset_release(consumer,old_dead); particle_reset_release(consumer,old_counters)
}
