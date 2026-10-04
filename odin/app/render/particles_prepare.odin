//! Burst admission and rate accumulators change only after the graph submission is accepted.
package render

import app ".."
import ecs "../../ecs"
import gfx "../../gfx"
import km "../../math"
import "core:math"
import "core:mem"
import "core:log"

@(private="package")
particle_delta_valid :: proc(delta:f32)->bool { return !math.is_nan(delta) && !math.is_inf(delta) && delta>=0 }
@(private="package")
particle_queue_changed :: proc(entity:ecs.Entity_Id) { log.error("Accepted particle submission lost its exclusive queue snapshot",entity) }
/// Encodes validated authored factors with initialized WGSL alignment padding.
particle_config :: proc(descriptor:app.Particle_Descriptor,position:[3]f32)->Particle_Config {
    return {position=position,shape=u32(descriptor.shape),emit_rate=descriptor.emit_rate,base_lifetime=descriptor.base_lifetime,lifetime_variation=descriptor.lifetime_variation,velocity_direction=descriptor.velocity_direction,velocity_magnitude=descriptor.velocity_magnitude,velocity_cone_angle=descriptor.velocity_cone_angle,base_scale=descriptor.base_scale,scale_variation=descriptor.scale_variation,color=descriptor.color,color_variation=descriptor.color_variation,color_end=descriptor.color_end,shape_params=descriptor.shape_params,gravity=descriptor.gravity,turbulence_strength=descriptor.turbulence_strength,turbulence_frequency=descriptor.turbulence_frequency,scale_end=descriptor.scale_end}
}
@(private="package")
particle_preparation_destroy :: proc(prepared:^Particle_Preparation,allocator:mem.Allocator) {
    for burst in prepared.bursts { delete(burst.counts,allocator) }
    delete(prepared.bursts,allocator); delete(prepared.states,allocator); delete(prepared.indices,allocator); prepared^={}
}
@(private="package")
particle_plan :: proc(owner:^app.Authoring,previous:[]Particle_Emitter_State,available,emitter_capacity:u32,delta:f32,allocator:=context.allocator,pool_capacity:u32=1_048_576,reclaim:=false)->(Particle_Preparation,Particle_Error) {
    if owner==nil || !particle_delta_valid(delta) { return {},{code=.Invalid_Configuration} }
    result:=Particle_Preparation{delta=delta}
    success:=false; defer { if !success { particle_preparation_destroy(&result,allocator) } }
    states:=make([dynamic]Particle_Emitter_State,allocator); defer delete(states)
    for state in previous {
        if reclaim {
            emitter,present:=ecs.get_component(&owner.world,state.entity,app.Particle_Emitter)
            if !present || !emitter.descriptor.active { continue }
        }
        append(&states,state)
    }
    for &state in states { state.active=false; if state.kill_on_destroy { state.config.kill_all=1 }; state.config.emit_rate=0 }
    bursts:=make([dynamic]Particle_Burst,allocator); defer delete(bursts)
    own_bursts:=true; defer { if own_bursts { for burst in bursts { delete(burst.counts,allocator) } } }
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    requested_bursts:u64
    for entity in ids {
        emitter,present:=ecs.get_component(&owner.world,entity,app.Particle_Emitter); if !present { continue }
        descriptor:=emitter.descriptor
        if !app.particle_descriptor_valid(descriptor) { return {},{code=.Invalid_Configuration} }
        if !descriptor.active {
            for &state in states { if state.entity==entity { state.configured_active=false } }; continue
        }
        world_matrix,error:=app.scene_world_matrix(owner,entity); if error!=.None { return {},{code=.Invalid_Configuration} }
        index:= -1
        for state,i in states { if state.entity==entity { index=i; break } }
        if index<0 {
            if len(states)>=int(emitter_capacity) { return {},{code=.Emitter_Capacity} }
            index=len(states); append(&states,Particle_Emitter_State{entity=entity})
        }
        position:=km.mat4_extract_translation(world_matrix)
        for value in position { if math.is_nan(value) || math.is_inf(value) { return {},{code=.Invalid_Configuration} } }
        state:=&states[index]; state.config=particle_config(descriptor,position); state.active=true; state.kill_on_destroy=descriptor.kill_on_destroy
        restart:=!state.clock_initialized || !state.configured_active || state.configured_timed!=descriptor.has_timed_emission || state.configured_duration!=descriptor.timed_emission || state.emission_revision!=descriptor.emission_revision
        if restart { state.remaining_duration=descriptor.timed_emission; state.accumulator=0 }
        state.clock_initialized=true; state.configured_active=descriptor.active; state.configured_timed=descriptor.has_timed_emission; state.configured_duration=descriptor.timed_emission; state.emission_revision=descriptor.emission_revision
        effective:=delta
        if descriptor.has_timed_emission { effective=min(effective,state.remaining_duration); if state.remaining_duration<=0 { state.active=false } }
        state.accumulator+=f64(descriptor.emit_rate)*f64(effective)
        if len(descriptor.burst_queue)>0 {
            counts:=make([]u32,len(descriptor.burst_queue),allocator); copy(counts,descriptor.burst_queue[:]); append(&bursts,Particle_Burst{entity,counts})
            for count in counts { requested_bursts+=u64(count) }
        }
    }
    if requested_bursts>u64(available) {
        result.deferred=true
        for &state in states {
            state.accumulator=0
            for old in previous { if old.entity==state.entity { state.accumulator=old.accumulator; break } }
        }
    }
    remaining_budget:=available; blocked:=false; requested_bursts=0
    for &burst in bursts {
        prefix:=0
        for count in burst.counts {
            if count>pool_capacity { return {},{code=.Particle_Capacity} }
            if blocked || count>remaining_budget { blocked=true; continue }
            remaining_budget-=count; requested_bursts+=u64(count); prefix+=1
        }
        selected:=make([]u32,prefix,allocator); copy(selected,burst.counts[:prefix])
        delete(burst.counts,allocator); burst.counts=selected
    }
    for i:=len(bursts)-1; i>=0; i-=1 { if len(bursts[i].counts)==0 { delete(bursts[i].counts,allocator); ordered_remove(&bursts,i) } }
    indices:=make([dynamic]u32,0,int(available),allocator); defer delete(indices)
    for burst in bursts {
        emitter_index:u32
        for state,i in states { if state.entity==burst.entity { emitter_index=u32(i); break } }
        for count in burst.counts { for _ in 0..<count { append(&indices,emitter_index) } }
    }
    for &state,i in states {
        if !state.active || result.deferred { continue }
        remaining:=u32(available)-u32(len(indices))
        emitted:=u32(min(math.floor(state.accumulator),f64(remaining)))
        state.accumulator-=f64(emitted)
        for _ in 0..<emitted { append(&indices,u32(i)) }
    }
    result.requested=u32(len(indices)); result.burst_count=u32(requested_bursts)
    result.states=make([]Particle_Emitter_State,len(states),allocator); copy(result.states,states[:])
    result.indices=make([]u32,len(indices),allocator); copy(result.indices,indices[:])
    result.bursts=make([]Particle_Burst,len(bursts),allocator); copy(result.bursts,bursts[:]); own_bursts=false
    success=true; return result,{}
}
/// Observes exact completed per-slot GPU counters; later accepted spawns keep their reservation.
particle_observe :: proc(consumer:^Particle_Consumer($R))->Particle_Error {
    for slot in consumer.slots {
        if slot.sequence<=consumer.observed_sequence { continue }
        counters:Particle_Counters
        values:=[1]Particle_Counters{counters}
        error:=consumer.operations.read_buffer(consumer.renderer,slot.readback,0,mem.slice_to_bytes(values[:]))
        if error==.Busy { continue }
        if error!=.None { return {gpu=error} }
        counters=values[0]
        if counters.alive>consumer.capacity || counters.dead>consumer.capacity || counters.alive+counters.dead!=consumer.capacity { return {gpu=.Native_Failure} }
        consumer.observed_sequence=slot.sequence; consumer.observed_alive=counters.alive
    }
    upper:=u64(consumer.observed_alive)
    for i:=len(consumer.records)-1; i>=0; i-=1 {
        record:=consumer.records[i]
        if record.sequence<=consumer.observed_sequence { ordered_remove(&consumer.records,i) }
        else { upper+=u64(record.requested) }
    }
    if upper>u64(consumer.capacity) { return {gpu=.Native_Failure} }
    consumer.alive_upper=u32(upper); return {}
}
/// Frees staged CPU inputs without changing queues, committed simulation state or acquired tokens.
particle_abort :: proc(consumer:^Particle_Consumer($R)) { particle_reset_discard(consumer); particle_preparation_destroy(&consumer.pending,consumer.allocator) }
/// Publishes rollover only after native queue acceptance and consumes precisely the staged queue prefix.
particle_committed :: proc(consumer:^Particle_Consumer($R),submission:gfx.Submission)->Particle_Error {
    prepared:=&consumer.pending
    if !prepared.ready || submission.token!=prepared.token { return {code=.Invalid_Frame} }
    error:=Particle_Error{}
    for burst in prepared.bursts {
        emitter:=ecs.get_component_mut(&consumer.owner.world,burst.entity,app.Particle_Emitter)
        match:=emitter!=nil && len(emitter.descriptor.burst_queue)>=len(burst.counts)
        if match { for count,i in burst.counts { if emitter.descriptor.burst_queue[i]!=count { match=false; break } } }
        if !match { error={code=.Queue_Changed}; particle_queue_changed(burst.entity); continue }
        if len(emitter.descriptor.burst_queue)==len(burst.counts) { consumed:=app.particle_take_bursts(emitter); delete(consumed) }
        else { for _ in burst.counts { ordered_remove(&emitter.descriptor.burst_queue,0) } }
    }
    for &state in prepared.states {
        if !state.active || prepared.deferred || !state.configured_timed { continue }
        state.remaining_duration=max(f32(0),state.remaining_duration-prepared.delta)
        if state.remaining_duration==0 { state.active=false }
    }
    delete(consumer.states,consumer.allocator); consumer.states=prepared.states; prepared.states=nil
    consumer.sequence+=1; particle_reset_commit(consumer); consumer.slots[prepared.token.slot].sequence=consumer.sequence
    append(&consumer.records,Particle_Record{consumer.sequence,prepared.requested})
    consumer.alive_upper+=prepared.requested; consumer.previous_slot=prepared.token.slot
    particle_abort(consumer); return error
}

/// Observes the last accepted emitter clock; authored duration and activation stay in the scene.
Particle_Emitter_Status :: struct { active,timed,finished:bool,remaining_duration:f32,sequence:u64 }
particle_emitter_status :: proc(consumer:^Particle_Consumer($R),entity:ecs.Entity_Id)->(Particle_Emitter_Status,bool) {
    for state in consumer.states { if state.entity==entity { return {active=state.active,timed=state.configured_timed,finished=state.configured_timed && state.remaining_duration==0,remaining_duration=state.remaining_duration,sequence=consumer.sequence},true } }
    return {},false
}
