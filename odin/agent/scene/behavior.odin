//! Attachment and bounded preview requests retain explicit detach semantics.
package scene
import ecs "../../ecs"
import "core:encoding/json"
import "core:mem"

Behavior_Action :: enum { Describe, Inspect, Set_Script, Set_Particles, Burst, Set_Active }
/// Particle documents borrow the decoded tree and must be validated by the owner before mutation.
Behavior_Op :: struct { action:Behavior_Action,entity:ecs.Entity_Id,path:string,document:json.Value,detach,active:bool,count:u32 }
Decoded_Behavior :: struct { operation:Behavior_Op,tree:json.Value,allocator:mem.Allocator }
/// Releases borrowed document storage using the captured allocator.
decoded_behavior_destroy :: proc(decoded:^Decoded_Behavior) { context.allocator=decoded.allocator; json.destroy_value(decoded.tree); decoded^={} }
/// Requires explicit null for detach and rejects fields belonging to another action.
decode_behavior :: proc(data:[]byte,allocator:=context.allocator)->(Decoded_Behavior,Decode_Error) {
    context.allocator=allocator; tree,err:=json.parse(data,spec=.JSON,parse_integers=true,allocator=allocator); if err!=nil { return {},.Invalid_JSON }
    success:=false; defer { if !success { json.destroy_value(tree) } }
    object,ok:=tree.(json.Object); if !ok { return {},.Invalid_Arguments }; action,is_action:=object["action"].(string); if !is_action { return {},.Invalid_Arguments }
    op:Behavior_Op
    if action=="describe" { if !keys_valid(object,{"action"}) { return {},.Invalid_Arguments }; op.action=.Describe; success=true; return {op,tree,allocator},.None }
    entity,is_entity:=entity_value(object["entity_id"]); if !is_entity { return {},.Invalid_Arguments }; op.entity=entity
    switch action {
    case "inspect": op.action=.Inspect; if !keys_valid(object,{"action","entity_id"}) { return {},.Invalid_Arguments }
    case "set_script":
        op.action=.Set_Script; if !keys_valid(object,{"action","entity_id","path"}) { return {},.Invalid_Arguments }
        value,present:=object["path"]; if !present { return {},.Invalid_Arguments }; _,op.detach=value.(json.Null)
        if !op.detach { text,is_text:=value.(string); if !is_text || text=="" { return {},.Invalid_Arguments }; op.path=text }
    case "set_particles":
        op.action=.Set_Particles; if !keys_valid(object,{"action","entity_id","document"}) { return {},.Invalid_Arguments }
        value,present:=object["document"]; if !present { return {},.Invalid_Arguments }; _,op.detach=value.(json.Null)
        if !op.detach { _,is_object:=value.(json.Object); if !is_object { return {},.Invalid_Arguments }; op.document=value }
    case "burst":
        op.action=.Burst; if !keys_valid(object,{"action","entity_id","count"}) { return {},.Invalid_Arguments }; value,is_integer:=object["count"].(json.Integer); if !is_integer || value<1 || value>100000 { return {},.Invalid_Arguments }; op.count=u32(value)
    case "set_active":
        op.action=.Set_Active; if !keys_valid(object,{"action","entity_id","active"}) { return {},.Invalid_Arguments }; active,is_bool:=object["active"].(bool); if !is_bool { return {},.Invalid_Arguments }; op.active=active
    case: return {},.Invalid_Arguments
    }
    success=true; return {op,tree,allocator},.None
}
