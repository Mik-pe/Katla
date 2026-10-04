//! Owned animation clips drive a single timeline and sampled local-pose contract.
package app

import scene "../agent/scene"
import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:slice"
import "core:strings"
import "core:math"
import "core:encoding/json"
import "core:fmt"

/// Channels use glTF TRS conventions and xyzw quaternion values.
Animation_Path :: enum { Translation, Rotation, Scale, Weights }
/// Cubic spline values contain incoming tangent, value and outgoing tangent triplets.
Animation_Interpolation :: enum { Step, Linear, Cubic_Spline }
Animation_Channel :: struct { node:u32, path:Animation_Path, interpolation:Animation_Interpolation, times:[]f32, values:[][4]f32, weight_count:u32,weight_values:[]f32 }
Animation_Clip :: struct { name:string, duration:f32, channels:[]Animation_Channel }
/// Parents form an acyclic skeleton; -1 denotes a root and inverse binds are optional.
Animation_Model :: struct { clips:[]Animation_Clip `inspect:"skip"`, bind_pose:[]km.Transform `inspect:"skip"`, parents:[]i32 `inspect:"skip"`, inverse_bind:[]km.Mat4 `inspect:"skip"` }
Animation_Event_Kind :: enum { Completed, Looped }
Animation_Event :: struct { kind:Animation_Event_Kind, clip:string, loop_count:u32 }
/// Source weight reaches zero at transition completion, then resets to one for the target.
Animation_Player :: struct {
    clip:string `inspect:"skip"`, duration,time:f32 `inspect:"skip"`, playing,looping:bool `inspect:"skip"`, speed:f32 `inspect:"skip"`,
    target_clip:string `inspect:"skip"`, target_duration,target_time:f32 `inspect:"skip"`, target_looping:bool `inspect:"skip"`,
    blend_duration,blend_time,blend_weight:f32 `inspect:"skip"`, blending,completed,target_completed:bool `inspect:"skip"`,
    loop_count,target_loop_count:u32 `inspect:"skip"`, events:[dynamic]Animation_Event `inspect:"skip"`,
}
/// Creates a stopped player with identity playback parameters.
animation_player_stopped :: proc()->Animation_Player { return {speed=1,blend_weight=1} }
@(private="package")
animation_model_destroy :: proc(value:rawptr) {
    model:=cast(^Animation_Model)value
    for &clip in model.clips { delete(clip.name); for channel in clip.channels { delete(channel.times); delete(channel.values); delete(channel.weight_values) }; delete(clip.channels) }
    delete(model.clips); delete(model.bind_pose); delete(model.parents); delete(model.inverse_bind); model^={}
}
@(private="package")
animation_model_clone :: proc(dst,src:rawptr) {
    source:=cast(^Animation_Model)src; model:=cast(^Animation_Model)dst
    model^={clips=make([]Animation_Clip,len(source.clips)),bind_pose=slice.clone(source.bind_pose),parents=slice.clone(source.parents),inverse_bind=slice.clone(source.inverse_bind)}
    for clip,i in source.clips { model.clips[i]={name=strings.clone(clip.name),duration=clip.duration,channels=make([]Animation_Channel,len(clip.channels))}; for channel,j in clip.channels { model.clips[i].channels[j]=channel; model.clips[i].channels[j].times=slice.clone(channel.times); model.clips[i].channels[j].values=slice.clone(channel.values); model.clips[i].channels[j].weight_values=slice.clone(channel.weight_values) } }
}
@(private="package")
animation_player_destroy :: proc(value:rawptr) {
    player:=cast(^Animation_Player)value; delete(player.clip); delete(player.target_clip); for event in player.events { delete(event.clip) }; delete(player.events); player^={}
}
@(private="package")
animation_player_clone :: proc(dst,src:rawptr) {
    source:=cast(^Animation_Player)src; player:=cast(^Animation_Player)dst; player^=source^; player.clip=strings.clone(source.clip); player.target_clip=strings.clone(source.target_clip)
    player.events=make([dynamic]Animation_Event,0,len(source.events)); for event in source.events { copied:=event; copied.clip=strings.clone(event.clip); append(&player.events,copied) }
}
/// Registers deep ownership; application spawn chooses whether either component is present.
animation_register :: proc(w:^ecs.World,reg:^editor.Component_Registry) {
    editor.editor_register(w,reg,"AnimationModel",Animation_Model{},ecs.Value_Ops{animation_model_destroy,animation_model_clone},spawn_default=false)
    editor.editor_register(w,reg,"AnimationPlayer",animation_player_stopped(),ecs.Value_Ops{animation_player_destroy,animation_player_clone},spawn_default=false)
}
@(private="package")
animation_clip :: proc(model:^Animation_Model,name:string)->^Animation_Clip { for &clip in model.clips { if clip.name==name { return &clip } }; return nil }
@(private="package")
finite_nonnegative :: proc(value:f32)->bool { return value>=0 && !math.is_nan(value) && !math.is_inf(value) }
@(private="package")
animation_transform_valid :: proc(pose:km.Transform)->bool {
    if !km.quat_is_normalized(pose.rotation) { return false }
    for vector in ([2]km.Vec3{pose.position,pose.scale}) { for value in vector { if !finite_nonnegative(abs(value)) { return false } } }
    return true
}
@(private="package")
animation_sample_state_valid :: proc(player:^Animation_Player)->bool {
    if player==nil { return true }
    if !finite_nonnegative(player.time) { return false }
    return !player.blending || (finite_nonnegative(player.target_time) && finite_nonnegative(player.blend_weight) && player.blend_weight<=1)
}
/// Validates asset timelines, channel cardinality, unit rotations and skeleton topology.
animation_model_valid :: proc(model:^Animation_Model)->bool {
    count:=len(model.bind_pose); if len(model.parents)!=count || (len(model.inverse_bind)!=0 && len(model.inverse_bind)!=count) { return false }
    for parent,i in model.parents { if parent < -1 || parent>=i32(count) || parent==i32(i) { return false }; steps:=0; cursor:=parent; for cursor>=0 { if steps>=count { return false }; cursor=model.parents[cursor]; steps+=1 } }
    for pose in model.bind_pose { if !animation_transform_valid(pose) { return false } }
    for inverse_bind in model.inverse_bind { for column in inverse_bind { for value in column { if !finite_nonnegative(abs(value)) { return false } } } }
    for clip,i in model.clips {
        if strings.trim_space(clip.name)=="" || !finite_nonnegative(clip.duration) { return false }; for previous in model.clips[:i] { if previous.name==clip.name { return false } }
        for channel,j in clip.channels {
            if int(channel.node)>=count || len(channel.times)==0 || channel.path not_in (bit_set[Animation_Path]{.Translation,.Rotation,.Scale,.Weights}) || channel.interpolation not_in (bit_set[Animation_Interpolation]{.Step,.Linear,.Cubic_Spline}) { return false }
            for previous in clip.channels[:j] { if previous.node==channel.node && previous.path==channel.path { return false } }
            stride:=3 if channel.interpolation==.Cubic_Spline else 1
            if channel.path==.Weights {
                if channel.weight_count==0 || channel.weight_count>4096 || len(channel.values)!=0 || u64(len(channel.weight_values))!=u64(len(channel.times))*u64(stride)*u64(channel.weight_count) { return false }
                for weight in channel.weight_values { if !finite_nonnegative(abs(weight)) { return false } }
            } else if len(channel.values)!=len(channel.times)*stride || channel.weight_count!=0 || len(channel.weight_values)!=0 { return false }
            for time,k in channel.times { if !finite_nonnegative(time) || time>clip.duration || (k>0 && time<=channel.times[k-1]) { return false } }
            for value,k in channel.values { for v in value { if math.is_nan(v) || math.is_inf(v) { return false } }; if channel.path==.Rotation && (stride==1 || k%3==1) && !km.quat_is_normalized(km.Quat(value)) { return false } }
        }
    }
    return true
}
@(private="package")
animation_clear_transition :: proc(player:^Animation_Player) {
    delete(player.target_clip); player.target_clip=""; player.target_duration=0; player.target_time=0; player.target_completed=false; player.target_looping=false; player.target_loop_count=0; player.blend_duration=0; player.blend_time=0; player.blending=false; player.blend_weight=1
}
/// Preflights every referenced clip and timing parameter before replacing playback state.
animation_play :: proc(w:^ecs.World,entity:ecs.Entity_Id,name:string,fade_seconds:f32,looping:bool,speed:f32)->editor.Scene_Error {
    context.allocator=w.allocator
    if !ecs.entity_exists(w,entity) { return .Entity_Not_Found }; _,hidden:=ecs.get_component(w,entity,Editor_Hidden); if hidden { return .Protected_Entity }
    model:=ecs.get_component_mut(w,entity,Animation_Model); if model==nil { return .Component_Not_Found }
    if !finite_nonnegative(fade_seconds) || !finite_nonnegative(speed) { return .Invalid_Field_Value }
    if !animation_model_valid(model) { return .Invalid_Operation }
    clip:=animation_clip(model,name); if clip==nil || !finite_nonnegative(clip.duration) { return .Invalid_Operation }
    player:=ecs.get_component_mut(w,entity,Animation_Player)
    if player!=nil && fade_seconds>0 {
        if player.blending { return .Invalid_Operation }
        if player.clip!="" { source:=animation_clip(model,player.clip); if source==nil || !finite_nonnegative(source.duration) { return .Invalid_Operation } }
    }
    if player==nil { ecs.add_component(w,entity,animation_player_stopped()); player=ecs.get_component_mut(w,entity,Animation_Player) }
    owned_name:=strings.clone(name)
    if fade_seconds==0 || player.clip=="" {
        delete(player.clip); player.clip=owned_name; player.duration=clip.duration; player.time=0; player.completed=false; player.loop_count=0; player.looping=looping; animation_clear_transition(player)
    } else {
        player.target_clip=owned_name; player.target_duration=clip.duration; player.target_time=0; player.target_completed=false; player.target_loop_count=0; player.target_looping=looping; player.blend_duration=fade_seconds; player.blend_time=0; player.blend_weight=1; player.blending=true
    }
    player.speed=speed; player.playing=true; return .None
}
@(private="package")
animation_advance_clock :: proc(name:string,time:^f32,duration:f32,looping:bool,completed:^bool,loop_count:^u32,advance:f64,events:^[dynamic]Animation_Event) {
    if completed^ { return }; end:=f64(max(duration,0)); next:=f64(time^)+advance
    if looping {
        if end==0 { time^=0 } else if next>=end { loops:=u32(min(math.floor(next/end),f64(max(u32)))); time^=f32(math.mod(next,end)); loop_count^=u32(min(u64(loop_count^)+u64(loops),u64(max(u32)))); append(events,Animation_Event{.Looped,strings.clone(name),loop_count^}) } else { time^=f32(next) }
    } else { time^=f32(min(next,end)); if next>=end { completed^=true; append(events,Animation_Event{.Completed,strings.clone(name),0}) } }
}
/// Advances source and target clocks with finite deltas; fades advance even at zero playback speed.
animation_update :: proc(w:^ecs.World,delta_seconds:f32) {
    context.allocator=w.allocator; if !finite_nonnegative(delta_seconds) || delta_seconds==0 { return }
    ids:=ecs.entity_ids(w); defer delete(ids)
    for entity in ids {
        model:=ecs.get_component_mut(w,entity,Animation_Model); player:=ecs.get_component_mut(w,entity,Animation_Player)
        if model==nil || player==nil || !player.playing || !finite_nonnegative(player.speed) { continue }
        if source:=animation_clip(model,player.clip); source!=nil { player.duration=source.duration }
        advance:=f64(delta_seconds)*f64(player.speed)
        if player.clip!="" { animation_advance_clock(player.clip,&player.time,player.duration,player.looping,&player.completed,&player.loop_count,advance,&player.events) }
        if player.blending {
            if target:=animation_clip(model,player.target_clip); target!=nil { player.target_duration=target.duration }
            animation_advance_clock(player.target_clip,&player.target_time,player.target_duration,player.target_looping,&player.target_completed,&player.target_loop_count,advance,&player.events)
            player.blend_time=f32(min(f64(player.blend_time)+f64(delta_seconds),f64(player.blend_duration)))
            if player.blend_time>=player.blend_duration {
                delete(player.clip); player.clip=player.target_clip; player.target_clip=""; player.duration=player.target_duration; player.time=player.target_time; player.completed=player.target_completed; player.looping=player.target_looping; player.loop_count=player.target_loop_count; animation_clear_transition(player)
            } else { player.blend_weight=1-player.blend_time/player.blend_duration }
        }
        if !player.blending && player.completed { player.playing=false }
    }
}
/// Returns owned completion/loop feedback and drains the player's event mailbox.
animation_take_events :: proc(player:^Animation_Player)->[dynamic]Animation_Event { result:=player.events; player.events=nil; return result }
/// Releases drained event names with the allocator used by the scene owner.
animation_events_destroy :: proc(events:^[dynamic]Animation_Event,allocator:=context.allocator) { for event in events^ { delete(event.clip,allocator) }; delete(events^); events^=nil }
/// Executes actual playback or inspection on the application owner thread.
animation_execute :: proc(app:^Authoring,op:scene.Animation_Op)->(editor.Tool_Result,editor.Undo_Group) {
    w:=&app.world; context.allocator=w.allocator; result:=error_result(w,.None)
    if !ecs.entity_exists(w,op.entity) { result.error=.Entity_Not_Found; return result,{} }; _,hidden:=ecs.get_component(w,op.entity,Editor_Hidden); if hidden { result.error=.Protected_Entity; return result,{} }
    if op.action==.Play { result.error=animation_play(w,op.entity,op.clip,op.fade_seconds,op.looping,op.speed); if result.error!=.None { return result,{} } }
    model:=ecs.get_component_mut(w,op.entity,Animation_Model); if model==nil { result.error=.Component_Not_Found; return result,{} }
    player:=ecs.get_component_mut(w,op.entity,Animation_Player)
    clip_names:=make([]struct {name:string,duration_seconds:f32},len(model.clips),w.allocator); defer delete(clip_names)
    for clip,i in model.clips { clip_names[i]={clip.name,clip.duration} }
    playback:json.Value=json.Null{}; if player!=nil {
        transition:json.Value=json.Null{}; if player.blending { transition=trigger_json_value(struct {target_clip:string,target_time_seconds:f32,target_looping:bool,duration_seconds,elapsed_seconds,progress:f32}{player.target_clip,player.target_time,player.target_looping,player.blend_duration,player.blend_time,1-player.blend_weight}) }; defer json.destroy_value(transition)
        current_clip:json.Value=json.Null{}; if player.clip!="" { current_clip=player.clip }
        playback=trigger_json_value(struct {clip:json.Value,time_seconds:f32,playing,looping:bool,speed:f32,transition:json.Value}{current_clip,player.time,player.playing,player.looping,player.speed,transition})
    }; defer json.destroy_value(playback)
    entity_text:=fmt.aprintf("%d",u64(op.entity)); defer delete(entity_text)
    data,err:=json.marshal(struct {entity_id:string,clips:[]struct {name:string,duration_seconds:f32},playback:json.Value}{entity_text,clip_names,playback},allocator=w.allocator)
    if err!=nil { result.error=.Decode_Failed } else { result.data=data; append(&result.entities,op.entity) }; return result,{}
}
