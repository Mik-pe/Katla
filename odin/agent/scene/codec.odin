//! Strict transport values for application-owned animation and preview operations.
package scene

import ecs "../../ecs"
import ron "../../encoding/ron"
import "core:encoding/json"
import "core:mem"
import "core:math"
import "core:strings"

/// Distinguishes malformed JSON from semantically invalid arguments.
Decode_Error :: enum { None, Invalid_JSON, Invalid_Arguments }
/// Named playback starts at zero; engine assets supply clip duration and poses.
Animation_Action :: enum { Inspect, Play, Pause, Resume, Stop, Seek, Speed, Loop, Fade }
Animation_Op :: struct { action:Animation_Action, entity:ecs.Entity_Id, clip:string, fade_seconds:f32, looping:bool, speed,time_seconds:f32 }
/// Owns all strings borrowed by the decoded operation.
Decoded_Animation :: struct { operation:Animation_Op, tree:json.Value, allocator:mem.Allocator }
/// Releases the operation's transport storage with its captured allocator.
decoded_animation_destroy :: proc(decoded:^Decoded_Animation) { context.allocator=decoded.allocator; json.destroy_value(decoded.tree); decoded^={} }
/// Uses explicit idempotent transitions rather than implicit toggles.
Simulation_Op :: enum { Inspect, Play, Pause, Resume, Stop }
@(private="package")
keys_valid :: proc(object:json.Object,allowed:[]string)->bool {
    for key in object { found:=false; for name in allowed { if name==key { found=true; break } }; if !found { return false } }; return true
}
@(private="package")
entity_value :: proc(value:json.Value)->(ecs.Entity_Id,bool) {
    text,ok:=value.(string); if !ok { return 0,false }
    id,valid:=ron.decimal_u64(text); return ecs.Entity_Id(id),valid
}
@(private="package")
nonnegative :: proc(value:json.Value)->(f32,bool) {
    number:f64
    #partial switch v in value {
    case json.Integer: number=f64(v)
    case json.Float: number=f64(v)
    case: return 0,false
    }
    converted:=f32(number); return converted,number>=0 && !math.is_nan(converted) && !math.is_inf(converted)
}
/// Rejects unknown fields, stale-shaped IDs and nonfinite timings before mailbox admission.
decode_animation :: proc(data:[]byte,allocator:=context.allocator)->(Decoded_Animation,Decode_Error) {
    context.allocator=allocator; tree,err:=json.parse(data,spec=.JSON,parse_integers=false,allocator=allocator)
    if err!=nil { return {},.Invalid_JSON }; success:=false; defer { if !success { json.destroy_value(tree) } }
    object,ok:=tree.(json.Object); if !ok { return {},.Invalid_Arguments }
    action,is_action:=object["action"].(string); entity,is_entity:=entity_value(object["entity_id"])
    if !is_action || !is_entity { return {},.Invalid_Arguments }
    op:=Animation_Op{entity=entity,fade_seconds=0.25,looping=true,speed=1}
    switch action {
    case "inspect": if !keys_valid(object,{"action","entity_id"}) { return {},.Invalid_Arguments }; op.action=.Inspect
    case "pause","resume","stop":
        if !keys_valid(object,{"action","entity_id"}) { return {},.Invalid_Arguments }
        op.action=.Pause if action=="pause" else .Resume if action=="resume" else .Stop
    case "seek":
        if !keys_valid(object,{"action","entity_id","time_seconds"}) { return {},.Invalid_Arguments }; op.action=.Seek
        number:f64
        #partial switch value in object["time_seconds"] {
        case json.Integer: number=f64(value)
        case json.Float: number=f64(value)
        case: return {},.Invalid_Arguments
        }
        op.time_seconds=f32(number); if !finite_number(op.time_seconds) { return {},.Invalid_Arguments }
    case "speed":
        if !keys_valid(object,{"action","entity_id","speed"}) { return {},.Invalid_Arguments }; op.action=.Speed
        valid:bool; op.speed,valid=nonnegative(object["speed"]); if !valid { return {},.Invalid_Arguments }
    case "loop":
        if !keys_valid(object,{"action","entity_id","looping"}) { return {},.Invalid_Arguments }; op.action=.Loop
        valid:bool; op.looping,valid=object["looping"].(bool); if !valid { return {},.Invalid_Arguments }
    case "play","fade":
        if !keys_valid(object,{"action","entity_id","clip","fade_seconds","looping","speed"}) { return {},.Invalid_Arguments }
        op.action=.Fade if action=="fade" else .Play; clip,is_clip:=object["clip"].(string); if !is_clip || strings.trim_space(clip)=="" { return {},.Invalid_Arguments }; op.clip=clip
        if value,present:=object["fade_seconds"]; present { valid:bool; op.fade_seconds,valid=nonnegative(value); if !valid { return {},.Invalid_Arguments } }
        if value,present:=object["speed"]; present { valid:bool; op.speed,valid=nonnegative(value); if !valid { return {},.Invalid_Arguments } }
        if value,present:=object["looping"]; present { valid:bool; op.looping,valid=value.(bool); if !valid { return {},.Invalid_Arguments } }
    case: return {},.Invalid_Arguments
    }
    success=true; return {op,tree,allocator},.None
}
/// Decodes exactly one preview transition without accepting unrelated arguments.
decode_simulation :: proc(data:[]byte,allocator:=context.allocator)->(Simulation_Op,Decode_Error) {
    context.allocator=allocator; tree,err:=json.parse(data,spec=.JSON,parse_integers=true,allocator=allocator); if err!=nil { return {},.Invalid_JSON }; defer json.destroy_value(tree)
    object,ok:=tree.(json.Object); if !ok || !keys_valid(object,{"action"}) { return {},.Invalid_Arguments }
    action,is_action:=object["action"].(string); if !is_action { return {},.Invalid_Arguments }
    switch action {
    case "inspect": return .Inspect,.None
    case "play": return .Play,.None
    case "pause": return .Pause,.None
    case "resume": return .Resume,.None
    case "stop": return .Stop,.None
    }
    return {},.Invalid_Arguments
}
