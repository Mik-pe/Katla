//! The application applies one completed Luau command stream against its canonical scene owners.
package app
import script "../script"
import ecs "../ecs"
import editor "../editor"
import box3d "../physics/box3d"
import "core:strings"
import "core:fmt"
import "core:slice"

/// Runs immutable local TRS/component/input snapshots, then commits deferred mutations in order.
script_native_step :: proc(app:^Authoring,delta_seconds:f32)->editor.Scene_Error {
    context.allocator=app.world.allocator
    if !finite_nonnegative(delta_seconds) { return .Invalid_Field_Value }
    owner:=ecs.get_resource_mut(&app.world,Script_Native_Runtime); if owner==nil { return .Application_Owned }
    sync_error:=script_native_sync(app)
    if sync_error!=.None {
        if app.mode!=.Playing || len(owner.runtime.instances)==0 { return sync_error }
        if len(owner.logs)<4096 { append(&owner.logs,script.Log{level=.Warn,message=fmt.aprintf("Script source reload rejected: %v; existing instances continue",sync_error)}) }
    }
    entities:=make([dynamic]script.Entity_State); defer { for entity in entities { delete(entity.components) }; delete(entities) }
    ids:=ecs.entity_ids(&app.world); defer delete(ids)
    for entity in ids {
        if _,hidden:=ecs.get_component(&app.world,entity,Editor_Hidden); hidden { continue }
        value:=script.Entity_State{id=u64(entity),without_transform=true}
        if label,present:=ecs.get_component(&app.world,entity,Scene_Name); present { value.name=label.name }
        if transform,present:=ecs.get_component(&app.world,entity,Scene_Transform); present { value.transform=transform.local; value.without_transform=false }
        if body,present:=ecs.get_component(&app.world,entity,Physics_Body); present && body.has_rigid_body { value.velocity=body.linear_velocity; value.has_velocity=true }
        names:=make([dynamic]string)
        for name,entry in app.registry.entries { if ecs.component_address(&app.world,entity,entry.T)!=nil { append(&names,name) } }
        value.components=slice.clone(names[:],app.world.allocator); delete(names); append(&entities,value)
    }
    input:script.Input
    if snapshot,present:=ecs.get_resource(&app.world,Script_Input); present && snapshot.focused && app.mode==.Playing { input={snapshot.actions,snapshot.keys,snapshot.mouse_delta,snapshot.mouse_wheel} }
    events:=make([dynamic]script.Event); defer delete(events)
    signals:=ecs.get_resource_mut(&app.world,Script_Signals)
    if signals!=nil { for signal in signals.pending { append(&events,script.Event{name=signal.name,trigger=u64(signal.trigger),other=u64(signal.other),payload=-1,animation_clip=signal.animation_clip,animation_loop_count=signal.animation_loop_count,has_animation=signal.has_animation}) } }
    output,failure:=script.tick(owner.runtime,delta_seconds,entities[:],events[:],input,owner.queries); defer script.output_destroy(owner.runtime,&output); defer delete(failure)
    if failure!="" { return .Invalid_Operation }
    script_input_consume_motion(app); script_native_queries_clear(owner)
    if signals!=nil { for signal in signals.pending { delete(signal.name); delete(signal.animation_clip) }; clear(&signals.pending) }
    for instance in output.instances {
        component:=ecs.get_component_mut(&app.world,ecs.Entity_Id(instance.entity),Script_Component); if component==nil { continue }
        component.disabled=instance.disabled; component.consecutive_errors=instance.consecutive_errors
        for error in component.last_errors { delete(error) }; clear(&component.last_errors)
        if component.last_errors.allocator.procedure==nil { component.last_errors=make([dynamic]string) }
    }
    for diagnostic in output.diagnostics { script_native_error(app,diagnostic.entity,diagnostic.error) }
    for log in output.logs {
        if len(owner.logs)>=4096 { delete(owner.logs[0].message); ordered_remove(&owner.logs,0) }
        append(&owner.logs,script.Log{log.entity,log.level,strings.clone(log.message)})
    }
    for command in output.commands { if error:=script_native_apply(app,owner,command); error!=.None { message:=fmt.aprintf("%v: %v",command.kind,error); script_native_error(app,command.owner,message); delete(message) } }
    return .None
}
@(private="package")
script_native_error :: proc(app:^Authoring,entity:u64,message:string) {
    component:=ecs.get_component_mut(&app.world,ecs.Entity_Id(entity),Script_Component); if component==nil { return }
    if component.last_errors.allocator.procedure==nil { component.last_errors=make([dynamic]string,app.world.allocator) }
    if len(component.last_errors)<4096 { append(&component.last_errors,strings.clone(message,app.world.allocator)) }
}
@(private="package")
script_native_apply :: proc(app:^Authoring,owner:^Script_Native_Runtime,command:script.Command)->editor.Scene_Error {
    if command.kind==.Emit { return .None }
    if command.kind==.Play_Sound_Cue { return audio_script_cue(app,command.name) }
    if command.kind==.Play_Sound || command.kind==.Play_Sound_At { return audio_script_play(app,command.path,command.volume,command.looping,command.origin,command.kind==.Play_Sound_At) }
    if command.kind==.Raycast {
        ray:=physics_raycast(app,command.origin,command.vector,command.max_distance); if ray.error!=.None { return ray.error }
        owner.queries.rays[{command.owner,command.index}]={ray.hit,u64(ray.entity),ray.point,ray.normal,ray.distance}; return .None
    }
    if command.kind==.Spawn_Entity { return script_native_spawn(app) }
    entity:=ecs.Entity_Id(command.entity)
    if !ecs.entity_exists(&app.world,entity) { return .Entity_Not_Found }
    if _,hidden:=ecs.get_component(&app.world,entity,Editor_Hidden); hidden { return .Protected_Entity }
    switch command.kind {
    case .Set_Transform,.Set_Position:
        target:=ecs.get_component_mut(&app.world,entity,Scene_Transform); if target==nil { return .Component_Not_Found }
        if command.kind==.Set_Position { target.local.position=command.vector } else { target.local=command.transform }; return .None
    case .Destroy_Entity: return script_native_remove(app,entity)
    case .Burst_Particles: return particle_burst(&app.world,entity,command.count)
    case .Set_Particles_Active: return particle_set_active(&app.world,entity,command.active)
    case .Apply_Force,.Apply_Impulse,.Set_Velocity:
        motion:=box3d.Motion.Force; if command.kind==.Apply_Impulse { motion=.Impulse }; if command.kind==.Set_Velocity { motion=.Set_Velocity }
        error:=physics_apply_motion(app,entity,motion,command.vector)
        return error
    case .Query_Trigger_Overlaps:
        backend:=ecs.get_resource_mut(&app.world,box3d.Backend); if backend==nil { return .Application_Owned }
        values,error:=box3d.backend_trigger_overlaps(backend,command.entity); if error!=.None { return .Invalid_Operation }
        owner.queries.overlaps[{command.owner,command.index}]=values; return .None
    case .Emit,.Spawn_Entity,.Play_Sound,.Play_Sound_At,.Play_Sound_Cue,.Raycast:
    }
    return .Invalid_Operation
}
