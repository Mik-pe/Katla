//! Imported node timelines and skin matrices use the canonical animation sampler.
package app

import km "../math"
import editor "../editor"
import cgltf "../deps/cgltf"
import "core:fmt"
import "core:strings"
import "core:math"

@(private="package")
gltf_extract_animation :: proc(data:^cgltf.data,model:^Gltf_Model,budget:^int)->Gltf_Error {
    model.animation.clips=make([]Animation_Clip,len(data.animations))
    for source,i in data.animations {
        clip:=&model.animation.clips[i]
        clip.name=gltf_name(source.name)
        if strings.trim_space(clip.name)=="" { delete(clip.name); clip.name=fmt.aprintf("Animation_%d",i) }
        for previous in model.animation.clips[:i] { if previous.name==clip.name { old:=clip.name; clip.name=fmt.aprintf("%s_%d",old,i); delete(old); break } }
        if len(source.channels)>MAX_GLTF_NODES { return .Limit }
        clip.channels=make([]Animation_Channel,len(source.channels))
        for channel,j in source.channels {
            target:=&clip.channels[j]
            if channel.target_node==nil || channel.sampler==nil || channel.sampler.input==nil || channel.sampler.output==nil { return .Invalid_Animation }
            sampler:=channel.sampler
            if sampler.input.component_type!=.r_32f || sampler.output.component_type!=.r_32f { return .Invalid_Animation }
            cost:=u64(sampler.input.count)*4+u64(sampler.output.count)*max(16,u64(cgltf.num_components(sampler.output.type))*4)
            if cost>u64(budget^) { return .Limit }; budget^-=int(cost)
            target.node=u32(cgltf.node_index(data,channel.target_node))
            switch sampler.interpolation {
            case .linear: target.interpolation=.Linear
            case .step: target.interpolation=.Step
            case .cubic_spline: target.interpolation=.Cubic_Spline
            }
            times,time_error:=gltf_floats(sampler.input,1); if time_error!=.None { return time_error }; target.times=times
            for time,k in times { if time<0 || (k>0 && time<=times[k-1]) { return .Invalid_Animation }; clip.duration=max(clip.duration,time) }
            stride:=3 if target.interpolation==.Cubic_Spline else 1
            components:uint
            switch channel.target_path {
            case .translation: target.path=.Translation; components=3
            case .rotation: target.path=.Rotation; components=4
            case .scale: target.path=.Scale; components=3
            case .weights: target.path=.Weights; components=1
            case .invalid: return .Invalid_Animation
            }
            if target.path!=.Weights && model.nodes[target.node].matrix_authored { return .Invalid_Animation }
            values,value_error:=gltf_floats(sampler.output,components,MAX_MESH_INDICES); if value_error!=.None { return value_error }
            if target.path==.Weights {
                target.weight_values=values
                keys:=len(times)*stride
                if len(values)%keys!=0 || len(values)/keys==0 || len(values)/keys>4096 { return .Invalid_Animation }
                target.weight_count=u32(len(values)/keys)
                mesh:=model.nodes[target.node].mesh
                if mesh<0 { return .Invalid_Animation }
                for primitive in model.primitives { if primitive.mesh==u32(mesh) && len(primitive.morphs)!=int(target.weight_count) { return .Invalid_Animation } }
            } else {
                defer delete(values)
                if len(values)!=len(times)*stride*int(components) { return .Invalid_Animation }
                target.values=make([][4]f32,len(times)*stride)
                for &value,k in target.values { for c in 0..<int(components) { value[c]=values[k*int(components)+c] } }
            }
        }
    }
    if !animation_model_valid(&model.animation) { return .Invalid_Animation }
    return .None
}
@(private="package")
gltf_compose_world :: proc(nodes:[]Gltf_Node,locals:[]km.Mat4,allocator:=context.allocator)->([]km.Mat4,editor.Scene_Error) {
    if len(locals)!=0 && len(locals)!=len(nodes) { return nil,.Invalid_Operation }
    matrices:=make([]km.Mat4,len(nodes),allocator); visited:=make([]bool,len(nodes),allocator); defer delete(visited,allocator)
    chain:=make([dynamic]int,allocator); defer delete(chain)
    for i in 0..<len(nodes) {
        if visited[i] { continue }; clear(&chain); cursor:=i
        for cursor>=0 && !visited[cursor] {
            if len(chain)>=min(1024,len(nodes)) { delete(matrices,allocator); return nil,.Invalid_Operation }
            append(&chain,cursor); cursor=int(nodes[cursor].parent)
            if cursor< -1 || cursor>=len(nodes) { delete(matrices,allocator); return nil,.Invalid_Operation }
        }
        parent:=km.identity(km.Mat4); if cursor>=0 { parent=matrices[cursor] }
        for j:=len(chain)-1;j>=0;j-=1 {
            index:=chain[j]; local:=nodes[index].local_matrix; if len(locals)>0 { local=locals[index] }
            matrices[index]=km.matrix_mul(parent,local); parent=matrices[index]; visited[index]=true
        }
    }
    return matrices,.None
}
/// Samples authored TRS channels while retaining every untouched source matrix exactly.
gltf_world_matrices :: proc(model:^Gltf_Model,player:^Animation_Player,allocator:=context.allocator)->([]km.Mat4,editor.Scene_Error) {
    if model==nil || len(model.nodes)!=len(model.animation.bind_pose) || len(model.nodes)!=len(model.animation.parents) { return nil,.Invalid_Operation }
    for node,i in model.nodes { if node.parent!=model.animation.parents[i] { return nil,.Invalid_Operation } }
    pose,error:=animation_sample_pose(&model.animation,player,allocator)
    if error!=.None { return nil,error }; defer delete(pose,allocator)
    modified:=make([]bool,len(model.nodes),allocator); defer delete(modified,allocator)
    if player!=nil {
        names:=[2]string{player.clip,""}; if player.blending { names[1]=player.target_clip }
        for name in names {
            if name=="" { continue }; clip:=animation_clip(&model.animation,name); if clip==nil { return nil,.Invalid_Operation }
            for channel in clip.channels {
                if channel.path==.Weights { continue }
                if model.nodes[channel.node].matrix_authored { return nil,.Invalid_Operation }
                modified[channel.node]=true
            }
        }
    }
    locals:=make([]km.Mat4,len(model.nodes),allocator); defer delete(locals,allocator)
    for node,i in model.nodes { locals[i]=node.local_matrix; if modified[i] { locals[i]=km.transform_to_mat4(pose[i]) } }
    return gltf_compose_world(model.nodes,locals,allocator)
}
/// Joint matrices are mesh-local: inverse(mesh world) * joint world * inverse bind.
gltf_skin_matrices :: proc(model:^Gltf_Model,node_index:u32,world:[]km.Mat4,allocator:=context.allocator)->([]km.Mat4,Gltf_Error) {
    if int(node_index)>=len(model.nodes) || len(world)!=len(model.nodes) { return nil,.Invalid_Skin }
    node:=model.nodes[node_index]
    if node.skin<0 || int(node.skin)>=len(model.skins) { return nil,.Invalid_Skin }
    inverse_mesh,invertible:=km.inverse(world[node_index]); if !invertible { return nil,.Invalid_Skin }
    skin:=model.skins[node.skin]
    matrices:=make([]km.Mat4,len(skin.joints),allocator)
    for joint,i in skin.joints { if int(joint)>=len(world) { delete(matrices,allocator); return nil,.Invalid_Skin }; matrices[i]=km.matrix_mul(inverse_mesh,km.matrix_mul(world[joint],skin.inverse_bind[i])) }
    return matrices,.None
}
/// Deforms a genuine indexed primitive using sampled morph and skin data for ordinary rendering consumers.
gltf_deform_geometry :: proc(model:^Gltf_Model,primitive_index,node_index:u32,world:[]km.Mat4,morph_weights:[]f32=nil,allocator:=context.allocator)->(Mesh_Geometry,Gltf_Error) {
    if int(primitive_index)>=len(model.primitives) || int(node_index)>=len(model.nodes) { return {},.Invalid_Geometry }
    source:=&model.primitives[primitive_index]; node:=model.nodes[node_index]
    if node.mesh<0 || source.mesh!=u32(node.mesh) || (len(morph_weights)!=0 && len(morph_weights)!=len(source.morphs)) { return {},.Invalid_Geometry }
    geometry:=mesh_geometry_clone(&source.geometry,allocator)
    success:=false; defer { if !success { mesh_geometry_destroy(&geometry) } }
    for weight,i in morph_weights {
        if math.is_nan(weight) || math.is_inf(weight) { return {},.Invalid_Geometry }
        morph:=source.morphs[i]
        for &vertex,j in geometry.vertices {
            if len(morph.position)>0 { vertex.position+=morph.position[j]*weight }
            if len(morph.normal)>0 { vertex.normal+=morph.normal[j]*weight }
            if len(morph.tangent)>0 { tangent:=km.xyz(vertex.tangent)+morph.tangent[j]*weight; vertex.tangent={tangent[0],tangent[1],tangent[2],vertex.tangent[3]} }
        }
    }
    if node.skin>=0 {
        matrices,error:=gltf_skin_matrices(model,node_index,world,allocator); if error!=.None { return {},error }; defer delete(matrices,allocator)
        normals:=make([]km.Mat3,len(matrices),allocator); defer delete(normals,allocator)
        for matrix_value,i in matrices { inverse,invertible:=km.inverse(km.mat4_to_mat3(matrix_value)); if !invertible { return {},.Invalid_Skin }; normals[i]=km.transpose(inverse) }
        if len(source.joints)!=len(geometry.vertices) || len(source.weights)!=len(geometry.vertices) { return {},.Invalid_Skin }
        for &vertex,i in geometry.vertices {
            position,normal,tangent:km.Vec3
            for weight,j in source.weights[i] {
                if weight==0 { continue }; joint:=source.joints[i][j]; if int(joint)>=len(matrices) { return {},.Invalid_Skin }
                position+=km.xyz(km.matrix_vector(matrices[joint],km.Vec4{vertex.position[0],vertex.position[1],vertex.position[2],1}))*weight
                normal+=km.matrix_vector(normals[joint],vertex.normal)*weight
                tangent+=km.matrix_vector(km.mat4_to_mat3(matrices[joint]),km.xyz(vertex.tangent))*weight
            }
            vertex.position=position; vertex.normal=normal; vertex.tangent={tangent[0],tangent[1],tangent[2],vertex.tangent[3]}
        }
    }
    if len(geometry.vertices)==0 { return {},.Invalid_Geometry }
    low,high:=geometry.vertices[0].position,geometry.vertices[0].position
    for &vertex in geometry.vertices {
        if !mesh_vec_finite(vertex.position) || km.length_squared(vertex.normal)<0.000000000001 { return {},.Invalid_Geometry }
        vertex.normal=km.normalize(vertex.normal)
        tangent:=km.xyz(vertex.tangent)-vertex.normal*km.dot(vertex.normal,km.xyz(vertex.tangent))
        if km.length_squared(tangent)<0.000000000001 { vertex.tangent=mesh_tangent(vertex.normal) } else { tangent=km.normalize(tangent); vertex.tangent={tangent[0],tangent[1],tangent[2],vertex.tangent[3]} }
        for axis in 0..<3 { low[axis]=min(low[axis],vertex.position[axis]); high[axis]=max(high[axis],vertex.position[axis]) }
    }
    geometry.bounds=km.aabb_from_min_max(low,high); success=true; return geometry,.None
}
