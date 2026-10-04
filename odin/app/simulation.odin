//! Explicit preview transitions snapshot authored state and restore fresh runtime identities.
package app

import scene "../agent/scene"
import ecs "../ecs"
import editor "../editor"
import "core:encoding/json"

/// Owns the pre-play authored scene; world components never contain this recursive owner.
Simulation_Runtime :: struct { snapshot:Scene_Snapshot, captured:bool, elapsed_seconds:f64, steps:u64 }
@(private="package")
simulation_runtime_destroy :: proc(value:rawptr) { runtime:=cast(^Simulation_Runtime)value; scene_snapshot_destroy(&runtime.snapshot); runtime^={} }
/// Installs the sole preview snapshot owner outside serializable scene components.
simulation_init :: proc(app:^Authoring) { ecs.insert_resource(&app.world,Simulation_Runtime{},ecs.Value_Ops{destroy=simulation_runtime_destroy}) }
@(private="package")
play_mode_name :: proc(mode:Play_Mode)->string {
    switch mode {
    case .Editing: return "editing"
    case .Playing: return "playing"
    case .Paused: return "paused"
    }
    return ""
}
/// Applies idempotent transitions; failed capture/restore leaves mode and snapshot intact.
simulation_execute :: proc(app:^Authoring,op:scene.Simulation_Op)->(editor.Tool_Result,editor.Undo_Group) {
    w:=&app.world; context.allocator=w.allocator; result:=error_result(w,.None)
    runtime:=ecs.get_resource_mut(w,Simulation_Runtime); if runtime==nil { result.error=.Invalid_Operation; return result,{} }
    previous:=app.mode; target:=previous
    switch op {
    case .Inspect:
    case .Play: if previous==.Editing { target=.Playing }
    case .Pause: if previous==.Playing { target=.Paused }
    case .Resume: if previous==.Paused { target=.Playing }
    case .Stop: target=.Editing
    }
    elapsed,steps:=runtime.elapsed_seconds,runtime.steps
    if previous==.Editing && target==.Playing { elapsed=0; steps=0 }
    data,err:=json.marshal(struct { mode:string,changed,runtime_ids_replaced:bool,elapsed_seconds:f64,steps:u64 }{play_mode_name(target),target!=previous,previous!=.Editing && target==.Editing,elapsed,steps},allocator=w.allocator)
    if err!=nil { result.error=.Decode_Failed; return result,{} }
    if target!=previous {
        if previous==.Editing {
            preflight:=physics_prepare(app); if preflight!=.None { delete(data); result.error=preflight; return result,{} }
            preflight=script_sync(app); if preflight!=.None { delete(data); result.error=preflight; return result,{} }
            snapshot,capture_error:=scene_snapshot_capture(app)
            if capture_error!=.None { delete(data); result.error=capture_error; return result,{} }
            reset_error:=physics_reset(app); if reset_error==.None { reset_error=script_reset(app) }; if reset_error!=.None { scene_snapshot_destroy(&snapshot); delete(data); result.error=reset_error; return result,{} }; events_reset(app)
            scene_snapshot_destroy(&runtime.snapshot); runtime.snapshot=snapshot; runtime.captured=true; runtime.elapsed_seconds=0; runtime.steps=0
        } else if target==.Editing {
            if !runtime.captured { delete(data); result.error=.Invalid_Operation; return result,{} }
            restore_error:=scene_snapshot_restore(app,&runtime.snapshot)
            if restore_error!=.None { delete(data); result.error=restore_error; return result,{} }
            reset_error:=physics_reset(app); if reset_error==.None { reset_error=script_reset(app) }; if reset_error!=.None { result.error=reset_error }; events_reset(app)
            scene_snapshot_destroy(&runtime.snapshot); runtime.captured=false
            session:=&app.agent.session; next_id,paused,finished:=session.next_id,session.paused,session.finished
            editor.agent_session_destroy(session); editor.agent_session_init(session,w.allocator); session.next_id=next_id; session.paused=paused; session.finished=finished
        }
        app.mode=target
    }
    result.data=data; return result,{}
}
/// Advances animation, completed physics transitions and sandboxed scripts only while playing.
simulation_step :: proc(app:^Authoring,delta_seconds:f32)->editor.Scene_Error {
    if !finite_nonnegative(delta_seconds) { return .Invalid_Field_Value }
    if app.mode!=.Playing || delta_seconds==0 { return .None }
    runtime:=ecs.get_resource_mut(&app.world,Simulation_Runtime); if runtime==nil || !runtime.captured { return .Invalid_Operation }
    if delta_seconds>0.25 { return .Invalid_Field_Value }
    event_error:=animation_events_preflight(app,delta_seconds)
    if event_error==.Invalid_Operation {
        signals:=ecs.get_resource_mut(&app.world,Script_Signals)
        if signals!=nil && len(signals.pending)>0 {
            retry_error:=script_step(app,0); if retry_error!=.None { return retry_error }
            event_error=animation_events_preflight(app,delta_seconds)
        }
    }
    if event_error!=.None { return event_error }
    animation_update(&app.world,delta_seconds)
    event_error=animation_events_dispatch(app); if event_error!=.None { return event_error }
    physics:=physics_step(app,delta_seconds); defer physics_step_result_destroy(&physics); if physics.error!=.None { return physics.error }
    for event in physics.events { events_dispatch(app,event) }
    script_error:=script_step(app,delta_seconds); if script_error!=.None { return script_error }
    runtime.elapsed_seconds+=f64(delta_seconds); runtime.steps+=1
    return .None
}
