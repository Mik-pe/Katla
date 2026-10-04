//! Authoring reads resolve optional surface overrides against the selected imported primitive.
package app
import ecs "../ecs"
import editor "../editor"
import agent "../agent"
import km "../math"

/// Returns effective editable values, including imported factors when authored overrides are absent.
material_entity_values :: proc(owner:^Authoring,id:ecs.Entity_Id)->(agent.Material_Values,editor.Scene_Error) {
    surface,error:=target_surface(&owner.world,id); if error!=.None { return {},error }
    component:=ecs.get_component_mut(&owner.world,id,Scene_Model)
    return material_source_values(surface,component)
}
/// Resolves factors from a decoded source revision without assuming document keys are live entity IDs.
material_source_values :: proc(surface:Surface_Material,model:^Scene_Model=nil)->(agent.Material_Values,editor.Scene_Error) {
    result:=material_values(surface)
    if model==nil { return result,.None }
    primitive:^Gltf_Primitive
    if model.source.kind==.Primitive { selected,valid:=scene_model_selected_primitive(model); if !valid { return {},.Invalid_Operation }; primitive=selected }
    else if model.source.kind==.Model && len(model.model.primitives)==1 { primitive=&model.model.primitives[0] }
    else { return {},.Invalid_Operation }
    material:=Gltf_Material{base_color={1,1,1,1},metallic=1,roughness=1,alpha_cutoff=.5,normal_texture={scale=1},occlusion_texture={scale=1}}
    if primitive.material>=0 { if int(primitive.material)>=len(model.model.materials) { return {},.Invalid_Operation }; material=model.model.materials[primitive.material] }
    inherited:=Material_Surface{material.emissive,material.normal_texture.scale,material.occlusion_texture.scale,Material_Alpha_Mode(material.alpha_mode),material.alpha_cutoff,material.double_sided}
    properties:=material_surface_effective(surface,inherited)
    result.emissive_factor=properties.emissive_factor; result.normal_scale=properties.normal_scale; result.occlusion_strength=properties.occlusion_strength
    result.alpha_mode=agent.Material_Alpha_Mode(properties.alpha_mode); result.alpha_cutoff=properties.alpha_cutoff; result.double_sided=properties.double_sided
    if !surface.has_factors {
        tint:=km.Color{1,1,1,1}; if surface.has_tint { tint=surface.linear_color }
        color:=km.color_to_srgb({material.base_color[0]*tint.r,material.base_color[1]*tint.g,material.base_color[2]*tint.b,material.base_color[3]*tint.a})
        result.base_color={color.r,color.g,color.b,color.a}; result.metallic=material.metallic*surface.metallic; result.roughness=material.roughness*surface.roughness
    }
    return result,.None
}
@(private="package")
material_primitive_defaults :: proc(source:^Scene_Model,primitive:Gltf_Primitive)->Surface_Material {
    material:=Gltf_Material{base_color={1,1,1,1},metallic=1,roughness=1,alpha_cutoff=.5,normal_texture={scale=1},occlusion_texture={scale=1}}
    if primitive.material>=0 && int(primitive.material)<len(source.model.materials) { material=source.model.materials[primitive.material] }
    return Surface_Material{linear_color={material.base_color[0],material.base_color[1],material.base_color[2],material.base_color[3]},has_tint=true,has_factors=true,metallic=material.metallic,roughness=material.roughness,ao=1,
        has_surface=true,surface={material.emissive,material.normal_texture.scale,material.occlusion_texture.scale,Material_Alpha_Mode(material.alpha_mode),material.alpha_cutoff,material.double_sided}}
}
