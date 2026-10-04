//! Authored scene staging validates particle capacity before native resources are published.
package render
import app ".."
import ecs "../../ecs"
import km "../../math"
import "core:math"

@(private="package")
particle_scene_limits :: proc(owner:^app.Authoring,selected:[]ecs.Entity_Id,states:[]Particle_Emitter_State,live_population:bool,capacity,emitter_capacity:u32)->Particle_Error {
    if owner==nil || capacity==0 || emitter_capacity==0 { return {code=.Invalid_Configuration} }
    count:=len(states) if live_population else 0
    for entity,i in selected {
        if !ecs.entity_exists(&owner.world,entity) { return {code=.Invalid_Configuration} }
        for previous in selected[:i] { if entity==previous { return {code=.Invalid_Configuration} } }
        emitter,present:=ecs.get_component(&owner.world,entity,app.Particle_Emitter); if !present { continue }
        if !app.particle_descriptor_valid(emitter.descriptor) { return {code=.Invalid_Configuration} }
        for burst in emitter.descriptor.burst_queue { if burst>capacity { return {code=.Particle_Capacity} } }
        world_transform,error:=app.scene_world_matrix(owner,entity); if error!=.None { return {code=.Invalid_Configuration} }
        for value in km.mat4_extract_translation(world_transform) { if math.is_nan(value) || math.is_inf(value) { return {code=.Invalid_Configuration} } }
        if !emitter.descriptor.active { continue }
        retained:=false
        if live_population { for state in states { if state.entity==entity { retained=true; break } } }
        if !retained { count+=1 }
        if count>int(emitter_capacity) { return {code=.Emitter_Capacity} }
    }
    return {}
}
/// Checks a candidate scene without consuming queues, reallocating GPU pools or advancing simulation.
particle_scene_validate :: proc(consumer:^Particle_Consumer($R),owner:Particle_Owner,selected:Particle_Selection)->Particle_Error {
    if consumer.owner!=owner || consumer.pending.ready { return {code=.Invalid_Configuration} }
    return particle_scene_limits(owner,selected,consumer.states,consumer.alive_upper!=0,consumer.capacity,consumer.emitter_capacity)
}
