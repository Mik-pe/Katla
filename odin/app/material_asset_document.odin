//! Reusable .katmat documents contain complete independent surface descriptions.
package app

import editor "../editor"
import km "../math"
import "core:encoding/json"
import "core:strings"

/// Owns a label and every image source path; factors and sampling are independent copies.
Material_Asset :: struct { version:u32,name:string,material:Surface_Material,textures:Texture_Assignments }
material_asset_destroy :: proc(value:^Material_Asset,allocator:=context.allocator) { delete(value.name,allocator); texture_assignments_destroy(&value.textures,allocator); value^={} }
/// Reads a strict version-one complete surface; omitted sampling and textures use neutral defaults.
material_asset_document_decode :: proc(owner:^Authoring,value:json.Value,origin:string)->(Material_Asset,editor.Scene_Error) {
    object,valid:=value.(json.Object); if !valid || !recipe_keys(object,{"version","name","values","sampling","textures"}) { return {},.Decode_Failed }
    version,is_version:=object["version"].(json.Integer); name,is_name:=object["name"].(string)
    if !is_version || version!=1 || !is_name || len(name)>256 || len(strings.trim_space(name))==0 || strings.contains(name,"\x00") { return {},.Invalid_Operation }
    result:=Material_Asset{version=1,name=strings.clone(name,owner.world.allocator)}; accepted:=false; defer { if !accepted { material_asset_destroy(&result,owner.world.allocator) } }
    values,is_values:=object["values"].(json.Object)
    if !is_values || !recipe_keys(values,{"base_color","metallic","roughness","ao","emissive_factor","normal_scale","occlusion_strength","alpha_mode","alpha_cutoff","double_sided"}) || len(values)!=10 { return {},.Decode_Failed }
    color,is_color:=recipe_vector(values["base_color"],4); if !is_color { return {},.Invalid_Field_Value }; for axis in color { if axis<0 || axis>1 { return {},.Invalid_Field_Value } }
    result.material.has_factors=true; result.material.has_tint=true; result.material.linear_color=km.color_to_linear({color[0],color[1],color[2],color[3]})
    for field in ([3]string{"metallic","roughness","ao"}) {
        number,is_number:=recipe_number(values[field]); if !is_number || number<0 || number>1 { return {},.Invalid_Field_Value }; switch field {
        case "metallic": result.material.metallic=number
        case "roughness": result.material.roughness=number
        case "ao": result.material.ao=number
        }
    }
    properties:=make(json.Object,owner.world.allocator); defer delete(properties)
    for key,raw in values { if key!="base_color" && key!="metallic" && key!="roughness" && key!="ao" { properties[key]=raw } }
    surface,is_surface:=material_surface_decode(properties); if !is_surface { return {},.Invalid_Field_Value }; result.material.surface=surface; result.material.has_surface=true
    result.material.sampling=material_sampling_default(); result.material.has_sampling=true
    if raw,present:=object["sampling"]; present { sampling,is_sampling:=material_sampling_decode(raw); if !is_sampling { return {},.Invalid_Field_Value }; result.material.sampling=sampling }
    for source in texture_assignments_roles(&result.textures) { source.kind=.Neutral }
    if raw,present:=object["textures"]; present { textures,is_textures:=material_textures_decode(owner,raw,origin,true); if !is_textures { return {},.Invalid_Operation }; result.textures=textures }
    for source in texture_assignments_roles(&result.textures) { if source.kind==.Inherit { return {},.Invalid_Operation } }
    accepted=true; return result,.None
}
/// Owns the complete public document, retaining each image source's destination-relative meaning.
material_asset_document_encode :: proc(owner:^Authoring,value:Material_Asset,origin:string)->(json.Value,bool) {
    result:=make(json.Object,owner.world.allocator); accepted:=false; defer { if !accepted { json.destroy_value(result) } }
    scene_json_put(&result,"version",json.Integer(1)); scene_json_put(&result,"name",strings.clone(value.name,owner.world.allocator))
    values:=material_surface_encode(value.material.surface).(json.Object)
    color:=km.color_to_srgb(value.material.linear_color)
    scene_json_put(&values,"base_color",trigger_json_value([4]f32{color.r,color.g,color.b,color.a}))
    scene_json_put(&values,"metallic",json.Float(value.material.metallic)); scene_json_put(&values,"roughness",json.Float(value.material.roughness)); scene_json_put(&values,"ao",json.Float(value.material.ao))
    scene_json_put(&result,"values",values); scene_json_put(&result,"sampling",material_sampling_encode(value.material.sampling))
    textures,valid:=material_textures_encode(owner,value.textures,origin); if !valid { return nil,false }; scene_json_put(&result,"textures",textures)
    accepted=true; return result,true
}
/// Image validation performs actual bounded decoding before a file is admitted or replaced.
material_asset_validate_images :: proc(owner:^Authoring,asset:^Material_Asset)->editor.Scene_Error {
    prepared,error:=material_images_prepare(owner,asset.textures); if error!=.None { return error }; defer material_images_destroy(&prepared,owner.world.allocator)
    return .None
}
