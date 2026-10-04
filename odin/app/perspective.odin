//! Scene cameras retain authored degree FOV and infinite reverse-Z projection.
package app

import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:encoding/json"

/// A camera's placement comes from its exact SceneTransform hierarchy.
Scene_Perspective :: struct {
    fov:f32 `min:"0" max:"180"`,
    near:f32 `min:"0"`,
    aspect_ratio:f32 `min:"0"`,
}
/// Matches the engine's authored camera defaults.
perspective_default :: proc()->Scene_Perspective { return {60,0.001,16.0/9.0} }
/// Rejects finite but singular projections as well as nonfinite parameters.
perspective_valid :: proc(value:Scene_Perspective)->bool {
    return mesh_finite(value.fov) && value.fov>0 && value.fov<180 && mesh_finite(value.near) && value.near>0 && mesh_finite(value.aspect_ratio) && value.aspect_ratio>0
}
/// Computes the canonical column-major Vulkan-depth projection without an authored far plane.
perspective_projection :: proc(value:Scene_Perspective)->(km.Mat4,bool) {
    if !perspective_valid(value) { return {},false }
    projection:=km.mat4_reverse_z(value.fov,value.aspect_ratio,value.near)
    for column in projection { for number in column { if !mesh_finite(number) { return {},false } } }
    return projection,true
}
/// Registers optional authored camera state independently from editor orbit controls.
perspective_register :: proc(owner:^Authoring) { editor.editor_register(&owner.world,&owner.registry,"Perspective",perspective_default(),spawn_default=false) }
/// Decodes the strict current scene camera descriptor.
perspective_scene_decode :: proc(owner:^Authoring,row:^Scene_Entity,fields:json.Object)->editor.Scene_Error {
    value,present:=scene_gameplay_present(fields,"perspective"); if !present { return .None }
    object,is_object:=value.(json.Object)
    if !is_object || !recipe_keys(object,{"fov","near","aspect_ratio"}) || !scene_gameplay_required(object,{"fov","near","aspect_ratio"}) { return .Decode_Failed }
    descriptor:Scene_Perspective
    if !scene_gameplay_number(object,"fov",&descriptor.fov) || !scene_gameplay_number(object,"near",&descriptor.near) || !scene_gameplay_number(object,"aspect_ratio",&descriptor.aspect_ratio) { return .Decode_Failed }
    if _,valid:=perspective_projection(descriptor); !valid { return .Invalid_Field_Value }
    return scene_row_component(owner,row,"Perspective",descriptor)
}
/// Emits only authored camera parameters into the built-in scene field.
perspective_scene_encode :: proc(owner:^Authoring,row:Scene_Entity,fields:^json.Object)->editor.Scene_Error {
    if !scene_row_has(row,"Perspective") { return .None }
    value,valid:=scene_row_owned_decode(owner,row,"Perspective"); defer scene_row_owned_destroy(owner,"Perspective",value); if !valid { return .Decode_Failed }
    descriptor:=(cast(^Scene_Perspective)value)^; if _,projection_valid:=perspective_projection(descriptor); !projection_valid { return .Invalid_Field_Value }
    scene_gameplay_store(fields,"perspective",trigger_json_value(descriptor)); return .None
}
/// Validates live camera state before native scene publication or command restoration.
perspective_scene_validate :: proc(owner:^Authoring,entities:[]ecs.Entity_Id)->editor.Scene_Error {
    for id in entities { if value,present:=ecs.get_component(&owner.world,id,Scene_Perspective); present { if _,valid:=perspective_projection(value); !valid { return .Invalid_Field_Value } } }
    return .None
}
