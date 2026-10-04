//! Imported PBR material and scene selection belong to the application consumer.
package render

import app ".."
import km "../../math"

@(private="package")
model_active_nodes :: proc(model:^app.Gltf_Model,allocator:=context.allocator)->([]bool,Model_Batch_Error) {
    active:=make([]bool,len(model.nodes),allocator)
    scene:=model.default_scene; if scene<0 && len(model.scenes)>0 { scene=0 }
    if scene>=0 && int(scene)>=len(model.scenes) { delete(active,allocator); return nil,{kind=.Invalid_Scene,scene=.Invalid_Operation} }
    for i in 0..<len(model.nodes) {
        cursor:=i
        for depth:=0;cursor>=0;depth+=1 {
            if cursor>=len(model.nodes) || depth>=1024 { delete(active,allocator); return nil,{kind=.Invalid_Scene,scene=.Invalid_Operation} }
            if scene>=0 {
                for root in model.scenes[scene].roots { if int(root)>=len(model.nodes) { delete(active,allocator); return nil,{kind=.Invalid_Scene,scene=.Invalid_Operation} }; if u32(cursor)==root { active[i]=true; break } }
            } else if model.nodes[cursor].parent<0 { active[i]=true }
            if active[i] { break }; cursor=int(model.nodes[cursor].parent)
        }
    }
    return active,{}
}
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
model_vertex_uv :: proc(primitive:app.Gltf_Primitive,index:u32,view:app.Gltf_Texture_View)->(km.Vec2,Model_Batch_Error) {
    if view.texture<0 { return {},{} }
    if view.texcoord<0 || int(view.texcoord)>=len(primitive.uv_sets) || int(index)>=len(primitive.uv_sets[view.texcoord]) { return {},{kind=.Invalid_Material} }
    return app.gltf_texture_uv(view,primitive.uv_sets[view.texcoord][index]),{}
}
@(private="package")
model_object :: proc(model:km.Mat4,surface:app.Surface_Material,material:app.Gltf_Material)->(Model_Object,Model_Batch_Error) {
    ordinary,error:=object_data_matrix(model,surface); if error!=.None { return {},{kind=.Invalid_Scene,scene=.Invalid_Operation} }
    result:=Model_Object{model=ordinary.model,normal_model=ordinary.normal_model,base_color=material.base_color*ordinary.linear_color,factors={material.metallic*surface.metallic,material.roughness*surface.roughness,surface.ao,material.normal_texture.scale},emissive=km.vec4(material.emissive,material.alpha_cutoff),specular_glossiness=km.vec4(material.specular,material.glossiness),flags={u32(material.workflow),u32(material.alpha_mode),u32(material.unlit),u32(material.normal_texture.texture>=0)}}
    if material.workflow==.Specular_Glossiness { result.factors[1]=surface.roughness }
    return result,{}
}
