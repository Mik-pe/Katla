//! Effective imported role settings are resolved from actual selected glTF geometry.
package app

import ecs "../ecs"
import editor "../editor"
import gfx "../gfx"
import agent "../agent"

@(private="package")
material_imported_primitive :: proc(owner:^Authoring,id:ecs.Entity_Id)->(^Scene_Model,^Gltf_Primitive,editor.Scene_Error) {
    component:=ecs.get_component_mut(&owner.world,id,Scene_Model); if component==nil { return nil,nil,.None }
    if component.source.kind==.Primitive { primitive,valid:=scene_model_selected_primitive(component); if !valid { return component,nil,.Invalid_Operation }; return component,primitive,.None }
    if component.source.kind==.Group || len(component.model.primitives)!=1 { return component,nil,.Invalid_Operation }
    return component,&component.model.primitives[0],.None
}
@(private="package")
material_imported_material :: proc(owner:^Authoring,id:ecs.Entity_Id)->(^Scene_Model,Gltf_Material,editor.Scene_Error) {
    model,primitive,error:=material_imported_primitive(owner,id); if error!=.None { return nil,{},error }
    if model==nil { return nil,{},.None }
    if primitive.material>=0 { if int(primitive.material)>=len(model.model.materials) { return nil,{},.Invalid_Operation }; return model,model.model.materials[primitive.material],.None }
    absent:=Gltf_Texture_View{texture= -1,uv_scale={1,1},scale=1}
    return model,Gltf_Material{base_color={1,1,1,1},metallic=1,roughness=1,alpha_cutoff=.5,base_color_texture=absent,normal_texture=absent,metallic_roughness_texture=absent,occlusion_texture=absent,emissive_texture=absent},.None
}
material_imported_views :: proc(material:Gltf_Material)->[5]Gltf_Texture_View {
    if material.workflow==.Specular_Glossiness { return {material.diffuse_texture,material.normal_texture,material.specular_glossiness_texture,material.occlusion_texture,material.emissive_texture} }
    return {material.base_color_texture,material.normal_texture,material.metallic_roughness_texture,material.occlusion_texture,material.emissive_texture}
}
@(private="package")
material_gltf_sampler :: proc(source:Gltf_Sampler)->gfx.Sampler_Desc {
    result:=texture_sampling_default().sampler
    switch source.min_filter {
    case 9728: result.min_filter=.Nearest; result.mip_filter=.None; result.max_lod=0
    case 9729: result.min_filter=.Linear; result.mip_filter=.None; result.max_lod=0
    case 9984: result.min_filter=.Nearest; result.mip_filter=.Nearest
    case 9985: result.min_filter=.Linear; result.mip_filter=.Nearest
    case 9986: result.min_filter=.Nearest; result.mip_filter=.Linear
    case 0,9987: result.min_filter=.Linear; result.mip_filter=.Linear
    }
    result.mag_filter=.Nearest if source.mag_filter==9728 else .Linear
    for mode,index in ([2]i32{source.wrap_s,source.wrap_t}) { address:gfx.Address_Mode=.Repeat; if mode==33071 { address=.Clamp_Edge }; if mode==33648 { address=.Mirror_Repeat }; if index==0 { result.address_u=address } else { result.address_v=address } }
    return result
}
/// Reads authored overrides or the selected source's actual independent imported policies.
material_effective_sampling :: proc(owner:^Authoring,id:ecs.Entity_Id)->(Material_Sampling,editor.Scene_Error) {
    surface,present:=ecs.get_component(&owner.world,id,Surface_Material); if !present { return {},.Component_Not_Found }; if surface.has_sampling { return surface.sampling,.None }
    result:=material_sampling_default()
    model,material,error:=material_imported_material(owner,id); if error!=.None { return {},error }; if model==nil { return result,.None }
    roles:=[5]^Texture_Sampling{&result.albedo,&result.normal,&result.metallic_roughness,&result.occlusion,&result.emission}
    for view,index in material_imported_views(material) {
        if view.texture<0 { continue }; if int(view.texture)>=len(model.model.textures) || view.texcoord<0 || view.texcoord>1 { return {},.Invalid_Operation }
        target:=roles[index]; target.uv={u32(view.texcoord),view.offset,view.rotation,view.uv_scale}
        sampler_index:=model.model.textures[view.texture].sampler
        if sampler_index>=0 { if int(sampler_index)>=len(model.model.samplers) { return {},.Invalid_Operation }; target.sampler=material_gltf_sampler(model.model.samplers[sampler_index]) }
    }
    return result,.None
}
/// Rejects selected coordinate sets absent from the actual target geometry.
material_target_uv :: proc(owner:^Authoring,id:ecs.Entity_Id,role:agent.Material_Texture_Role,tex_coord:u32)->bool {
    if tex_coord>1 { return false }
    component,primitive,error:=material_imported_primitive(owner,id); if error!=.None { return false }
    if component!=nil { return int(tex_coord)<len(primitive.uv_sets) && len(primitive.uv_sets[tex_coord])==len(primitive.geometry.vertices) }
    mesh,present:=ecs.get_component(&owner.world,id,Scene_Mesh); return present && tex_coord==0 && len(mesh.geometry.vertices)>0
}
