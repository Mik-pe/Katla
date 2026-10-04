//! Timeline controls use the canonical animation executor and retained per-entity drafts.
package editor_app

import app ".."
import scene "../../agent/scene"
import ecs "../../ecs"
import ui "../../ui"
import "core:encoding/json"
import "core:fmt"
import "core:math"

@(private="package")
timeline_selected :: proc(shell:^Shell)->(ecs.Entity_Id,^app.Animation_Model,bool) {
    if !shell.state.selection.has_primary {return 0,nil,false}
    entity:=shell.state.selection.primary
    if !selectable(shell.state,entity) {return entity,nil,false}
    model:=ecs.get_component_mut(&shell.state.owner.world,entity,app.Animation_Model)
    return entity,model,model!=nil && len(model.clips)>0
}
@(private="package")
timeline_state :: proc(shell:^Shell,entity:ecs.Entity_Id,name:string,value:ui.Value)->ui.State_Id {return ui.state(shell.ctx,key(110,name,u64(entity)),0,value)}
@(private="package")
timeline_index :: proc(shell:^Shell,entity:ecs.Entity_Id,name:string,count:int,initial:int=0)->int {
    value,valid:=ui.state_get(shell.ctx,timeline_state(shell,entity,name,f32(initial)),f32)
    if !valid||math.is_nan(value)||math.is_inf(value)||value<0||value>=f32(count) {return clamp(initial,0,max(0,count-1))}
    return int(value)
}
@(private="package")
timeline_number :: proc(shell:^Shell,entity:ecs.Entity_Id,name:string,value,minimum,maximum,step:f32,action:Action,disabled:bool=false)->ui.Descriptor {
    control_key:=key(110,name,u64(entity));state:=timeline_state(shell,entity,name,value)
    if shell.ctx.focused.key!=control_key&&shell.ctx.captured.key!=control_key {ui.state_set(shell.ctx,state,value)}
    return {key=control_key,kind=.Slider,text=name,state=state,minimum=minimum,maximum=maximum,step=step,action=u64(action),payload=u64(entity),disabled=disabled,layout={height=ui.pixels(30),width=ui.percent(1)}}
}
@(private="package")
timeline_button :: proc(entity:ecs.Entity_Id,name:string,action:Action,disabled:bool)->ui.Descriptor {
    return {key=key(110,name,u64(entity)),kind=.Button,text=name,action=u64(action),payload=u64(entity),disabled=disabled,layout={height=ui.pixels(30),padding={0,8,0,8}}}
}
@(private="package")
shell_timeline :: proc(shell:^Shell)->ui.Descriptor {
    items:=make([dynamic]ui.Descriptor,shell.allocator);defer delete(items)
    entity,model,present:=timeline_selected(shell)
    if !present {append(&items,text(110,"Select an entity with animation clips"))}
    else {
        names:=make([]string,len(model.clips),shell.allocator);defer delete(names,shell.allocator)
        for clip,i in model.clips {names[i]=clip.name}
        owned_names:=make([]string,len(names),shell.allocator)
        for name,i in names {owned_names[i]=name}
        // Options borrow owned clip names; their slice lives until the next shell frame.
        append(&shell.option_lists,owned_names)
        player:=ecs.get_component_mut(&shell.state.owner.world,entity,app.Animation_Player)
        initial:=0;if player!=nil {for clip,i in model.clips {if clip.name==player.clip {initial=i;break}}}
        selected:=timeline_index(shell,entity,"clip",len(names),initial)
        disabled:=shell.state.owner.mode==.Paused
        append(&items,ui.Descriptor{key=key(110,"clip",u64(entity)),kind=.Combo,text="Clip",options=owned_names,state=timeline_state(shell,entity,"clip",f32(selected)),action=u64(Action.Timeline_Clip),payload=u64(entity),disabled=disabled,layout={height=ui.pixels(30),width=ui.percent(1)}})
        playing:=player!=nil&&player.playing;has_clip:=player!=nil&&player.clip_present
        controls:=[4]ui.Descriptor{
            timeline_button(entity,"Play clip",.Timeline_Play,disabled),
            timeline_button(entity,"Pause clip",.Timeline_Pause,disabled||!playing),
            timeline_button(entity,"Resume clip",.Timeline_Resume,disabled||!has_clip||playing||player.completed),
            timeline_button(entity,"Stop clip",.Timeline_Stop,disabled||!has_clip),
        }
        append(&items,ui.Descriptor{key=key(110,"transport",u64(entity)),kind=.Row,children=nodes(shell,controls[:]),layout={gap={6,0},width=ui.percent(1)}})
        clock,duration,speed:=f32(0),model.clips[selected].duration,f32(1);looping:=true
        if player!=nil {clock=player.time;duration=player.duration;speed=player.speed;looping=player.looping}
        label:=fmt.aprintf("%.3f / %.3f s%s",clock,duration," · finished" if player!=nil&&player.completed else "",allocator=shell.allocator);append(&shell.texts,label);append(&items,text(110,label))
        append(&items,timeline_number(shell,entity,"Time",clock,0,max(duration,0.001),0.001,.Timeline_Seek,disabled||!has_clip))
        append(&items,timeline_number(shell,entity,"Speed",speed,0,max(4,speed),0.05,.Timeline_Speed,disabled||!has_clip))
        loop_state:=timeline_state(shell,entity,"Loop",looping);ui.state_set(shell.ctx,loop_state,looping)
        append(&items,ui.Descriptor{key=key(110,"Loop",u64(entity)),kind=.Checkbox,text="Loop",state=loop_state,action=u64(Action.Timeline_Loop),payload=u64(entity),disabled=disabled||!has_clip,layout={height=ui.pixels(30),width=ui.percent(1)}})
        target:=timeline_index(shell,entity,"fade_target",len(names),min(1,len(names)-1))
        append(&items,ui.Descriptor{key=key(110,"fade_target",u64(entity)),kind=.Combo,text="Fade target",options=owned_names,state=timeline_state(shell,entity,"fade_target",f32(target)),action=u64(Action.Timeline_Fade),payload=u64(entity),disabled=disabled,layout={height=ui.pixels(30),width=ui.percent(1)}})
        fade_state:=timeline_state(shell,entity,"Fade seconds",f32(0.25));fade_seconds,_:=ui.state_get(shell.ctx,fade_state,f32)
        append(&items,timeline_number(shell,entity,"Fade seconds",fade_seconds,0,10,0.05,.Timeline_Fade_Time,disabled))
        append(&items,timeline_button(entity,"Fade to clip",.Timeline_Fade,disabled||!has_clip||player.blending))
        if player!=nil&&player.blending {
            progress:=clamp(1-player.blend_weight,0,1)
            value:=fmt.aprintf("Fade → %s · %.3f / %.3f s",player.target_clip,player.blend_time,player.blend_duration,allocator=shell.allocator);append(&shell.texts,value);append(&items,text(110,value))
            append(&items,ui.Descriptor{key=key(110,"fade_progress",u64(entity)),kind=.Progress,value=progress,layout={height=ui.pixels(8),width=ui.percent(1)}})
        }
        if shell.state.owner.mode==.Paused {append(&items,text(110,"Simulation is paused; timeline clocks are frozen"))}
    }
    return {key=key(110,"panel"),kind=.Scroll_Area,children=nodes(shell,{ui.Descriptor{key=key(110,"content"),kind=.Column,layout={padding={12,12,12,12},gap={0,6},width=ui.percent(1)},children=nodes(shell,items[:])}})}
}
@(private="package")
timeline_execute :: proc(shell:^Shell,op:scene.Animation_Op) {
    if shell.state.owner.mode==.Paused {shell.state.last_error=.Invalid_Operation;return}
    entity,_,present:=timeline_selected(shell);if !present||op.entity!=entity {shell.state.last_error=.Entity_Not_Found;return}
    fields:=make(json.Object,shell.allocator)
    names:=[9]string{"inspect","play","pause","resume","stop","seek","speed","loop","fade"}
    fields["action"]=names[int(op.action)];fields["entity_id"]=fmt.aprintf("%d",u64(entity),allocator=shell.allocator)
    switch op.action {
    case .Play,.Fade:fields["clip"]=op.clip;fields["fade_seconds"]=json.Float(op.fade_seconds);fields["looping"]=op.looping;fields["speed"]=json.Float(op.speed)
    case .Seek:fields["time_seconds"]=json.Float(op.time_seconds)
    case .Speed:fields["speed"]=json.Float(op.speed)
    case .Loop:fields["looping"]=op.looping
    case .Inspect,.Pause,.Resume,.Stop:
    }
    // Marshal while names are borrowed; json owns only the formatted entity identity.
    bytes,error:=json.marshal(fields,allocator=shell.allocator)
    delete(fields["entity_id"].(string),shell.allocator);delete(fields)
    if error!=nil {shell.state.last_error=.Decode_Failed;return};defer delete(bytes,shell.allocator)
    execute(shell.state,{kind=.Application,tool_name="animation",value=bytes})
}
@(private="package")
shell_timeline_click :: proc(shell:^Shell,event:ui.Click_Action)->bool {
    action:=Action(event.action);if action not_in (bit_set[Action]{.Timeline_Play,.Timeline_Pause,.Timeline_Resume,.Timeline_Stop,.Timeline_Fade}) {return false}
    entity,model,present:=timeline_selected(shell);if !present||event.payload!=u64(entity) {shell.state.last_error=.Entity_Not_Found;return true}
    player:=ecs.get_component_mut(&shell.state.owner.world,entity,app.Animation_Player)
    op:=scene.Animation_Op{entity=entity,looping=true,speed=1}
    #partial switch action {
    case .Timeline_Play:op.action=.Play;op.clip=model.clips[timeline_index(shell,entity,"clip",len(model.clips))].name
    case .Timeline_Fade:op.action=.Fade;op.clip=model.clips[timeline_index(shell,entity,"fade_target",len(model.clips),min(1,len(model.clips)-1))].name;op.fade_seconds,_=ui.state_get(shell.ctx,timeline_state(shell,entity,"Fade seconds",f32(0.25)),f32)
    case .Timeline_Pause:op.action=.Pause
    case .Timeline_Resume:op.action=.Resume
    case .Timeline_Stop:op.action=.Stop
    case:
    }
    if player!=nil {op.looping=player.looping;op.speed=player.speed}
    timeline_execute(shell,op);return true
}
@(private="package")
shell_timeline_number :: proc(shell:^Shell,event:ui.Number_Action)->bool {
    action:=Action(event.action);if action not_in (bit_set[Action]{.Timeline_Seek,.Timeline_Speed,.Timeline_Fade_Time}) {return false}
    if action==.Timeline_Fade_Time {ui.state_set(shell.ctx,event.state,event.value);return true}
    op:=scene.Animation_Op{entity=ecs.Entity_Id(event.payload)}
    if action==.Timeline_Seek {op.action=.Seek;op.time_seconds=event.value}else {if !event.finished {return true};op.action=.Speed;op.speed=event.value}
    timeline_execute(shell,op);return true
}
@(private="package")
shell_timeline_toggle :: proc(shell:^Shell,event:ui.Toggle_Action)->bool {
    if Action(event.action)!=.Timeline_Loop {return false}
    timeline_execute(shell,{action=.Loop,entity=ecs.Entity_Id(event.payload),looping=event.value});return true
}
@(private="package")
shell_timeline_choice :: proc(shell:^Shell,event:ui.Selection_Action)->bool {
    action:=Action(event.action);if action!=.Timeline_Clip&&action!=.Timeline_Fade {return false}
    entity,model,present:=timeline_selected(shell);if !present||event.payload!=u64(entity)||event.index<0||event.index>=len(model.clips) {shell.state.last_error=.Invalid_Field_Value;return true}
    name:="clip";if action==.Timeline_Fade {name="fade_target"}
    ui.state_set(shell.ctx,timeline_state(shell,entity,name,f32(0)),f32(event.index));return true
}
