//! Imported PBR material and scene selection belong to the application consumer.
package render

import app ".."
import km "../../math"
import gfx "../../gfx"
import ecs "../../ecs"

@(private="package")
model_material :: proc(model:^app.Gltf_Model,index:i32)->(app.Gltf_Material,Model_Batch_Error) {
    if index>=0 { if int(index)>=len(model.materials) { return {},{kind=.Invalid_Material} }; return model.materials[index],{} }
    material:=app.Gltf_Material{base_color={1,1,1,1},metallic=1,roughness=1,alpha_cutoff=0.5}
    absent:=app.Gltf_Texture_View{texture= -1,uv_scale={1,1},scale=1}
    material.base_color_texture=absent; material.normal_texture=absent; material.metallic_roughness_texture=absent; material.occlusion_texture=absent; material.emissive_texture=absent; material.diffuse_texture=absent; material.specular_glossiness_texture=absent
    return material,{}
}
@(private="package")
model_texture_views :: proc(material:app.Gltf_Material)->[5]app.Gltf_Texture_View {
    if material.workflow==.Specular_Glossiness { return {material.diffuse_texture,material.normal_texture,material.specular_glossiness_texture,material.occlusion_texture,material.emissive_texture} }
    return {material.base_color_texture,material.normal_texture,material.metallic_roughness_texture,material.occlusion_texture,material.emissive_texture}
}
@(private="package")
model_vertex_uv :: proc(primitive:app.Gltf_Primitive,index:u32,view:app.Gltf_Texture_View,required:=false)->(km.Vec2,Model_Batch_Error) {
    if !required && view.texture<0 { return {},{} }
    if view.texcoord<0 || int(view.texcoord)>=len(primitive.uv_sets) || int(index)>=len(primitive.uv_sets[view.texcoord]) { return {},{kind=.Invalid_Material} }
    return app.gltf_texture_uv(view,primitive.uv_sets[view.texcoord][index]),{}
}
@(private="package")
model_object :: proc(model:km.Mat4,surface:app.Surface_Material,material:app.Gltf_Material)->(Model_Object,Model_Batch_Error) {
    ordinary,error:=object_data_matrix(model,surface); if error!=.None { return {},{kind=.Invalid_Scene,scene=.Invalid_Operation} }
    result:=Model_Object{model=ordinary.model,normal_model=ordinary.normal_model,base_color=material.base_color*ordinary.linear_color,factors={material.metallic*surface.metallic,material.roughness*surface.roughness,surface.ao,material.normal_texture.scale},emissive=km.vec4(material.emissive,material.alpha_cutoff),specular_glossiness=km.vec4(material.specular,material.glossiness),flags={u32(material.workflow),u32(material.alpha_mode),u32(material.unlit),u32(material.normal_texture.texture>=0)}}
    if surface.has_factors {
        result.base_color=ordinary.linear_color
        result.factors[0]=surface.metallic;result.factors[1]=surface.roughness
    }
    if material.workflow==.Specular_Glossiness { result.factors[1]=surface.roughness }
    return result,{}
}

@(private="package")
model_material_surface :: proc(material:app.Gltf_Material,surface:app.Surface_Material)->app.Gltf_Material {
    result:=material
    value:=app.material_surface_effective(surface,app.Material_Surface{emissive_factor=material.emissive,normal_scale=material.normal_texture.scale,occlusion_strength=material.occlusion_texture.scale,alpha_mode=app.Material_Alpha_Mode(material.alpha_mode),alpha_cutoff=material.alpha_cutoff,double_sided=material.double_sided})
    result.emissive=value.emissive_factor
    result.normal_texture.scale=value.normal_scale
    result.occlusion_texture.scale=value.occlusion_strength
    result.alpha_mode=app.Gltf_Alpha_Mode(value.alpha_mode)
    result.alpha_cutoff=value.alpha_cutoff
    result.double_sided=value.double_sided
    return result
}
@(private="package")
model_material_sampling :: proc(material:app.Gltf_Material,surface:app.Surface_Material)->([5]app.Gltf_Texture_View,[5]gfx.Sampler_Desc) {
    views:=model_texture_views(material);samplers:[5]gfx.Sampler_Desc
    if surface.has_sampling {
        for role,i in app.material_sampling_roles(surface.sampling) {
            views[i].texcoord=i32(role.uv.tex_coord);views[i].offset=role.uv.offset
            views[i].uv_scale=role.uv.scale;views[i].rotation=role.uv.rotation
            samplers[i]=role.sampler
        }
    }
    return views,samplers
}

@(private="package")
model_entry_images :: proc(owner:^app.Authoring,entry:^Model_Entry,object:^Model_Object)->Model_Batch_Error {
    images,present:=ecs.get_component(&owner.world,entry.entity,app.Material_Images)
    if !present {
        choices,selected:=ecs.get_component(&owner.world,entry.entity,app.Texture_Assignments)
        if selected { for role in app.texture_assignments_roles(&choices) { if role.kind!=.Inherit { return {kind=.Invalid_Material} } } }
        return {}
    }
    for role,i in images.roles { entry.material_sources[i]=role.source.kind;entry.material_digests[i]=role.digest }
    if entry.material_sources[1]!=.Inherit { object.flags[3]|=1 }
    return {}
}

@(private="package")
model_uv_equal :: proc(a,b:app.Gltf_Texture_View)->bool { return a.texcoord==b.texcoord && a.offset==b.offset && a.uv_scale==b.uv_scale && a.rotation==b.rotation }
