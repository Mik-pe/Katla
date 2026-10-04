//! Light document fields decode before shared CPU/native scene staging.
package app

import editor "../editor"
import "core:encoding/json"
import ecs "../ecs"
import "core:math"

/// Converts strict Rust scene light descriptors to canonical registered components.
light_scene_decode :: proc(owner:^Authoring,row:^Scene_Entity,fields:json.Object)->editor.Scene_Error {
    if value,present:=scene_gameplay_present(fields,"point_light"); present {
        object,is_object:=value.(json.Object); if !is_object || !recipe_keys(object,{"color","intensity","range"}) || !scene_gameplay_required(object,{"color","intensity","range"}) { return .Decode_Failed }
        light:=point_light_default(); if !scene_gameplay_vector(object,"color",&light.color) || !scene_gameplay_number(object,"intensity",&light.intensity) || !scene_gameplay_number(object,"range",&light.range) { return .Decode_Failed }; if !point_light_valid(light) { return .Invalid_Field_Value }
        if error:=scene_row_component(owner,row,"PointLight",light); error!=.None { return error }
    }
    if value,present:=scene_gameplay_present(fields,"directional_light"); present {
        object,is_object:=value.(json.Object); if !is_object || !recipe_keys(object,{"direction","color","intensity"}) || !scene_gameplay_required(object,{"direction","color","intensity"}) { return .Decode_Failed }
        light:=directional_light_default(); if !scene_gameplay_vector(object,"direction",&light.direction) || !scene_gameplay_vector(object,"color",&light.color) || !scene_gameplay_number(object,"intensity",&light.intensity) { return .Decode_Failed }; if !directional_light_valid(light) { return .Invalid_Field_Value }
        maximum:=max(abs(light.direction[0]),abs(light.direction[1]),abs(light.direction[2])); light.direction/=maximum
        magnitude:=math.sqrt(light.direction[0]*light.direction[0]+light.direction[1]*light.direction[1]+light.direction[2]*light.direction[2]); light.direction/=magnitude
        if error:=scene_row_component(owner,row,"DirectionalLight",light); error!=.None { return error }
    }
    return .None
}
/// Emits authored light descriptors while omitting all renderer/native handles.
light_scene_encode :: proc(owner:^Authoring,row:Scene_Entity,fields:^json.Object)->editor.Scene_Error {
    for name in ([2]string{"PointLight","DirectionalLight"}) {
        if !scene_row_has(row,name) { continue }; value,ok:=scene_row_owned_decode(owner,row,name); defer scene_row_owned_destroy(owner,name,value); if !ok { return .Decode_Failed }
        if name=="PointLight" { light:=(cast(^Scene_Point_Light)value)^; if !point_light_valid(light) { return .Invalid_Field_Value }; scene_gameplay_store(fields,"point_light",trigger_json_value(light)) }
        else { light:=(cast(^Scene_Directional_Light)value)^; if !directional_light_valid(light) { return .Invalid_Field_Value }; scene_gameplay_store(fields,"directional_light",trigger_json_value(light)) }
    }
    return .None
}
/// Rejects invalid authored lights before a scene replacement can reach native resources.
light_scene_validate :: proc(owner:^Authoring,entities:[]ecs.Entity_Id)->editor.Scene_Error {
    for id in entities {
        if light,present:=ecs.get_component(&owner.world,id,Scene_Point_Light); present && !point_light_valid(light) { return .Invalid_Field_Value }
        if light,present:=ecs.get_component(&owner.world,id,Scene_Directional_Light); present && !directional_light_valid(light) { return .Invalid_Field_Value }
    }
    return .None
}
