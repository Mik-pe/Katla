//! Physics overlap transitions execute ordered scene-owned rules and deferred Luau signals.
package app
import scene "../agent/scene"
import ecs "../ecs"
import editor "../editor"
import "core:strings"
import "core:slice"
import "core:fmt"
import "core:encoding/json"
import km "../math"

/// Runtime diagnostics are owned with rules; only authored rule fields carry entity references.
Trigger_Rules :: struct { rules:[]scene.Trigger_Rule `inspect:"skip"`,fired:[]bool `inspect:"skip"`,last_errors:[dynamic]string `inspect:"skip"` }
/// Current native overlap identities; the event source remains the completed physics step.
Trigger_Volume :: struct { overlapping:[dynamic]ecs.Entity_Id `inspect:"skip"` }
/// A named event contains generational references usable by lossless Luau entity userdata.
Script_Signal :: struct { name:string,trigger,other:ecs.Entity_Id,animation_clip:string,animation_loop_count:u32,has_animation:bool }
Script_Signals :: struct { pending:[dynamic]Script_Signal }
@(private="package")
trigger_rules_destroy :: proc(value:rawptr) {
    rules:=cast(^Trigger_Rules)value
    for rule in rules.rules { for action in rule.actions { delete(action.clip); delete(action.name) }; delete(rule.actions) }
    delete(rules.rules); delete(rules.fired); for error in rules.last_errors { delete(error) }; delete(rules.last_errors); rules^={}
}
@(private="package")
trigger_rules_clone :: proc(dst,src:rawptr) {
    source:=cast(^Trigger_Rules)src; target:=cast(^Trigger_Rules)dst
    target^={rules=make([]scene.Trigger_Rule,len(source.rules)),fired=slice.clone(source.fired),last_errors=make([dynamic]string,0,len(source.last_errors))}
    for rule,i in source.rules { target.rules[i]=rule; target.rules[i].actions=make([]scene.Event_Action,len(rule.actions)); for action,j in rule.actions { target.rules[i].actions[j]=action; target.rules[i].actions[j].clip=strings.clone(action.clip); target.rules[i].actions[j].name=strings.clone(action.name) } }
    for error in source.last_errors { append(&target.last_errors,strings.clone(error)) }
}
@(private="package")
trigger_rules_map :: proc(value:rawptr,mapping:editor.Reference_Map)->bool {
    rules:=cast(^Trigger_Rules)value
    for &rule in rules.rules {
        if rule.has_other { if !editor.reference_map_entity(&rule.other,mapping) { return false } }
        for &action in rule.actions { if action.target.kind==.Entity && action.kind!=.Emit { if !editor.reference_map_entity(&action.target.entity,mapping) { return false } } }
    }; return true
}
@(private="package")
trigger_volume_destroy :: proc(value:rawptr) { volume:=cast(^Trigger_Volume)value; delete(volume.overlapping); volume^={} }
@(private="package")
trigger_volume_clone :: proc(dst,src:rawptr) { target:=cast(^Trigger_Volume)dst; source:=cast(^Trigger_Volume)src; target.overlapping=slice.clone_to_dynamic(source.overlapping[:]) }
@(private="package")
script_signals_destroy :: proc(value:rawptr) { signals:=cast(^Script_Signals)value; for signal in signals.pending { delete(signal.name); delete(signal.animation_clip) }; delete(signals.pending); signals^={} }
/// Registers explicit owned rule/reference codecs and the deferred signal owner.
events_register :: proc(app:^Authoring) {
    editor.editor_register(&app.world,&app.registry,"TriggerRules",Trigger_Rules{},ecs.Value_Ops{trigger_rules_destroy,trigger_rules_clone},trigger_rules_map,spawn_default=false)
    editor.editor_register(&app.world,&app.registry,"TriggerVolume",Trigger_Volume{},ecs.Value_Ops{trigger_volume_destroy,trigger_volume_clone},spawn_default=false)
    ecs.insert_resource(&app.world,Script_Signals{make([dynamic]Script_Signal,app.world.allocator)},ecs.Value_Ops{destroy=script_signals_destroy})
}
@(private="package")
trigger_recipient :: proc(target:scene.Event_Target,trigger,other:ecs.Entity_Id)->ecs.Entity_Id {
    switch target.kind {
    case .Trigger: return trigger
    case .Other: return other
    case .Entity: return target.entity
    }; return trigger
}
/// Preflights explicit references and clip/emitter requirements before installing any rule.
trigger_validate_references :: proc(app:^Authoring,rules:[]scene.Trigger_Rule,trigger:ecs.Entity_Id,has_trigger:bool)->editor.Scene_Error {
    if !scene.trigger_rules_valid(rules) { return .Invalid_Field_Value }
    for rule in rules {
        if rule.has_other && !ecs.entity_exists(&app.world,rule.other) { return .Entity_Not_Found }
        for action in rule.actions {
            if action.kind==.Emit { continue }
            if action.target.kind==.Entity && !ecs.entity_exists(&app.world,action.target.entity) { return .Entity_Not_Found }
            if action.target.kind==.Other { continue }
            if action.target.kind==.Trigger && !has_trigger { if action.kind==.Burst_Particles || action.kind==.Set_Particles_Active { return .Component_Not_Found }; continue }
            entity:=trigger if action.target.kind==.Trigger else action.target.entity
            _,hidden:=ecs.get_component(&app.world,entity,Editor_Hidden); if hidden { return .Protected_Entity }
            switch action.kind {
            case .Play_Animation: model:=ecs.get_component_mut(&app.world,entity,Animation_Model); if model==nil { return .Component_Not_Found }; if animation_clip(model,action.clip)==nil { return .Invalid_Operation }
            case .Burst_Particles,.Set_Particles_Active: _,present:=ecs.get_component(&app.world,entity,Particle_Emitter); if !present { return .Component_Not_Found }
            case .Emit:
            }
        }
    }; return .None
}
/// Resets once-only consumption, diagnostics and overlaps at the beginning of a play session.
events_reset :: proc(app:^Authoring) {
    context.allocator=app.world.allocator; ids:=ecs.entity_ids(&app.world); defer delete(ids)
    for entity in ids {
        if rules:=ecs.get_component_mut(&app.world,entity,Trigger_Rules); rules!=nil { for &fired in rules.fired { fired=false }; for error in rules.last_errors { delete(error) }; clear(&rules.last_errors) }
        if volume:=ecs.get_component_mut(&app.world,entity,Trigger_Volume); volume!=nil { clear(&volume.overlapping) }
    }
    if signals:=ecs.get_resource_mut(&app.world,Script_Signals); signals!=nil { for signal in signals.pending { delete(signal.name); delete(signal.animation_clip) }; clear(&signals.pending) }
}
/// Executes all matching actions in order after native physics, retaining per-action failures.
events_dispatch :: proc(app:^Authoring,event:Physics_Event) {
    context.allocator=app.world.allocator
    rules:=ecs.get_component_mut(&app.world,event.trigger,Trigger_Rules); volume:=ecs.get_component_mut(&app.world,event.trigger,Trigger_Volume)
    if rules==nil || volume==nil { return }
    if event.phase==.Enter { present:=false; for entity in volume.overlapping { if entity==event.other { present=true; break } }; if !present { if volume.overlapping.allocator.procedure==nil { volume.overlapping=make([dynamic]ecs.Entity_Id,app.world.allocator) }; append(&volume.overlapping,event.other) } }
    else { for entity,i in volume.overlapping { if entity==event.other { ordered_remove(&volume.overlapping,i); break } } }
    actions:=make([dynamic]scene.Event_Action,app.world.allocator); defer delete(actions)
    if len(rules.fired)!=len(rules.rules) { delete(rules.fired); rules.fired=make([]bool,len(rules.rules),app.world.allocator) }
    for rule,i in rules.rules { if rule.phase!=event.phase || (rule.has_other && rule.other!=event.other) || (rule.once && rules.fired[i]) { continue }; if rule.once { rules.fired[i]=true }; append(&actions,..rule.actions) }
    if len(actions)==0 { return }; for error in rules.last_errors { delete(error) }; clear(&rules.last_errors)
    if rules.last_errors.allocator.procedure==nil { rules.last_errors=make([dynamic]string,app.world.allocator) }
    for action in actions {
        entity:=trigger_recipient(action.target,event.trigger,event.other); err:editor.Scene_Error
        switch action.kind {
        case .Play_Animation: err=animation_play(&app.world,entity,action.clip,action.fade_seconds,action.looping,action.speed)
        case .Burst_Particles: err=particle_burst(&app.world,entity,action.count)
        case .Set_Particles_Active: err=particle_set_active(&app.world,entity,action.active)
        case .Emit:
            signals:=ecs.get_resource_mut(&app.world,Script_Signals)
            if signals==nil || len(signals.pending)>=4096 { err=.Invalid_Operation } else { append(&signals.pending,Script_Signal{name=strings.clone(action.name),trigger=event.trigger,other=event.other}) }
        }
        if err!=.None { append(&rules.last_errors,fmt.aprintf("%s: %v",action.clip if action.kind==.Play_Animation else "trigger action",err)) }
    }
}

@(private="package")
trigger_json_value :: proc(value:$T)->json.Value { bytes,err:=json.marshal(value); if err!=nil { return nil }; defer delete(bytes); tree,parse_error:=json.parse(bytes,spec=.JSON,parse_integers=true); if parse_error!=nil { return nil }; return tree }
@(private="package")
trigger_wire_target :: proc(target:scene.Event_Target)->json.Value {
    switch target.kind {
    case .Trigger: return trigger_json_value(struct {kind:string}{"trigger"})
    case .Other: return trigger_json_value(struct {kind:string}{"other"})
    case .Entity: text:=fmt.aprintf("%d",u64(target.entity)); defer delete(text); return trigger_json_value(struct {kind,entity:string}{"entity",text})
    }; return nil
}
@(private="package")
trigger_wire_actions :: proc(actions:[]scene.Event_Action)->json.Array {
    output:=make(json.Array,len(actions))
    for action,i in actions {
        if action.kind==.Emit { output[i]=trigger_json_value(struct {action,name:string}{"emit",action.name}); continue }
        target:=trigger_wire_target(action.target); defer json.destroy_value(target)
        switch action.kind {
        case .Play_Animation: output[i]=trigger_json_value(struct {action:string,target:json.Value,clip:string,fade_seconds:f32,looping:bool,speed:f32}{"play_animation",target,action.clip,action.fade_seconds,action.looping,action.speed})
        case .Burst_Particles: output[i]=trigger_json_value(struct {action:string,target:json.Value,count:u32}{"burst_particles",target,action.count})
        case .Set_Particles_Active: output[i]=trigger_json_value(struct {action:string,target:json.Value,active:bool}{"set_particles_active",target,action.active})
        case .Emit:
        }
    }; return output
}
/// Returns a canonical transport-shaped owned rule array with decimal reference strings.
trigger_wire_rules :: proc(rules:[]scene.Trigger_Rule)->json.Value {
    output:=make(json.Array,len(rules))
    for rule,i in rules {
        actions:=trigger_wire_actions(rule.actions); defer json.destroy_value(json.Value(actions)); phase:="enter" if rule.phase==.Enter else "exit"
        if rule.has_other { text:=fmt.aprintf("%d",u64(rule.other)); defer delete(text); output[i]=trigger_json_value(struct {event,other_entity:string,once:bool,actions:json.Array}{phase,text,rule.once,actions}) }
        else { output[i]=trigger_json_value(struct {event:string,once:bool,actions:json.Array}{phase,rule.once,actions}) }
    }; return output
}
/// Admits owned trigger proposals through component-delta history and native scene preparation.
trigger_execute :: proc(app:^Authoring,op:scene.Trigger_Op)->(editor.Tool_Result,editor.Undo_Group) {
    context.allocator=app.world.allocator; w:=&app.world; result:=error_result(w,.None); undo:editor.Undo_Group
    if op.action!=.Inspect && app.mode!=.Editing { result.error=.Editing_Required; return result,undo }
    if op.action!=.Inspect { if error:=authoring_before_mutation(app); error!=.None { result.error=error; return result,undo } }
    entity:=op.entity
    if op.action!=.Create_Box {
        if !ecs.entity_exists(w,entity) { result.error=.Entity_Not_Found; return result,undo }; _,hidden:=ecs.get_component(w,entity,Editor_Hidden); if hidden { result.error=.Protected_Entity; return result,undo }
        volume:=ecs.get_component_mut(w,entity,Trigger_Volume); if volume==nil { result.error=.Component_Not_Found; return result,undo }
    }
    if op.action==.Create_Box {
        if strings.trim_space(op.name)=="" || len(op.name)>128 { result.error=.Invalid_Field_Value; return result,undo }
        for position in op.position { if !finite_nonnegative(abs(position)) { result.error=.Invalid_Field_Value; return result,undo } }
        for size in op.half_extents { if !finite_nonnegative(size) || size<=0 { result.error=.Invalid_Field_Value; return result,undo } }
        ids:=ecs.entity_ids(w); defer delete(ids); for id in ids { name,exists:=ecs.get_component(w,id,Scene_Name); if exists && name.name==op.name { result.error=.Invalid_Operation; return result,undo } }
        reference_error:=trigger_validate_references(app,op.rules,0,false); if reference_error!=.None { result.error=reference_error; return result,undo }
        identity:=ecs.get_resource_mut(w,Scene_Identity); if identity==nil || identity.next_entity_id==0 || identity.next_entity_id==max(u64) { result.error=.Invalid_Operation; return result,undo }
        owned_rules:Trigger_Rules; source:=Trigger_Rules{rules=op.rules}; trigger_rules_clone(&owned_rules,&source)
        owned_rules.fired=make([]bool,len(op.rules),w.allocator)
        proposal:=ecs.create_entity(w); ecs.add_component(w,proposal,Scene_Key{identity.next_entity_id})
        ecs.add_component(w,proposal,Scene_Name{strings.clone(op.name)}); ecs.add_component(w,proposal,Scene_Transform{km.transform(position=km.Vec3(op.position))})
        ecs.add_component(w,proposal,physics_body(Physics_Shape{kind=.Box,half_extents=op.half_extents},.Kinematic,true)); ecs.add_component(w,proposal,Trigger_Volume{}); ecs.add_component(w,proposal,owned_rules)
        command:=scene_action_command_new(app)
        append(&command.rows,Scene_Action_Row{entity=proposal,after_exists=true,after=editor.entity_components_capture(w,&app.registry,proposal)})
        ecs.destroy_entity(w,proposal)
        undo=scene_action_command_group(command)
        if error:=editor.redo_group(w,&app.registry,&undo); error!=.None { editor.undo_group_destroy(&undo); result.error=error; return result,undo }
        entity=undo.entities[0]; identity.next_entity_id+=1
    } else if op.action==.Set_Rules {
        body,has_body:=ecs.get_component(w,entity,Physics_Body); _,has_transform:=ecs.get_component(w,entity,Scene_Transform)
        if !has_body || !body.sensor || !has_transform { result.error=.Component_Not_Found; return result,undo }
        reference_error:=trigger_validate_references(app,op.rules,entity,true); if reference_error!=.None { result.error=reference_error; return result,undo }
        before:=editor.entity_components_capture(w,&app.registry,entity)
        proposal:=scene_action_proposal_clone(app,before[:])
        owned_rules:Trigger_Rules; source:=Trigger_Rules{rules=op.rules}; trigger_rules_clone(&owned_rules,&source)
        owned_rules.fired=make([]bool,len(op.rules),w.allocator); ecs.add_component(w,proposal,owned_rules)
        after:=editor.entity_components_capture(w,&app.registry,proposal); ecs.destroy_entity(w,proposal)
        command:=scene_action_command_new(app)
        append(&command.rows,Scene_Action_Row{entity,true,true,before,after})
        undo=scene_action_command_group(command)
        if error:=editor.redo_group(w,&app.registry,&undo); error!=.None { editor.undo_group_destroy(&undo); result.error=error; return result,undo }
    }
    rules:=ecs.get_component_mut(w,entity,Trigger_Rules); volume:=ecs.get_component_mut(w,entity,Trigger_Volume); body,has_body:=ecs.get_component(w,entity,Physics_Body)
    if rules==nil || volume==nil || !has_body { result.error=.Component_Not_Found; return result,undo }
    wire:=trigger_wire_rules(rules.rules); defer json.destroy_value(wire)
    overlap_ids:=make([]string,len(volume.overlapping)); defer { for id in overlap_ids { delete(id) }; delete(overlap_ids) }; for id,i in volume.overlapping { overlap_ids[i]=fmt.aprintf("%d",u64(id)) }
    fired:=make([dynamic]int); defer delete(fired); for activated,i in rules.fired { if activated { append(&fired,i) } }
    name:=""; if label,present:=ecs.get_component(w,entity,Scene_Name); present { name=label.name }
    world_matrix,_:=scene_world_matrix(app,entity)
    entity_text:=fmt.aprintf("%d",u64(entity)); defer delete(entity_text)
    result.data,_=json.marshal(struct {entity_id,name:string,position:[3]f32,shape:Physics_Shape,simulation_active:bool,overlapping_entities:[]string,rules:json.Value,fired_once_rules:[]int,last_errors:[]string}{entity_text,name,km.mat4_extract_translation(world_matrix),body.shape,app.mode==.Playing,overlap_ids,wire,fired[:],rules.last_errors[:]})
    append(&result.entities,entity); if result.data==nil { result.error=.Decode_Failed }; return result,undo
}
