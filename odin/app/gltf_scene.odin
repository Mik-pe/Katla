//! Material slots, node transforms, scenes and skin joint identities preserve source indexing.
package app

import km "../math"
import cgltf "../deps/cgltf"
import "core:slice"

@(private="package")
gltf_texture_view :: proc(data:^cgltf.data,source:cgltf.texture_view)->(Gltf_Texture_View,Gltf_Error) {
    result:=Gltf_Texture_View{texture= -1,texcoord=i32(source.texcoord),scale=source.scale,uv_scale={1,1}}
    if source.texture==nil { return result,.None }
    result.texture=i32(cgltf.texture_index(data,source.texture))
    if source.has_transform {
        result.offset=source.transform.offset; result.uv_scale=source.transform.scale; result.rotation=source.transform.rotation
        if source.transform.has_texcoord { result.texcoord=i32(source.transform.texcoord) }
    }
    if result.texcoord<0 || result.texcoord>=32 || !mesh_vec_finite(result.offset) || !mesh_vec_finite(result.uv_scale) || !mesh_finite(result.rotation) || !mesh_finite(result.scale) { return {},.Invalid_Data }
    return result,.None
}
@(private="package")
gltf_extract_materials :: proc(data:^cgltf.data,model:^Gltf_Model)->Gltf_Error {
    model.samplers=make([]Gltf_Sampler,len(data.samplers)); model.textures=make([]Gltf_Texture,len(data.textures)); model.materials=make([]Gltf_Material,len(data.materials))
    for sampler,i in data.samplers { model.samplers[i]={i32(sampler.min_filter),i32(sampler.mag_filter),i32(sampler.wrap_s),i32(sampler.wrap_t)} }
    for &texture,i in data.textures {
        if texture.has_basisu || texture.image_==nil { return .Unsupported }
        model.textures[i]={image=i32(cgltf.image_index(data,texture.image_)),sampler= -1}
        if texture.sampler!=nil { model.textures[i].sampler=i32(cgltf.sampler_index(data,texture.sampler)) }
    }
    for source,i in data.materials {
        target:=&model.materials[i]
        target.name=gltf_name(source.name)
        target.base_color=source.pbr_metallic_roughness.base_color_factor
        target.metallic=source.pbr_metallic_roughness.metallic_factor; target.roughness=source.pbr_metallic_roughness.roughness_factor
        target.emissive=source.emissive_factor; target.alpha_mode=Gltf_Alpha_Mode(source.alpha_mode); target.alpha_cutoff=source.alpha_cutoff; target.double_sided=bool(source.double_sided); target.unlit=bool(source.unlit)
        if !mesh_vec_finite(target.base_color) || !mesh_vec_finite(target.emissive) || target.metallic<0 || target.metallic>1 || target.roughness<0 || target.roughness>1 || !mesh_finite(target.metallic) || !mesh_finite(target.roughness) || !mesh_finite(target.alpha_cutoff) { return .Invalid_Data }
        if source.has_emissive_strength { target.emissive*=source.emissive_strength.emissive_strength }
        if source.has_pbr_specular_glossiness {
            target.workflow=.Specular_Glossiness; target.base_color=source.pbr_specular_glossiness.diffuse_factor
            target.specular=source.pbr_specular_glossiness.specular_factor; target.glossiness=source.pbr_specular_glossiness.glossiness_factor
            if !mesh_vec_finite(target.specular) || !mesh_vec_finite(target.base_color) || !mesh_finite(target.glossiness) || target.glossiness<0 || target.glossiness>1 { return .Invalid_Data }
        }
        views:=[7]cgltf.texture_view{source.pbr_metallic_roughness.base_color_texture,source.normal_texture,source.pbr_metallic_roughness.metallic_roughness_texture,source.occlusion_texture,source.emissive_texture,source.pbr_specular_glossiness.diffuse_texture,source.pbr_specular_glossiness.specular_glossiness_texture}
        targets:=[7]^Gltf_Texture_View{&target.base_color_texture,&target.normal_texture,&target.metallic_roughness_texture,&target.occlusion_texture,&target.emissive_texture,&target.diffuse_texture,&target.specular_glossiness_texture}
        for view,j in views { converted,error:=gltf_texture_view(data,view); if error!=.None { return error }; targets[j]^=converted }
    }
    return .None
}
@(private="package")
gltf_local_transform :: proc(source:^cgltf.node)->(km.Transform,km.Mat4,Gltf_Error) {
    local:=km.transform()
    if source.has_translation { local.position=source.translation }
    if source.has_scale { local.scale=source.scale }
    if source.has_rotation { local.rotation=km.Quat(source.rotation) }
    local_matrix:km.Mat4; cgltf.node_transform_local(source,cast([^]f32)&local_matrix)
    for column in local_matrix { if !mesh_vec_finite(column) { return {},{},.Invalid_Data } }
    if source.has_matrix {
        // Source matrices remain authoritative; TRS is only the animation bind-pose carrier.
        local=km.transform(km.xyz(local_matrix[3]))
    }
    if !animation_transform_valid(local) { return {},{},.Invalid_Data }
    return local,local_matrix,.None
}
@(private="package")
gltf_extract_nodes :: proc(data:^cgltf.data,model:^Gltf_Model)->Gltf_Error {
    model.nodes=make([]Gltf_Node,len(data.nodes)); model.animation.bind_pose=make([]km.Transform,len(data.nodes)); model.animation.parents=make([]i32,len(data.nodes))
    for &source,i in data.nodes {
        target:=&model.nodes[i]; target.name=gltf_name(source.name); target.parent= -1; target.mesh= -1; target.skin= -1
        if source.parent!=nil { target.parent=i32(cgltf.node_index(data,source.parent)) }
        if source.mesh!=nil { target.mesh=i32(cgltf.mesh_index(data,source.mesh)) }
        if source.skin!=nil { target.skin=i32(cgltf.skin_index(data,source.skin)) }
        if source.has_mesh_gpu_instancing { return .Unsupported }
        local,local_matrix,error:=gltf_local_transform(&source); if error!=.None { return error }; target.local=local; target.local_matrix=local_matrix; target.matrix_authored=bool(source.has_matrix)
        weights:=source.weights; if len(weights)==0 && source.mesh!=nil { weights=source.mesh.weights }
        target.weights=slice.clone(weights)
        if len(target.weights)==0 && source.mesh!=nil && len(source.mesh.primitives)>0 { target.weights=make([]f32,len(source.mesh.primitives[0].targets)) }
        for weight in target.weights { if !mesh_finite(weight) { return .Invalid_Data } }
        model.animation.bind_pose[i]=local; model.animation.parents[i]=target.parent
    }
    matrices,matrix_error:=gltf_compose_world(model.nodes,nil)
    if matrix_error!=.None { return .Invalid_Data }; defer delete(matrices)
    for matrix_value,i in matrices { model.nodes[i].world_matrix=matrix_value }
    model.scenes=make([]Gltf_Scene,len(data.scenes))
    for source,i in data.scenes { target:=&model.scenes[i]; target.name=gltf_name(source.name); target.roots=make([]u32,len(source.nodes)); for node,j in source.nodes { target.roots[j]=u32(cgltf.node_index(data,node)) } }
    if data.scene!=nil { model.default_scene=i32(cgltf.scene_index(data,data.scene)) }
    return .None
}
@(private="package")
gltf_extract_skins :: proc(data:^cgltf.data,model:^Gltf_Model)->Gltf_Error {
    model.skins=make([]Gltf_Skin,len(data.skins))
    for source,i in data.skins {
        target:=&model.skins[i]; target.name=gltf_name(source.name); target.skeleton= -1
        if source.skeleton!=nil { target.skeleton=i32(cgltf.node_index(data,source.skeleton)) }
        if len(source.joints)==0 || len(source.joints)>MAX_GLTF_NODES { return .Invalid_Skin }
        target.joints=make([]u32,len(source.joints)); target.inverse_bind=make([]km.Mat4,len(source.joints))
        for node,j in source.joints { target.joints[j]=u32(cgltf.node_index(data,node)); target.inverse_bind[j]=km.identity(km.Mat4); for previous in target.joints[:j] { if previous==target.joints[j] { return .Invalid_Skin } } }
        if source.inverse_bind_matrices!=nil {
            values,error:=gltf_floats(source.inverse_bind_matrices,16,MAX_GLTF_NODES); if error!=.None { return error }; defer delete(values)
            if len(values)!=len(source.joints)*16 { return .Invalid_Skin }
            for &inverse_bind,j in target.inverse_bind { for c in 0..<4 { for r in 0..<4 { inverse_bind[c][r]=values[j*16+c*4+r] } } }
        }
    }
    for node in model.nodes {
        if node.skin<0 { continue }
        if node.mesh<0 || int(node.skin)>=len(model.skins) { return .Invalid_Skin }
        skin:=model.skins[node.skin]
        for primitive in model.primitives {
            if primitive.mesh!=u32(node.mesh) { continue }
            if len(primitive.joints)!=len(primitive.geometry.vertices) { return .Invalid_Skin }
            for joint in primitive.joints { for index in joint { if int(index)>=len(skin.joints) { return .Invalid_Skin } } }
        }
    }
    return .None
}
