//! Portable surface descriptors preserve independent image origins and role sampling.
package app

import gfx "../gfx"
import "core:encoding/json"
import "core:math"

@(private="package")
material_document_number :: proc(raw:json.Value)->(f32,bool) {
    value:f32
    if number,valid:=raw.(json.Integer); valid { value=f32(number) } else if floating,is_float:=raw.(json.Float); is_float { value=f32(floating) } else { return 0,false }
    return value,!math.is_nan(value) && !math.is_inf(value)
}
@(private="package")
material_document_vector :: proc(raw:json.Value,$N:int)->([N]f32,bool) {
    array,valid:=raw.(json.Array); if !valid || len(array)!=N { return {},false }; result:[N]f32
    for value,index in array { number,ok:=material_document_number(value); if !ok { return {},false }; result[index]=number }; return result,true
}
/// Reads complete authored surface properties, allowing the scene format's omitted defaults.
material_surface_decode :: proc(value:json.Value)->(Material_Surface,bool) {
    result:=material_surface_default()
    object,valid:=value.(json.Object); if !valid || !recipe_keys(object,{"emissive_factor","normal_scale","occlusion_strength","alpha_mode","alpha_cutoff","double_sided"}) { return {},false }
    if raw,present:=object["emissive_factor"]; present { vector,ok:=material_document_vector(raw,3); if !ok { return {},false }; result.emissive_factor=vector }
    for name in ([3]string{"normal_scale","occlusion_strength","alpha_cutoff"}) { if raw,present:=object[name]; present {
        number,ok:=material_document_number(raw); if !ok { return {},false }; switch name {
        case "normal_scale": result.normal_scale=number
        case "occlusion_strength": result.occlusion_strength=number
        case "alpha_cutoff": result.alpha_cutoff=number
        }
    } }
    if raw,present:=object["alpha_mode"]; present { text,ok:=raw.(string); if !ok { return {},false }; switch text {
        case "opaque": result.alpha_mode=.Opaque
        case "mask": result.alpha_mode=.Mask
        case "blend": result.alpha_mode=.Blend
        case: return {},false
        } }
    if raw,present:=object["double_sided"]; present { boolean,ok:=raw.(bool); if !ok { return {},false }; result.double_sided=boolean }
    return result,material_surface_valid(result)
}
/// Owns the public JSON surface representation, including lower-case coverage modes.
material_surface_encode :: proc(surface:Material_Surface)->json.Value {
    mode:="opaque"; if surface.alpha_mode==.Mask { mode="mask" }; if surface.alpha_mode==.Blend { mode="blend" }
    return trigger_json_value(struct {emissive_factor:[3]f32,normal_scale,occlusion_strength:f32,alpha_mode:string,alpha_cutoff:f32,double_sided:bool}{surface.emissive_factor,surface.normal_scale,surface.occlusion_strength,mode,surface.alpha_cutoff,surface.double_sided})
}
/// Reads one role's coordinate and sampling policy without conflating its selected image.
material_sampling_decode :: proc(value:json.Value)->(Material_Sampling,bool) {
    result:=material_sampling_default(); object,valid:=value.(json.Object)
    if !valid || !recipe_keys(object,{"albedo","normal","metallic_roughness","occlusion","emission"}) { return {},false }
    roles:=[5]^Texture_Sampling{&result.albedo,&result.normal,&result.metallic_roughness,&result.occlusion,&result.emission}
    for name,index in ([5]string{"albedo","normal","metallic_roughness","occlusion","emission"}) { if raw,present:=object[name]; present {
        fields,ok:=raw.(json.Object); if !ok || !recipe_keys(fields,{"uv","sampler"}) { return {},false }; target:=roles[index]
        if uv_raw,has_uv:=fields["uv"]; has_uv {
            uv,is_uv:=uv_raw.(json.Object); if !is_uv || !recipe_keys(uv,{"tex_coord","offset","rotation","scale"}) { return {},false }
            if n,has:=uv["tex_coord"]; has { number,is_number:=n.(json.Integer); if !is_number || number<0 || number>1 { return {},false }; target.uv.tex_coord=u32(number) }
            if n,has:=uv["rotation"]; has { number,is_number:=material_document_number(n); if !is_number { return {},false }; target.uv.rotation=number }
            for axis in ([2]string{"offset","scale"}) { if n,has:=uv[axis]; has { vector,is_vector:=material_document_vector(n,2); if !is_vector { return {},false }; if axis=="offset" { target.uv.offset=vector } else { target.uv.scale=vector } } }
        }
        if sampler_raw,has_sampler:=fields["sampler"]; has_sampler { sampler,sampler_ok:=material_sampler_decode(sampler_raw); if !sampler_ok { return {},false }; target.sampler=sampler }
    } }
    return result,material_sampling_valid(result)
}
@(private="package")
material_sampler_decode :: proc(value:json.Value)->(gfx.Sampler_Desc,bool) {
    result:=material_sampling_default().albedo.sampler
    object,valid:=value.(json.Object); if !valid || !recipe_keys(object,{"min_filter","mag_filter","mip_filter","address_u","address_v","address_w","comparison","anisotropy"}) { return {},false }
    if len(object)!=8 { return {},false }
    for name in ([3]string{"min_filter","mag_filter","mip_filter"}) {
        text,is_text:=object[name].(string); if !is_text { return {},false }; mode:gfx.Filter
        switch text {
        case "nearest": mode=.Nearest
        case "linear": mode=.Linear
        case "none": if name!="mip_filter" { return {},false }; result.max_lod=0; mode=.Nearest
        case: return {},false
        }
        switch name {
        case "min_filter": result.min_filter=mode
        case "mag_filter": result.mag_filter=mode
        case "mip_filter": result.mip_filter=.Linear if text=="linear" else .Nearest; if text=="none" { result.mip_filter=.None }
        }
    }
    for name in ([3]string{"address_u","address_v","address_w"}) {
        text,is_text:=object[name].(string); if !is_text { return {},false }; mode:gfx.Address_Mode
        switch text {
        case "repeat": mode=.Repeat
        case "clamp_to_edge": mode=.Clamp_Edge
        case "mirrored_repeat": mode=.Mirror_Repeat
        case: return {},false
        }
        switch name {
        case "address_u": result.address_u=mode
        case "address_v": result.address_v=mode
        case "address_w": result.address_w=mode
        }
    }
    if _,is_null:=object["comparison"].(json.Null); !is_null { return {},false }
    anisotropy,is_anisotropy:=object["anisotropy"].(json.Integer); if !is_anisotropy || anisotropy<1 || anisotropy>16 { return {},false }; result.max_anisotropy=u32(anisotropy)
    if anisotropy>1 && (result.min_filter!=.Linear || result.mag_filter!=.Linear) { return {},false }
    return result,true
}
/// Owns all five complete role policies using the public sampler vocabulary.
material_sampling_encode :: proc(value:Material_Sampling)->json.Value {
    names:=[5]string{"albedo","normal","metallic_roughness","occlusion","emission"}
    result:=make(json.Object,context.allocator)
    for role,index in ([5]Texture_Sampling{value.albedo,value.normal,value.metallic_roughness,value.occlusion,value.emission}) {
        sampler:=role.sampler; min:="nearest"; if sampler.min_filter==.Linear { min="linear" }; mag:="nearest"; if sampler.mag_filter==.Linear { mag="linear" }; mip:="nearest"; if sampler.mip_filter==.Linear { mip="linear" }; if sampler.max_lod==0 || sampler.mip_filter==.None { mip="none" }
        addresses:[3]string; for mode,i in ([3]gfx.Address_Mode{sampler.address_u,sampler.address_v,sampler.address_w}) { switch mode {
            case .Repeat: addresses[i]="repeat"
            case .Clamp_Edge: addresses[i]="clamp_to_edge"
            case .Mirror_Repeat: addresses[i]="mirrored_repeat"
            case .Clamp_Border: addresses[i]="clamp_to_edge"
            } }
        object:=make(json.Object,context.allocator); scene_json_put(&object,"uv",trigger_json_value(role.uv))
        scene_json_put(&object,"sampler",trigger_json_value(struct {min_filter,mag_filter,mip_filter,address_u,address_v,address_w:string,comparison:json.Value,anisotropy:u32}{min,mag,mip,addresses[0],addresses[1],addresses[2],json.Null{},sampler.max_anisotropy}))
        scene_json_put(&result,names[index],object)
    }
    return result
}
/// Resolves a strict portable image source into owned canonical asset identity.
material_source_decode :: proc(owner:^Authoring,value:json.Value,origin:string)->(Texture_Source,bool) {
    object,valid:=value.(json.Object); if !valid { return {},false }; kind,is_kind:=object["kind"].(string); if !is_kind { return {},false }
    result:Texture_Source
    switch kind {
    case "inherit": if !recipe_keys(object,{"kind"}) { return {},false }; result.kind=.Inherit
    case "neutral": if !recipe_keys(object,{"kind"}) { return {},false }; result.kind=.Neutral
    case "file","gltf_image":
        if kind=="file" { if !recipe_keys(object,{"kind","asset"}) { return {},false }; result.kind=.File }
        else { if !recipe_keys(object,{"kind","asset","image_index"}) { return {},false }; result.kind=.GltfImage; number,is_number:=object["image_index"].(json.Integer); if !is_number || number<0 || number>i64(max(u32)) { return {},false }; result.image_index=u32(number) }
        path,root,valid_path:=scene_asset_path(owner,object["asset"],origin); if !valid_path { return {},false }; result.path=path; result.root=root
    case: return {},false
    }
    return result,true
}
/// Encodes a source against the destination origin without changing the file's authority.
material_source_encode :: proc(owner:^Authoring,value:Texture_Source,origin:string)->(json.Value,bool) {
    switch value.kind {
    case .Inherit: return trigger_json_value(struct {kind:string}{"inherit"}),true
    case .Neutral: return trigger_json_value(struct {kind:string}{"neutral"}),true
    case .File,.GltfImage:
        reference,valid:=scene_asset_reference(owner,value.path,value.root,origin); if !valid { return nil,false }; defer json.destroy_value(reference)
        if value.kind==.File { return trigger_json_value(struct {kind:string,asset:json.Value}{"file",reference}),true }
        return trigger_json_value(struct {kind:string,asset:json.Value,image_index:u32}{"gltf_image",reference,value.image_index}),true
    }
    return nil,false
}
/// Decodes owned image selections; absent roles preserve inherited choices for scenes.
material_textures_decode :: proc(owner:^Authoring,value:json.Value,origin:string,neutral_default:bool=false)->(Texture_Assignments,bool) {
    result:Texture_Assignments; if neutral_default { result.albedo.kind=.Neutral; result.normal.kind=.Neutral; result.metallic_roughness.kind=.Neutral; result.occlusion.kind=.Neutral; result.emission.kind=.Neutral }
    accepted:=false; defer { if !accepted { texture_assignments_destroy(&result,owner.world.allocator) } }
    object,valid:=value.(json.Object); if !valid || !recipe_keys(object,{"albedo","normal","metallic_roughness","occlusion","emission"}) { return {},false }
    roles:=[5]^Texture_Source{&result.albedo,&result.normal,&result.metallic_roughness,&result.occlusion,&result.emission}
    for name,index in ([5]string{"albedo","normal","metallic_roughness","occlusion","emission"}) { if raw,present:=object[name]; present { source,ok:=material_source_decode(owner,raw,origin); if !ok { return {},false }; roles[index]^=source } }
    accepted=true; return result,true
}
/// Owns every role's explicit portable source JSON.
material_textures_encode :: proc(owner:^Authoring,value:Texture_Assignments,origin:string)->(json.Value,bool) {
    names:=[5]string{"albedo","normal","metallic_roughness","occlusion","emission"}
    result:=make(json.Object,owner.world.allocator); accepted:=false; defer { if !accepted { json.destroy_value(result) } }
    for source,index in ([5]Texture_Source{value.albedo,value.normal,value.metallic_roughness,value.occlusion,value.emission}) {
        raw,valid:=material_source_encode(owner,source,origin); if !valid { return nil,false }; scene_json_put(&result,names[index],raw)
    }
    accepted=true; return result,true
}
