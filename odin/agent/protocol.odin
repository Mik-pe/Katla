//! Validated host calls borrow JSON until submission copies the scene operation.
package agent

import ecs "../ecs"
import editor "../editor"
import ron "../encoding/ron"
import "core:encoding/json"
import "core:mem"

/// Roles shared by a conversation UI and host adapters.
Message_Role :: enum { System, User, Assistant, Tool }
/// A borrowed call envelope; arguments must contain one JSON object.
Tool_Call :: struct { id, name:string, arguments:[]byte }
/// Reports protocol rejection before a scene action is queued.
Call_Error :: enum { None, Unknown_Tool, Invalid_JSON, Invalid_Arguments, Mailbox_Full, Mailbox_Closed, Identifier_Exhausted }
/// Owns decoded JSON and optional field bytes; operation data borrows those owners.
Decoded_Call :: struct { operation:editor.Scene_Op, tree:json.Value, field_bytes:[]byte, allocator:mem.Allocator }
/// Releases a decoded call after synchronous execution or mailbox submission.
decoded_call_destroy :: proc(call:^Decoded_Call) {
    context.allocator=call.allocator
    json.destroy_value(call.tree)
    delete(call.field_bytes,call.allocator)
    call^={}
}
/// Converts decimal strings without rounding generational IDs through floating point.
parse_entity_id :: proc(text:string)->(ecs.Entity_Id,bool) {
    value,valid:=ron.decimal_u64(text)
    return ecs.Entity_Id(value),valid
}
@(private="package")
required_string :: proc(object:json.Object,key:string)->(string,bool) {
    value,present:=object[key]
    if !present { return "",false }
    text,ok:=value.(string)
    return text,ok && len(text)>0
}
@(private="package")
vector_argument :: proc(value:json.Value)->([3]f32,bool) {
    values,ok:=value.(json.Array)
    if !ok || len(values)!=3 { return {},false }
    result:[3]f32
    for v,i in values {
        number:f64
        #partial switch n in v {
        case json.Integer: number=f64(n)
        case json.Float: number=f64(n)
        case: return {},false
        }
        if !(number>=-f64(max(f32)) && number<=f64(max(f32))) { return {},false }
        result[i]=f32(number)
    }
    return result,true
}
/// Accepts the owner-supported scene and application tools with typed pre-admission validation.
decode_call :: proc(call:Tool_Call,allocator:=context.allocator)->(Decoded_Call,Call_Error) {
    if call.name=="editor_view" { return decode_view_call(call,allocator) }
    switch call.name {
    case "generate_resource","material","material_asset","animation","simulation","behavior","trigger","prefab","search_assets","list_resources","read_resource","load_scene","save_scene","create_resource","write_resource":
        return decode_application_call(call,allocator)
    }
    kind:editor.Scene_Op_Kind
    allowed:[]string
    needs_entity,needs_component:bool
    switch call.name {
    case "spawn_entity": kind=.Spawn; allowed={"name","position","rotation","scale","shape"}
    case "destroy_entity": kind=.Destroy; allowed={"entity_id"}; needs_entity=true
    case "duplicate_entity": kind=.Duplicate; allowed={"entity_id","position_offset"}; needs_entity=true
    case "set_field": kind=.Set_Field; allowed={"entity_id","component","field","value"}; needs_entity=true; needs_component=true
    case "add_component": kind=.Add_Component; allowed={"entity_id","component"}; needs_entity=true; needs_component=true
    case "remove_component": kind=.Remove_Component; allowed={"entity_id","component"}; needs_entity=true; needs_component=true
    case "get_component_attributes": kind=.Get_Attributes; allowed={"entity_id","component"}; needs_entity=true; needs_component=true
    case "set_parent": kind=.Set_Parent; allowed={"entity_id","parent_id"}; needs_entity=true
    case "spawn_model": kind=.Spawn_Model; allowed={"path","position","default_animation"}
    case "get_scene_hierarchy": kind=.Get_Hierarchy
    case "query_entities": kind=.Query_Entities; allowed={"component_filter","limit","name_filter","position","radius"}
    case "list_available_components": kind=.List_Components
    case: return {},.Unknown_Tool
    }
    context.allocator=allocator
    query_limit:=64
    if kind==.Query_Entities { validation:Call_Error; query_limit,validation=query_arguments_validate(call.arguments,allocator); if validation!=.None { return {},validation } }
    tree,err:=json.parse(call.arguments,spec=.JSON,parse_integers=kind!=.Query_Entities,allocator=allocator)
    if err!=nil { return {},.Invalid_JSON }
    result:=Decoded_Call{operation={kind=kind,scale={1,1,1}},tree=tree,allocator=allocator}
    success:=false
    defer { if !success { decoded_call_destroy(&result) } }
    object,ok:=tree.(json.Object)
    if !ok { return {},.Invalid_Arguments }
    for key in object {
        found:=false
        for name in allowed { if key==name { found=true; break } }
        if !found { return {},.Invalid_Arguments }
    }
    if needs_entity {
        text,valid:=required_string(object,"entity_id")
        if !valid { return {},.Invalid_Arguments }
        result.operation.entity,valid=parse_entity_id(text)
        if !valid { return {},.Invalid_Arguments }
    }
    if needs_component {
        text,valid:=required_string(object,"component")
        if !valid { return {},.Invalid_Arguments }
        result.operation.component=text
    }
    if kind==.Set_Field {
        text,valid:=required_string(object,"field")
        if !valid { return {},.Invalid_Arguments }
        value,present:=object["value"]
        if !present { return {},.Invalid_Arguments }
        result.operation.field=text
        field_bytes,marshal_err:=json.marshal(value,allocator=allocator)
        result.field_bytes=field_bytes
        if marshal_err!=nil { return {},.Invalid_Arguments }
        result.operation.value=result.field_bytes
    }
    if kind==.Spawn {
        if value,present:=object["name"]; present {
            if _,is_null:=value.(json.Null); !is_null {
                text,valid:=value.(string); if !valid { return {},.Invalid_Arguments }
                result.operation.name=text
            }
        }
        for key,i in ([3]string{"position","rotation","scale"}) {
            if value,present:=object[key]; present {
                if _,is_null:=value.(json.Null); is_null && key!="position" { continue }
                vector,valid:=vector_argument(value); if !valid { return {},.Invalid_Arguments }
                switch i {
                case 0: result.operation.position=vector
                case 1: for axis,j in vector { result.operation.rotation[j]=axis*0.017453292519943295 }
                case 2: result.operation.scale=vector
                }
            }
        }
    }
    if kind==.Spawn {
        if value,present:=object["shape"]; present {
            if _,null:=value.(json.Null); !null {
                shape,valid:=value.(string); if !valid { return {},.Invalid_Arguments }
                if shape!="cube" && shape!="sphere" && shape!="plane" && shape!="cylinder" && shape!="torus" && shape!="cone" { return {},.Invalid_Arguments }
                result.operation.shape=shape
            }
        }
    }
    if kind==.Duplicate {
        if value,present:=object["position_offset"]; present {
            if _,null:=value.(json.Null); !null {
                vector,valid:=vector_argument(value); if !valid { return {},.Invalid_Arguments }
                result.operation.position_offset=vector; result.operation.has_position_offset=true
            }
        }
    }
    if kind==.Set_Parent {
        if value,present:=object["parent_id"]; present {
            if _,null:=value.(json.Null); !null {
                text,valid:=value.(string); if !valid { return {},.Invalid_Arguments }
                result.operation.parent,valid=parse_entity_id(text); if !valid { return {},.Invalid_Arguments }; result.operation.has_parent=true
            }
        }
    }
    if kind==.Spawn_Model {
        path,valid:=required_string(object,"path"); if !valid { return {},.Invalid_Arguments }; result.operation.path=path
        if value,present:=object["position"]; present { if _,is_null:=value.(json.Null); !is_null { vector,argument_valid:=vector_argument(value); if !argument_valid { return {},.Invalid_Arguments }; result.operation.position=vector } }
        if value,present:=object["default_animation"]; present {
            if _,null:=value.(json.Null); !null { text,argument_valid:=value.(string); if !argument_valid { return {},.Invalid_Arguments }; result.operation.default_animation=text }
        }
    }
    if kind==.Query_Entities {
        if value,present:=object["name_filter"]; present {
            if _,is_null:=value.(json.Null); !is_null { text,valid:=value.(string); if !valid { return {},.Invalid_Arguments }; result.operation.name_filter=text; result.operation.has_name_filter=true }
        }
        if value,present:=object["position"]; present {
            if _,is_null:=value.(json.Null); !is_null { vector,valid:=vector_argument(value); if !valid { return {},.Invalid_Arguments }; result.operation.position=vector; result.operation.has_query_position=true }
        }
        if value,present:=object["radius"]; present {
            if _,is_null:=value.(json.Null); !is_null {
                number:f64
                #partial switch n in value {
                case json.Integer: number=f64(n)
                case json.Float: number=f64(n)
                case: return {},.Invalid_Arguments
                }
                if !(number>=0 && number<=f64(max(f32))) { return {},.Invalid_Arguments }; result.operation.radius=f32(number); result.operation.has_radius=true
            }
        }
        if result.operation.has_radius!=result.operation.has_query_position { return {},.Invalid_Arguments }
        if value,present:=object["component_filter"]; present {
            if _,is_null:=value.(json.Null); !is_null { text,valid:=required_string(object,"component_filter"); if !valid { return {},.Invalid_Arguments }; result.operation.component=text }
        }
        result.operation.limit=query_limit
    }
    success=true
    return result,.None
}
/// Returns a cancellation ticket on acceptance; rejection leaves the scene and history untouched.
submit_call :: proc(h:^editor.Agent_Harness,call:Tool_Call)->(u64,Call_Error) {
    decoded,err:=decode_call(call,h.allocator)
    if err!=.None { return 0,err }
    defer decoded_call_destroy(&decoded)
    ticket,mailbox_error:=editor.agent_submit(h,decoded.operation,call.id)
    switch mailbox_error {
    case .None: err=.None
    case .Full: err=.Mailbox_Full
    case .Closed: err=.Mailbox_Closed
    case .Identifier_Exhausted: err=.Identifier_Exhausted
    }
    return ticket,err
}
