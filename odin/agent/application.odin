//! Application requests validate their typed transport before entering the scene mailbox.
package agent

import asset "assets"
import scene "scene"
import "core:mem"

@(private="package")
decode_application_call :: proc(call:Tool_Call,allocator:mem.Allocator)->(Decoded_Call,Call_Error) {
    switch call.name {
    case "material":
        decoded,err:=decode_material(call.arguments,allocator); if err!=.None { return {},err }; decoded_material_destroy(&decoded)
    case "animation":
        decoded,err:=scene.decode_animation(call.arguments,allocator); if err!=.None { return {},.Invalid_JSON if err==.Invalid_JSON else .Invalid_Arguments }; scene.decoded_animation_destroy(&decoded)
    case "simulation":
        _,err:=scene.decode_simulation(call.arguments,allocator); if err!=.None { return {},.Invalid_JSON if err==.Invalid_JSON else .Invalid_Arguments }
    case "behavior":
        decoded,err:=scene.decode_behavior(call.arguments,allocator); if err!=.None { return {},.Invalid_JSON if err==.Invalid_JSON else .Invalid_Arguments }; scene.decoded_behavior_destroy(&decoded)
    case "trigger":
        decoded,err:=scene.decode_trigger(call.arguments,allocator); if err!=.None { return {},.Invalid_JSON if err==.Invalid_JSON else .Invalid_Arguments }; scene.decoded_trigger_destroy(&decoded)
    case "prefab":
        decoded,err:=asset.prefab_decode(call.arguments,allocator); if err!=.None { return {},.Invalid_JSON if err==.Invalid_JSON else .Invalid_Arguments }; asset.prefab_destroy(&decoded)
    case "load_scene","save_scene":
        decoded,err:=asset.scene_file_decode(call.name,call.arguments,allocator); if err!=.None { return {},.Invalid_JSON if err==.Invalid_JSON else .Invalid_Arguments }; asset.scene_file_destroy(&decoded)
    case "search_assets","list_resources","read_resource":
        decoded,err:=asset.decode(call.name,call.arguments,allocator); if err!=.None { return {},.Invalid_JSON if err==.Invalid_JSON else .Invalid_Arguments }; asset.destroy(&decoded)
    case "create_resource","write_resource":
        decoded,err:=asset.resource_write_decode(call.name,call.arguments,allocator); if err!=.None { return {},.Invalid_JSON if err==.Invalid_JSON else .Invalid_Arguments }; asset.resource_write_destroy(&decoded)
    case: return {},.Unknown_Tool
    }
    bytes:=make([]byte,len(call.arguments),allocator); copy(bytes,call.arguments)
    return {operation={kind=.Application,tool_name=call.name,value=bytes},field_bytes=bytes,allocator=allocator},.None
}
