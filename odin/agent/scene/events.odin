//! Durable trigger rules use lossless generational references.
package scene

Trigger_Phase :: enum { Enter, Exit }

import ecs "../../ecs"
import "core:encoding/json"
import "core:mem"
import "core:strings"

Event_Target_Kind :: enum { Trigger, Other, Entity }
/// Kind determines whether the recipient contains an explicit live reference.
Event_Target :: struct { kind:Event_Target_Kind,entity:ecs.Entity_Id }
Event_Action_Kind :: enum { Play_Animation, Burst_Particles, Set_Particles_Active, Emit }
/// One semantic command; clips, timings and targets remain application policy.
Event_Action :: struct { kind:Event_Action_Kind,target:Event_Target,clip,name:string,fade_seconds,speed:f32,looping,active:bool,count:u32 }
/// Once rules are consumed on the first matched transition, including failed actions.
Trigger_Rule :: struct { phase:Trigger_Phase,other:ecs.Entity_Id,has_other,once:bool,actions:[]Event_Action }
Trigger_Action :: enum { Create_Box, Set_Rules, Inspect }
Trigger_Op :: struct { action:Trigger_Action,entity:ecs.Entity_Id,name:string,position,half_extents:[3]f32,rules:[]Trigger_Rule }
Decoded_Trigger :: struct { operation:Trigger_Op,tree:json.Value,allocator:mem.Allocator }
/// Releases decoded arrays; transport strings remain owned by the decoded JSON tree.
decoded_trigger_destroy :: proc(decoded:^Decoded_Trigger) { context.allocator=decoded.allocator; for rule in decoded.operation.rules { delete(rule.actions,decoded.allocator) }; delete(decoded.operation.rules,decoded.allocator); json.destroy_value(decoded.tree); decoded^={} }
@(private="package")
trigger_vector :: proc(value:json.Value,positive:bool)->([3]f32,bool) {
    array,ok:=value.(json.Array); if !ok || len(array)!=3 { return {},false }; output:[3]f32
    for element,i in array {
        number:f64
        #partial switch v in element {
        case json.Integer: number=f64(v)
        case json.Float: number=f64(v)
        case: return {},false
        }
        output[i]=f32(number); if !finite_number(output[i]) || (positive && output[i]<=0) { return {},false }
    }
    return output,true
}
@(private="package")
finite_number :: proc(value:f32)->bool { return value<=max(f32) && value>=-max(f32) }
@(private="package")
trigger_target :: proc(value:json.Value)->(Event_Target,bool) {
    object,ok:=value.(json.Object); if !ok { return {},false }; kind,is_kind:=object["kind"].(string); if !is_kind { return {},false }
    switch kind {
    case "trigger": return {.Trigger,0},keys_valid(object,{"kind"})
    case "other": return {.Other,0},keys_valid(object,{"kind"})
    case "entity": id,valid:=entity_value(object["entity"]); return {.Entity,id},valid && keys_valid(object,{"kind","entity"})
    }
    return {},false
}
@(private="package")
trigger_event_action :: proc(raw_value:json.Value)->(Event_Action,bool) {
    object,ok:=raw_value.(json.Object); if !ok { return {},false }; name,is_name:=object["action"].(string); if !is_name { return {},false }; action:=Event_Action{fade_seconds=0.25,speed=1,looping=true}
    if name=="emit" { text,is_text:=object["name"].(string); action.kind=.Emit; action.name=text; return action,is_text && strings.trim_space(text)!="" && len(text)<=128 && keys_valid(object,{"action","name"}) }
    target,is_target:=trigger_target(object["target"]); if !is_target { return {},false }; action.target=target
    switch name {
    case "play_animation":
        action.kind=.Play_Animation; if !keys_valid(object,{"action","target","clip","fade_seconds","looping","speed"}) { return {},false }
        clip,is_clip:=object["clip"].(string); if !is_clip || strings.trim_space(clip)=="" { return {},false }; action.clip=clip
        if value,present:=object["fade_seconds"]; present { valid:bool; action.fade_seconds,valid=nonnegative(value); if !valid { return {},false } }
        if value,present:=object["speed"]; present { valid:bool; action.speed,valid=nonnegative(value); if !valid { return {},false } }
        if value,present:=object["looping"]; present { valid:bool; action.looping,valid=value.(bool); if !valid { return {},false } }
    case "burst_particles":
        action.kind=.Burst_Particles; if !keys_valid(object,{"action","target","count"}) { return {},false }; count,is_count:=object["count"].(json.Integer); if !is_count || count<1 || count>100000 { return {},false }; action.count=u32(count)
    case "set_particles_active":
        action.kind=.Set_Particles_Active; if !keys_valid(object,{"action","target","active"}) { return {},false }; active,is_active:=object["active"].(bool); if !is_active { return {},false }; action.active=active
    case: return {},false
    }
    return action,true
}
@(private="package")
trigger_rules_decode :: proc(raw_value:json.Value,allocator:mem.Allocator)->([]Trigger_Rule,bool) {
    array,ok:=raw_value.(json.Array); if !ok || len(array)>64 { return nil,false }
    rules:=make([]Trigger_Rule,len(array),allocator); success:=false; defer { if !success { for rule in rules { delete(rule.actions,allocator) }; delete(rules,allocator) } }
    for element,i in array {
        object,is_object:=element.(json.Object); if !is_object || !keys_valid(object,{"event","other_entity","once","actions"}) { return nil,false }
        phase,is_phase:=object["event"].(string); if !is_phase || (phase!="enter" && phase!="exit") { return nil,false }; rules[i].phase=.Enter if phase=="enter" else .Exit
        if other,present:=object["other_entity"]; present { if _,is_null:=other.(json.Null); !is_null { id,is_id:=entity_value(other); if !is_id { return nil,false }; rules[i].other=id; rules[i].has_other=true } }
        if once,present:=object["once"]; present { boolean,is_bool:=once.(bool); if !is_bool { return nil,false }; rules[i].once=boolean }
        actions,is_actions:=object["actions"].(json.Array); if !is_actions || len(actions)<1 || len(actions)>32 { return nil,false }; rules[i].actions=make([]Event_Action,len(actions),allocator)
        for value,j in actions { action,valid:=trigger_event_action(value); if !valid { return nil,false }; rules[i].actions[j]=action }
    }
    success=true; return rules,true
}
/// Fully validates nested lists, decimal IDs and animation parameters before authoring admission.
decode_trigger :: proc(data:[]byte,allocator:=context.allocator)->(Decoded_Trigger,Decode_Error) {
    context.allocator=allocator; tree,err:=json.parse(data,spec=.JSON,parse_integers=true,allocator=allocator); if err!=nil { return {},.Invalid_JSON }
    decoded:=Decoded_Trigger{tree=tree,allocator=allocator}; success:=false; defer { if !success { decoded_trigger_destroy(&decoded) } }
    object,is_object:=tree.(json.Object); if !is_object { return {},.Invalid_Arguments }; action,is_action:=object["action"].(string); if !is_action { return {},.Invalid_Arguments }
    switch action {
    case "inspect":
        if !keys_valid(object,{"action","entity_id"}) { return {},.Invalid_Arguments }; decoded.operation.action=.Inspect; id,valid:=entity_value(object["entity_id"]); if !valid { return {},.Invalid_Arguments }; decoded.operation.entity=id
    case "set_rules":
        if !keys_valid(object,{"action","entity_id","rules"}) { return {},.Invalid_Arguments }; decoded.operation.action=.Set_Rules; id,valid:=entity_value(object["entity_id"]); if !valid { return {},.Invalid_Arguments }; decoded.operation.entity=id
        rules,rules_valid:=trigger_rules_decode(object["rules"],allocator); if !rules_valid { return {},.Invalid_Arguments }; decoded.operation.rules=rules
    case "create_box":
        if !keys_valid(object,{"action","name","position","half_extents","rules"}) { return {},.Invalid_Arguments }; decoded.operation.action=.Create_Box
        name,is_name:=object["name"].(string); if !is_name || strings.trim_space(name)=="" || len(name)>128 { return {},.Invalid_Arguments }; decoded.operation.name=name
        position,is_position:=trigger_vector(object["position"],false); extents,is_extents:=trigger_vector(object["half_extents"],true); if !is_position || !is_extents { return {},.Invalid_Arguments }; decoded.operation.position=position; decoded.operation.half_extents=extents
        rules,rules_valid:=trigger_rules_decode(object["rules"],allocator); if !rules_valid { return {},.Invalid_Arguments }; decoded.operation.rules=rules
    case: return {},.Invalid_Arguments
    }
    success=true; return decoded,.None
}

/// Validates programmatic rules as strictly as decoded transport operations.
trigger_rules_valid :: proc(rules:[]Trigger_Rule)->bool {
    if len(rules)>64 { return false }
    for rule in rules {
        if len(rule.actions)<1 || len(rule.actions)>32 { return false }
        for action in rule.actions {
            switch action.kind {
            case .Play_Animation: if strings.trim_space(action.clip)=="" || !finite_number(action.fade_seconds) || action.fade_seconds<0 || !finite_number(action.speed) || action.speed<0 { return false }
            case .Burst_Particles: if action.count<1 || action.count>100000 { return false }
            case .Set_Particles_Active:
            case .Emit: if strings.trim_space(action.name)=="" || len(action.name)>128 { return false }
            }
        }
    }
    return true
}
