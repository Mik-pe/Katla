//! View requests cross the same owner mailbox and wait for a committed color/object-ID capture.
package agent

import ecs "../ecs"
import "core:encoding/json"

Editor_View_Action :: enum { Observe, Set_Camera, Select, Focus, Undo, Redo }
/// Entity zero is valid; has_entity explicitly distinguishes cleared selection.
Editor_View_Request :: struct { action:Editor_View_Action, entity:ecs.Entity_Id, has_entity,select:bool, position,target:[3]f32, limit:int }
/// Validates owner-routed view arguments without retaining JSON or GPU state.
editor_view_decode :: proc(arguments:[]byte,allocator:=context.allocator)->(Editor_View_Request,Call_Error) {
    context.allocator=allocator
    limit,validation:=query_arguments_validate(arguments,allocator,true); if validation!=.None { return {},validation }
    tree,err:=json.parse(arguments,spec=.JSON,parse_integers=false,allocator=allocator)
    if err!=nil { return {},.Invalid_JSON }; defer json.destroy_value(tree)
    object,object_ok:=tree.(json.Object); if !object_ok { return {},.Invalid_Arguments }
    action,action_ok:=object["action"].(string); if !action_ok { return {},.Invalid_Arguments }
    result:=Editor_View_Request{limit=limit}; allowed:[]string
    switch action {
    case "observe": result.action=.Observe; allowed={"action","limit"}
    case "set_camera": result.action=.Set_Camera; allowed={"action","position","target"}
    case "select": result.action=.Select; allowed={"action","entity_id"}
    case "focus": result.action=.Focus; allowed={"action","entity_id","select"}
    case "undo": result.action=.Undo; allowed={"action"}
    case "redo": result.action=.Redo; allowed={"action"}
    case: return {},.Invalid_Arguments
    }
    for key in object {
        valid:=false; for name in allowed { if name==key { valid=true; break } }
        if !valid { return {},.Invalid_Arguments }
    }
    if result.action==.Set_Camera {
        var_valid:bool
        result.position,var_valid=vector_argument(object["position"]); if !var_valid { return {},.Invalid_Arguments }
        result.target,var_valid=vector_argument(object["target"]); if !var_valid || result.position==result.target { return {},.Invalid_Arguments }
    }
    if result.action==.Select || result.action==.Focus {
        value,present:=object["entity_id"]
        _,is_null:=value.(json.Null)
        if present && !is_null {
            text,valid:=value.(string); if !valid { return {},.Invalid_Arguments }
            result.entity,valid=parse_entity_id(text); if !valid { return {},.Invalid_Arguments }; result.has_entity=true
        }
        if result.action==.Focus && !result.has_entity { return {},.Invalid_Arguments }
    }
    if value,present:=object["select"]; present {
        flag,valid:=value.(bool); if !valid { return {},.Invalid_Arguments }; result.select=flag
    }
    return result,.None
}
@(private="package")
decode_view_call :: proc(call:Tool_Call,allocator:=context.allocator)->(Decoded_Call,Call_Error) {
    _,err:=editor_view_decode(call.arguments,allocator); if err!=.None { return {},err }
    return Decoded_Call{operation={kind=.Application,tool_name=call.name,value=call.arguments},allocator=allocator},.None
}
