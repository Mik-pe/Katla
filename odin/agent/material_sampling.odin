//! Portable material role and sampler requests never expose renderer resources.
package agent

import ecs "../ecs"
import "core:encoding/json"
import "core:math"
import "core:mem"

/// Stable five-role order shared by inspection, persisted assets and image assignment.
Material_Texture_Role :: enum { Albedo, Normal, Metallic_Roughness, Occlusion, Emission }
Material_Minification :: enum { Nearest, Linear, Nearest_Mipmap_Nearest, Linear_Mipmap_Nearest, Nearest_Mipmap_Linear, Linear_Mipmap_Linear }
Material_Magnification :: enum { Nearest, Linear }
Material_Texture_Wrap :: enum { Repeat, Clamp_To_Edge, Mirrored_Repeat }
Material_Sampling_Field :: enum { Tex_Coord, Offset, Rotation, Scale, Minification, Magnification, Wrap_U, Wrap_V, Anisotropy }
/// Presence distinguishes omitted/null fields from explicit zero or negative UV changes.
Material_Sampling_Patch :: struct {
    fields:bit_set[Material_Sampling_Field],tex_coord:u32,offset:[2]f32,rotation:f32,scale:[2]f32,
    minification:Material_Minification,magnification:Material_Magnification,
    wrap_u,wrap_v:Material_Texture_Wrap,anisotropy:u8,
}
/// Owns a target batch and patches exactly one role through the application owner.
Material_Set_Sampling :: struct { entities:[]ecs.Entity_Id,role:Material_Texture_Role,patch:Material_Sampling_Patch }
/// Returns the stable wire role name.
material_texture_role_name :: proc(role:Material_Texture_Role)->string {
    names:=[5]string{"albedo","normal","metallic_roughness","occlusion","emission"}
    if int(role)<0 || int(role)>=len(names) { return "" }; return names[int(role)]
}
@(private="package")
material_role :: proc(value:json.Value)->(Material_Texture_Role,bool) {
    text,valid:=value.(string); if !valid { return {},false }
    for role in Material_Texture_Role { if text==material_texture_role_name(role) { return role,true } }
    return {},false
}
@(private="package")
material_finite :: proc(value:json.Value)->(f32,bool) {
    number:f64
    #partial switch v in value {
    case json.Integer: number=f64(v)
    case json.Float: number=f64(v)
    case: return 0,false
    }
    return f32(number),number>=-f64(max(f32)) && number<=f64(max(f32))
}
@(private="package")
material_pair :: proc(value:json.Value)->([2]f32,bool) {
    array,valid:=value.(json.Array); if !valid || len(array)!=2 { return {},false }; result:[2]f32
    for item,index in array { result[index],valid=material_finite(item); if !valid { return {},false } }
    return result,true
}
/// Decodes one borrowed patch; omitted or null properties preserve their current values.
material_sampling_patch_decode :: proc(value:json.Value)->(Material_Sampling_Patch,bool) {
    object,okay:=value.(json.Object); if !okay { return {},false }
    names:=[9]string{"tex_coord","offset","rotation","scale","minification","magnification","wrap_u","wrap_v","anisotropy"}
    if !material_keys_valid(object,names[:]) { return {},false }
    result:Material_Sampling_Patch
    for key,index in names {
        item,present:=object[key]; if !present || material_is_null(item) { continue }; valid:=false
        switch Material_Sampling_Field(index) {
        case .Tex_Coord,.Anisotropy:
            number,finite:=material_finite(item); if !finite || number!=math.floor(number) { return {},false }
            if index==0 { if number<0 || number>1 { return {},false }; result.tex_coord=u32(number) } else { if number<1 || number>16 { return {},false }; result.anisotropy=u8(number) }; valid=true
        case .Offset: result.offset,valid=material_pair(item)
        case .Rotation: result.rotation,valid=material_finite(item)
        case .Scale: result.scale,valid=material_pair(item)
        case .Minification:
            text,is_text:=item.(string); values:=[6]string{"nearest","linear","nearest_mipmap_nearest","linear_mipmap_nearest","nearest_mipmap_linear","linear_mipmap_linear"}
            if is_text { for name,i in values { if text==name { result.minification=Material_Minification(i); valid=true; break } } }
        case .Magnification:
            text,is_text:=item.(string); if is_text && (text=="nearest" || text=="linear") { result.magnification=.Linear if text=="linear" else .Nearest; valid=true }
        case .Wrap_U,.Wrap_V:
            text,is_text:=item.(string); values:=[3]string{"repeat","clamp_to_edge","mirrored_repeat"}; wrap:Material_Texture_Wrap
            if is_text { for name,i in values { if text==name { wrap=Material_Texture_Wrap(i); valid=true; break } } }
            if index==7 { result.wrap_v=wrap } else { result.wrap_u=wrap }
        }
        if !valid { return {},false }; result.fields|={Material_Sampling_Field(index)}
    }
    return result,result.fields!={}
}
@(private="package")
material_entities :: proc(value:json.Value,allocator:mem.Allocator)->([]ecs.Entity_Id,bool) {
    array,valid:=value.(json.Array); if !valid || len(array)<1 || len(array)>256 { return nil,false }
    entities:=make([]ecs.Entity_Id,len(array),allocator); success:=false; defer { if !success { delete(entities,allocator) } }
    for item,index in array {
        text,is_text:=item.(string); if !is_text { return nil,false }; id,is_id:=parse_entity_id(text); if !is_id { return nil,false }
        for previous in entities[:index] { if previous==id { return nil,false } }; entities[index]=id
    }
    success=true; return entities,true
}
@(private="package")
material_extended_decode :: proc(object:json.Object,action:string,image_index:u64,allocator:mem.Allocator)->(Decoded_Material,Call_Error) {
    field:="patch" if action=="set_sampling" else "source"
    if !material_keys_valid(object,{"action","entity_ids","role",field}) { return {},.Invalid_Arguments }
    role,valid:=material_role(object["role"]); if !valid { return {},.Invalid_Arguments }
    entities,owned:=material_entities(object["entity_ids"],allocator); if !owned { return {},.Invalid_Arguments }
    success:=false; defer { if !success { delete(entities,allocator) } }
    if action=="set_sampling" {
        patch,okay:=material_sampling_patch_decode(object["patch"]); if !okay { return {},.Invalid_Arguments }
        success=true; return {Material_Set_Sampling{entities,role,patch},allocator},.None
    }
    source,okay:=material_texture_source_value(object["source"],image_index,allocator); if !okay { return {},.Invalid_Arguments }
    success=true; return {Material_Set_Texture{entities,role,source},allocator},.None
}
