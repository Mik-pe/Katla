//! Portable material choices retain authoring provenance independently of native image ownership.
package app
import ecs "../ecs"
import editor "../editor"
import gfx "../gfx"
import resources "../resources"
import "core:math"
import "core:strings"
import "core:path/filepath"
import "core:unicode/utf8"

Material_Alpha_Mode :: enum { Opaque,Mask,Blend }
/// Surface factors are linear; coverage remains independent of base-color alpha.
Material_Surface :: struct { emissive_factor:[3]f32,normal_scale,occlusion_strength:f32,alpha_mode:Material_Alpha_Mode,alpha_cutoff:f32,double_sided:bool }
/// Coordinates apply scale, counterclockwise rotation in radians, then translation.
Uv_Transform :: struct { tex_coord:u32,offset:[2]f32,rotation:f32,scale:[2]f32 }
Texture_Sampling :: struct { uv:Uv_Transform,sampler:gfx.Sampler_Desc }
Material_Sampling :: struct { albedo,normal,metallic_roughness,occlusion,emission:Texture_Sampling }
Texture_Role :: enum { Albedo,Normal,Metallic_Roughness,Occlusion,Emission }
Texture_Source_Kind :: enum { Inherit,Neutral,File,GltfImage }
/// Paths retain their explicit asset capability scope; indices select actual glTF images.
Texture_Source :: struct { kind:Texture_Source_Kind,root:Mesh_Path_Root,path:string,image_index:u32 }
/// Every selected path is owned; native leases and decoded images belong to application resource services.
Texture_Assignments :: struct { albedo,normal,metallic_roughness,occlusion,emission:Texture_Source }
material_surface_default :: proc()->Material_Surface { return {normal_scale=1,occlusion_strength=1,alpha_cutoff=.5} }
uv_transform_default :: proc()->Uv_Transform { return {scale={1,1}} }
texture_sampling_default :: proc()->Texture_Sampling { return {uv=uv_transform_default(),sampler={min_filter=.Linear,mag_filter=.Linear,mip_filter=.Linear,address_u=.Repeat,address_v=.Repeat,address_w=.Repeat,min_lod=0,max_lod=32,max_anisotropy=1}} }
material_sampling_default :: proc()->Material_Sampling { role:=texture_sampling_default(); return {role,role,role,role,role} }
/// Omitted surface and sampling retain a source's original settings, or these conventional defaults.
material_surface_effective :: proc(material:Surface_Material,inherited:Material_Surface)->Material_Surface { return material.surface if material.has_surface else inherited }
material_sampling_effective :: proc(material:Surface_Material,inherited:Material_Sampling)->Material_Sampling { return material.sampling if material.has_sampling else inherited }
texture_assignments_roles :: proc(value:^Texture_Assignments)->[5]^Texture_Source { return {&value.albedo,&value.normal,&value.metallic_roughness,&value.occlusion,&value.emission} }
material_sampling_roles :: proc(value:Material_Sampling)->[5]Texture_Sampling { return {value.albedo,value.normal,value.metallic_roughness,value.occlusion,value.emission} }
uv_transform_valid :: proc(value:Uv_Transform)->bool {
    if value.tex_coord>1 || math.is_nan(value.rotation) || math.is_inf(value.rotation) { return false }
    for axis in ([4]f32{value.offset[0],value.offset[1],value.scale[0],value.scale[1]}) { if math.is_nan(axis) || math.is_inf(axis) { return false } }; return true
}
/// Negative and zero UV scales are authored values, not missing-coordinate markers.
uv_transform_apply :: proc(value:Uv_Transform,uv:[2]f32)->[2]f32 {
    sine,cosine:=math.sin(value.rotation),math.cos(value.rotation)
    return {cosine*value.scale[0]*uv[0]-sine*value.scale[1]*uv[1]+value.offset[0],sine*value.scale[0]*uv[0]+cosine*value.scale[1]*uv[1]+value.offset[1]}
}
material_surface_valid :: proc(value:Material_Surface)->bool {
    if value.alpha_mode not_in (bit_set[Material_Alpha_Mode]{.Opaque,.Mask,.Blend}) { return false }
    for factor in value.emissive_factor { if math.is_nan(factor) || math.is_inf(factor) || factor<0 { return false } }
    return !math.is_nan(value.normal_scale) && !math.is_inf(value.normal_scale) && value.occlusion_strength>=0 && value.occlusion_strength<=1 && !math.is_inf(value.alpha_cutoff) && value.alpha_cutoff>=0
}
material_sampling_valid :: proc(value:Material_Sampling)->bool {
    for role in material_sampling_roles(value) {
        sampler:=role.sampler
        if !uv_transform_valid(role.uv) || sampler.comparison || sampler.max_anisotropy<1 || sampler.max_anisotropy>16 || sampler.min_lod!=0 || (sampler.max_lod!=0 && sampler.max_lod!=32) { return false }
        if sampler.min_filter not_in (bit_set[gfx.Filter]{.Nearest,.Linear}) || sampler.mag_filter not_in (bit_set[gfx.Filter]{.Nearest,.Linear}) || sampler.mip_filter not_in (bit_set[gfx.Mip_Filter]{.None,.Nearest,.Linear}) { return false }
        if sampler.max_anisotropy>1 && (sampler.min_filter!=.Linear || sampler.mag_filter!=.Linear) { return false }
        for address in ([3]gfx.Address_Mode{sampler.address_u,sampler.address_v,sampler.address_w}) { if address not_in (bit_set[gfx.Address_Mode]{.Repeat,.Mirror_Repeat,.Clamp_Edge}) { return false } }
    }; return true
}
texture_source_valid :: proc(source:Texture_Source)->bool {
    switch source.kind {
    case .Inherit,.Neutral: return source.path=="" && source.image_index==0 && source.root==.Resource
    case .File,.GltfImage:
        if source.kind==.File && source.image_index!=0 { return false }
        if source.root==.Resource || source.root==.Project { return resources.valid_relative_path(source.path) }
        if source.root!=.File || !filepath.is_abs(source.path) || !utf8.valid_string(source.path) { return false }
        for character in source.path { if character==0 { return false } }; return true
    }
    return false
}
texture_assignments_valid :: proc(value:Texture_Assignments)->bool {
    for source in ([5]Texture_Source{value.albedo,value.normal,value.metallic_roughness,value.occlusion,value.emission}) { if !texture_source_valid(source) { return false } }; return true
}
texture_assignments_clone :: proc(value:Texture_Assignments,allocator:=context.allocator)->Texture_Assignments {
    result:=value; for source in texture_assignments_roles(&result) { source.path=strings.clone(source.path,allocator) }; return result
}
texture_assignments_destroy :: proc(value:^Texture_Assignments,allocator:=context.allocator) { for source in texture_assignments_roles(value) { delete(source.path,allocator) }; value^={} }
@(private="package")
material_textures_destroy_value :: proc(value:rawptr) { texture_assignments_destroy(cast(^Texture_Assignments)value) }
@(private="package")
material_textures_clone_value :: proc(dst,src:rawptr) { (cast(^Texture_Assignments)dst)^=texture_assignments_clone((cast(^Texture_Assignments)src)^) }
/// Registers optional durable image selections with deep ownership in history and scene staging.
material_textures_register :: proc(owner:^Authoring) { editor.editor_register(&owner.world,&owner.registry,"MaterialTextures",Texture_Assignments{},ecs.Value_Ops{material_textures_destroy_value,material_textures_clone_value},spawn_default=false) }
