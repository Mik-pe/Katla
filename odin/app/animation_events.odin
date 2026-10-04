//! Completed animation packets leave component mailboxes through one application frame consumer.
package app
import ecs "../ecs"
import editor "../editor"
import "core:strings"
import "core:math"

Animation_Notice :: struct { entity:ecs.Entity_Id,event:Animation_Event }
/// Owned diagnostic packets; retired counts older display history, never undelivered script events.
Animation_Feedback :: struct { events:[]Animation_Notice,retired:u64 }
@(private="package")
Animation_Event_Runtime :: struct { feedback:[dynamic]Animation_Notice,bytes:int,retired,delivered:u64 }
@(private="package")
animation_event_runtime_destroy :: proc(value:rawptr) {
    owner:=cast(^Animation_Event_Runtime)value
    for notice in owner.feedback { delete(notice.event.clip) }; delete(owner.feedback); owner^={}
}
@(private="package")
animation_events_init :: proc(w:^ecs.World) {
    ecs.insert_resource(w,Animation_Event_Runtime{feedback=make([dynamic]Animation_Notice,w.allocator)},ecs.Value_Ops{destroy=animation_event_runtime_destroy})
}
@(private="package")
animation_clock_emits :: proc(time,duration:f32,looping,completed:bool,advance:f64)->bool {
    return !completed && (!looping || duration>0) && f64(time)+advance>=f64(duration)
}
/// Refuses advancement before a registered script consumer's bounded mailbox would overflow.
animation_events_preflight :: proc(owner:^Authoring,delta_seconds:f32)->editor.Scene_Error {
    if !finite_nonnegative(delta_seconds) { return .Invalid_Field_Value }
    if !ecs.contains_resource(&owner.world,Animation_Event_Runtime) { return .Application_Owned }
    if owner.mode!=.Playing || !ecs.contains_resource(&owner.world,Script_Native_Runtime) { return .None }
    signals:=ecs.get_resource_mut(&owner.world,Script_Signals); if signals==nil { return .Application_Owned }
    context.allocator=owner.world.allocator; ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    needed:=len(signals.pending); if needed>4096 { return .Invalid_Operation }
    for entity in ids {
        player:=ecs.get_component_mut(&owner.world,entity,Animation_Player); if player==nil { continue }
        needed+=len(player.events)
        model:=ecs.get_component_mut(&owner.world,entity,Animation_Model)
        if delta_seconds>0 && model!=nil && player.playing && finite_nonnegative(player.speed) {
            advance:=f64(delta_seconds)*f64(player.speed)
            duration:=player.duration; if clip:=animation_clip(model,player.clip); clip!=nil { duration=clip.duration }
            if player.clip!="" && animation_clock_emits(player.time,duration,player.looping,player.completed,advance) { needed+=1 }
            if player.blending {
                target_duration:=player.target_duration; if clip:=animation_clip(model,player.target_clip); clip!=nil { target_duration=clip.duration }
                if animation_clock_emits(player.target_time,target_duration,player.target_looping,player.target_completed,advance) { needed+=1 }
            }
        }
        if needed>4096 { return .Invalid_Operation }
    }
    return .None
}
/// Routes every packet once to the console and, while Playing, the registered Luau consumer.
/// Call after editor preview advancement; simulation_step owns the Playing invocation.
animation_events_dispatch :: proc(owner:^Authoring)->editor.Scene_Error {
    error:=animation_events_preflight(owner,0); if error!=.None { return error }
    context.allocator=owner.world.allocator
    runtime:=ecs.get_resource_mut(&owner.world,Animation_Event_Runtime)
    signals:^Script_Signals
    if owner.mode==.Playing && ecs.contains_resource(&owner.world,Script_Native_Runtime) { signals=ecs.get_resource_mut(&owner.world,Script_Signals) }
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    for entity in ids {
        player:=ecs.get_component_mut(&owner.world,entity,Animation_Player); if player==nil { continue }
        events:=animation_take_events(player)
        for event in events {
            if signals!=nil {
                name:="animation_completed" if event.kind==.Completed else "animation_looped"
                append(&signals.pending,Script_Signal{name=strings.clone(name),trigger=entity,other=entity,has_animation=true,animation_clip=strings.clone(event.clip),animation_loop_count=event.loop_count})
            }
            runtime.delivered=math.min(runtime.delivered,max(u64)-1)+1
            append(&runtime.feedback,Animation_Notice{entity,event}); runtime.bytes+=len(event.clip)
            for len(runtime.feedback)>4096 || runtime.bytes>4<<20 {
                retired:=runtime.feedback[0]; runtime.bytes-=len(retired.event.clip); delete(retired.event.clip); ordered_remove(&runtime.feedback,0)
                runtime.retired=math.min(runtime.retired,max(u64)-1)+1
            }
        }
        delete(events)
    }
    return .None
}
/// Moves console diagnostics independently of player control and scene Undo.
animation_feedback_drain :: proc(owner:^Authoring)->Animation_Feedback {
    runtime:=ecs.get_resource_mut(&owner.world,Animation_Event_Runtime); if runtime==nil { return {} }
    result:=Animation_Feedback{events=make([]Animation_Notice,len(runtime.feedback),owner.world.allocator),retired=runtime.retired}
    copy(result.events,runtime.feedback[:]); clear(&runtime.feedback); runtime.bytes=0; runtime.retired=0
    return result
}
/// Releases feedback clip names with the allocator of the owning authoring world.
animation_feedback_destroy :: proc(feedback:^Animation_Feedback,allocator:=context.allocator) {
    for notice in feedback.events { delete(notice.event.clip,allocator) }; delete(feedback.events,allocator); feedback^={}
}
