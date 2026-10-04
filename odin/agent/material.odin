//! Lossless material authoring requests shared by host calls and application owners.
package agent

import ecs "../ecs"
import "core:encoding/json"
import "core:mem"

/// Authoring factors use sRGB color and linear metallic/roughness/occlusion values.
Material_Alpha_Mode :: enum { Opaque, Mask, Blend }
/// Returns the lowercase portable coverage policy used by inspection and scene documents.
material_alpha_mode_name :: proc(mode:Material_Alpha_Mode)->string {
    switch mode {
    case .Opaque: return "opaque"
    case .Mask: return "mask"
    case .Blend: return "blend"
    }
    return ""
}
/// Complete portable PBR factors; emission uses linear RGB and normal scale is signed.
Material_Values :: struct { base_color:[4]f32, metallic,roughness,ao:f32, emissive_factor:[3]f32, normal_scale:f32, occlusion_strength:f32, alpha_mode:Material_Alpha_Mode, alpha_cutoff:f32, double_sided:bool }
/// Named flat PBR factors; textures and geometry belong to application resources.
Material_Preset :: enum { Plaster, Oak, Concrete, Ceramic, Brushed_Metal, Fabric }
/// Fields explicitly patched after an optional preset.
Material_Field :: enum { Base_Color, Metallic, Roughness, AO, Emissive_Factor, Normal_Scale, Occlusion_Strength, Alpha_Mode, Alpha_Cutoff, Double_Sided }
/// Lists the common preset library.
Material_Presets :: struct {}
/// Reads one existing scene surface.
Material_Inspect :: struct { entity:ecs.Entity_Id }
/// Patches 1..256 unique surfaces as one application-owned undo step.
Material_Set :: struct { entities:[]ecs.Entity_Id, preset:Material_Preset, has_preset:bool, fields:bit_set[Material_Field], values:Material_Values }
/// Exactly one material request, with no GPU handle exposed in transport.
Material_Op :: union { Material_Presets, Material_Inspect, Material_Set, Material_Set_Sampling, Material_Set_Texture }
/// Owns any entity array decoded from transport.
Decoded_Material :: struct { operation:Material_Op, allocator:mem.Allocator }
/// Releases the decoded array using its captured allocator.
decoded_material_destroy :: proc(decoded:^Decoded_Material) {
    #partial switch op in decoded.operation {
    case Material_Set: delete(op.entities,decoded.allocator)
    case Material_Set_Sampling: delete(op.entities,decoded.allocator)
    case Material_Set_Texture: delete(op.entities,decoded.allocator); delete(op.source.path,decoded.allocator)
    }
    decoded^={}
}
/// Converts a preset to stable per-object factors.
material_preset_values :: proc(preset:Material_Preset)->Material_Values {
    value:=Material_Values{ao=1,normal_scale=1,occlusion_strength=1,alpha_cutoff=0.5}
    switch preset {
    case .Plaster: value.base_color={0.88,0.85,0.79,1}; value.roughness=0.9
    case .Oak: value.base_color={0.55,0.35,0.18,1}; value.roughness=0.6
    case .Concrete: value.base_color={0.48,0.49,0.5,1}; value.roughness=0.95
    case .Ceramic: value.base_color={0.9,0.93,0.94,1}; value.roughness=0.18
    case .Brushed_Metal: value.base_color={0.68,0.72,0.76,1}; value.metallic=1; value.roughness=0.32
    case .Fabric: value.base_color={0.28,0.38,0.42,1}; value.roughness=1
    }
    return value
}
/// Returns the transport spelling for one named preset.
material_preset_name :: proc(preset:Material_Preset)->string {
    switch preset {
    case .Plaster: return "plaster"
    case .Oak: return "oak"
    case .Concrete: return "concrete"
    case .Ceramic: return "ceramic"
    case .Brushed_Metal: return "brushed_metal"
    case .Fabric: return "fabric"
    }
    return ""
}
/// Validates all channels, including alpha, without accepting NaN or infinity.
material_values_valid :: proc(values:Material_Values)->bool {
    for factor in values.base_color { if !(factor>=0 && factor<=1) { return false } }
    for factor in ([3]f32{values.metallic,values.roughness,values.ao}) { if !(factor>=0 && factor<=1) { return false } }
    for factor in values.emissive_factor { if !(factor>=0 && factor<=max(f32)) { return false } }
    if !(values.normal_scale>=-max(f32) && values.normal_scale<=max(f32)) || !(values.alpha_cutoff>=0 && values.alpha_cutoff<=max(f32)) || !(values.occlusion_strength>=0 && values.occlusion_strength<=1) { return false }
    return values.alpha_mode>=.Opaque && values.alpha_mode<=.Blend
}
@(private="package")
material_is_null :: proc(value:json.Value)->bool { _,ok:=value.(json.Null); return ok }
@(private="package")
material_factor :: proc(value:json.Value)->(f32,bool) {
    number:f64
    #partial switch v in value {
    case json.Integer: number=f64(v)
    case json.Float: number=f64(v)
    case: return 0,false
    }
    return f32(number),number>=0 && number<=1
}
@(private="package")
material_keys_valid :: proc(object:json.Object,allowed:[]string)->bool {
    for key in object {
        found:=false; for name in allowed { if key==name { found=true; break } }
        if !found { return false }
    }
    return true
}
/// Rejects malformed or unknown fields before any scene request can enter its mailbox.
decode_material :: proc(data:[]byte,allocator:=context.allocator)->(Decoded_Material,Call_Error) {
    context.allocator=allocator
    image_index,validation:=material_arguments_validate(data,allocator); if validation!=.None { return {},validation }
    tree,err:=json.parse(data,spec=.JSON,parse_integers=false,allocator=allocator)
    if err!=nil { return {},.Invalid_JSON }; defer json.destroy_value(tree)
    object,ok:=tree.(json.Object); if !ok { return {},.Invalid_Arguments }
    action,valid:=required_string(object,"action"); if !valid { return {},.Invalid_Arguments }
    if action=="presets" {
        if !material_keys_valid(object,{"action"}) { return {},.Invalid_Arguments }
        return {Material_Presets{},allocator},.None
    }
    if action=="inspect" {
        if !material_keys_valid(object,{"action","entity_id"}) { return {},.Invalid_Arguments }
        text,present:=required_string(object,"entity_id"); if !present { return {},.Invalid_Arguments }
        id,exists:=parse_entity_id(text); if !exists { return {},.Invalid_Arguments }
        return {Material_Inspect{id},allocator},.None
    }
    if action=="set_sampling" || action=="set_texture" { return material_extended_decode(object,action,image_index,allocator) }
    if action!="set" || !material_keys_valid(object,{"action","entity_ids","preset","base_color","metallic","roughness","ao","emissive_factor","normal_scale","occlusion_strength","alpha_mode","alpha_cutoff","double_sided"}) { return {},.Invalid_Arguments }
    array,is_array:=object["entity_ids"].(json.Array)
    if !is_array || len(array)<1 || len(array)>256 { return {},.Invalid_Arguments }
    op:=Material_Set{entities=make([]ecs.Entity_Id,len(array),allocator)}
    success:=false; defer { if !success { delete(op.entities,allocator) } }
    for value,i in array {
        text,is_text:=value.(string); if !is_text { return {},.Invalid_Arguments }
        id,is_id:=parse_entity_id(text); if !is_id { return {},.Invalid_Arguments }
        for previous in op.entities[:i] { if previous==id { return {},.Invalid_Arguments } }
        op.entities[i]=id
    }
    if value,present:=object["preset"]; present && !material_is_null(value) {
        text,is_text:=value.(string); if !is_text { return {},.Invalid_Arguments }
        found:=false
        for preset in Material_Preset { if text==material_preset_name(preset) { op.preset=preset; found=true; break } }
        if !found { return {},.Invalid_Arguments }; op.has_preset=true
    }
    for key,i in ([4]string{"base_color","metallic","roughness","ao"}) {
        value,present:=object[key]; if !present || material_is_null(value) { continue }
        if i==0 {
            color,is_color:=value.(json.Array); if !is_color || len(color)!=4 { return {},.Invalid_Arguments }
            for channel,j in color { factor,is_factor:=material_factor(channel); if !is_factor { return {},.Invalid_Arguments }; op.values.base_color[j]=factor }
        } else {
            factor,is_factor:=material_factor(value); if !is_factor { return {},.Invalid_Arguments }
            switch i {
            case 1: op.values.metallic=factor
            case 2: op.values.roughness=factor
            case 3: op.values.ao=factor
            }
        }
        op.fields|={Material_Field(i)}
    }
    for key,i in ([6]string{"emissive_factor","normal_scale","occlusion_strength","alpha_mode","alpha_cutoff","double_sided"}) {
        value,present:=object[key]; if !present || material_is_null(value) { continue }
        switch i {
        case 0:
            color,is_color:=value.(json.Array); if !is_color || len(color)!=3 { return {},.Invalid_Arguments }
            for channel,j in color { factor,is_factor:=material_finite(channel); if !is_factor || factor<0 { return {},.Invalid_Arguments }; op.values.emissive_factor[j]=factor }
        case 1: factor,field_valid:=material_finite(value); if !field_valid { return {},.Invalid_Arguments }; op.values.normal_scale=factor
        case 2: factor,field_valid:=material_factor(value); if !field_valid { return {},.Invalid_Arguments }; op.values.occlusion_strength=factor
        case 3:
            mode,field_valid:=value.(string); if !field_valid { return {},.Invalid_Arguments }
            switch mode {
            case "opaque": op.values.alpha_mode=.Opaque
            case "mask": op.values.alpha_mode=.Mask
            case "blend": op.values.alpha_mode=.Blend
            case: return {},.Invalid_Arguments
            }
        case 4: factor,field_valid:=material_finite(value); if !field_valid || factor<0 { return {},.Invalid_Arguments }; op.values.alpha_cutoff=factor
        case 5: sided,field_valid:=value.(bool); if !field_valid { return {},.Invalid_Arguments }; op.values.double_sided=sided
        }
        op.fields|={Material_Field(i+4)}
    }
    if !op.has_preset && op.fields=={} { return {},.Invalid_Arguments }
    success=true; return {op,allocator},.None
}
