//! Timeline commands share the same validated player and local-pose state used by rendered models.
package app

import scene "../agent/scene"
import ecs "../ecs"
import editor "../editor"
import "core:mem"

/// Pause preserves a pending fade; stop resets clocks and clears it while retaining the selected clip.
/// Seek clamps the source clock and resets completion, preserving pending target time and loop events.
animation_control :: proc(w:^ecs.World,op:scene.Animation_Op)->editor.Scene_Error {
    if !ecs.entity_exists(w,op.entity) { return .Entity_Not_Found }
    _,hidden:=ecs.get_component(w,op.entity,Editor_Hidden); if hidden { return .Protected_Entity }
    if op.action==.Play || op.action==.Fade { return animation_play(w,op.entity,op.clip,op.fade_seconds,op.looping,op.speed) }
    model:=ecs.get_component_mut(w,op.entity,Animation_Model); if model==nil { return .Component_Not_Found }
    if !animation_model_valid(model) { return .Invalid_Operation }
    player:=ecs.get_component_mut(w,op.entity,Animation_Player); if player==nil { return .Component_Not_Found }
    if op.action!=.Stop && (!animation_sample_state_valid(player) || !finite_nonnegative(player.duration) || !finite_nonnegative(player.speed) || player.blending&&(!finite_nonnegative(player.target_duration)||!finite_nonnegative(player.blend_duration)||!finite_nonnegative(player.blend_time))) { return .Invalid_Operation }
    context.allocator=w.allocator
    switch op.action {
    case .Pause: player.playing=false
    case .Resume:
        source:=animation_clip(model,player.clip)
        if source==nil || !animation_sample_state_valid(player) { return .Invalid_Operation }
        if player.blending && animation_clip(model,player.target_clip)==nil { return .Invalid_Operation }
        player.duration=source.duration; player.playing=true
    case .Stop:
        if !finite_nonnegative(player.duration)||!finite_nonnegative(player.speed) {return .Invalid_Operation}
        player.playing=false; player.time=0; player.loop_count=0; player.completed=false; animation_clear_transition(player)
    case .Seek:
        if !finite_nonnegative(abs(op.time_seconds)) { return .Invalid_Field_Value }
        source:=animation_clip(model,player.clip); if source==nil { return .Invalid_Operation }
        player.duration=source.duration; player.time=clamp(op.time_seconds,0,source.duration); player.completed=false
    case .Speed:
        if !finite_nonnegative(op.speed) { return .Invalid_Field_Value }; player.speed=op.speed
    case .Loop: player.looping=op.looping
    case .Inspect,.Play,.Fade: return .Invalid_Operation
    }
    return .None
}
/// Editing timeline previews advance actual poses; Playing advances through simulation_step and Paused freezes.
animation_editor_step :: proc(owner:^Authoring,delta_seconds:f32)->editor.Scene_Error {
    if !finite_nonnegative(delta_seconds) || delta_seconds>0.25 { return .Invalid_Field_Value }
    if owner.mode==.Editing { animation_update(&owner.world,delta_seconds) }
    return .None
}

@(private="package")
Animation_Setting_Command :: struct {entity:ecs.Entity_Id,action:scene.Animation_Action,before_speed,after_speed:f32,before_loop,after_loop:bool}
@(private="package")
animation_setting_apply :: proc(state:rawptr,w:^ecs.World,_:^editor.Component_Registry,redo:bool,_:^[dynamic]editor.Entity_Remap)->editor.Scene_Error {
    command:=cast(^Animation_Setting_Command)state
    speed:=command.after_speed if redo else command.before_speed
    looping:=command.after_loop if redo else command.before_loop
    return animation_control(w,{action=command.action,entity=command.entity,speed=speed,looping=looping})
}
@(private="package")
animation_setting_destroy :: proc(state:rawptr,allocator:mem.Allocator) {free(state,allocator)}
@(private="package")
animation_setting_remap :: proc(state:rawptr,remap:editor.Entity_Remap) {command:=cast(^Animation_Setting_Command)state;if command.entity==remap.before {command.entity=remap.after}}
@(private="package")
animation_setting_history :: proc(owner:^Authoring,op:scene.Animation_Op,before_speed:f32,before_loop:bool)->editor.Undo_Group {
    player:=ecs.get_component_mut(&owner.world,op.entity,Animation_Player)
    if owner.mode!=.Editing || player==nil || (op.action!=.Speed&&op.action!=.Loop) {return {}}
    if op.action==.Speed&&before_speed==player.speed||op.action==.Loop&&before_loop==player.looping {return {}}
    command:=new(Animation_Setting_Command,owner.world.allocator)
    command^={op.entity,op.action,before_speed,player.speed,before_loop,player.looping}
    return editor.undo_group_create(command,{animation_setting_apply,animation_setting_destroy,animation_setting_remap},{op.entity},owner.world.allocator)
}
