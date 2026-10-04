//! Local-pose sampling and skin matrices share the engine's column-major convention.
package app

import km "../math"
import editor "../editor"
import "core:slice"

@(private="package")
animation_sample_channel :: proc(channel:Animation_Channel,time:f32)->[4]f32 {
    stride:=3 if channel.interpolation==.Cubic_Spline else 1
    offset:=1 if stride==3 else 0
    if time<=channel.times[0] { return channel.values[offset] }
    last:=len(channel.times)-1; if time>=channel.times[last] { return channel.values[last*stride+offset] }
    upper:=1; for channel.times[upper]<=time { upper+=1 }; lower:=upper-1
    a,b:=channel.values[lower*stride+offset],channel.values[upper*stride+offset]
    if channel.interpolation==.Step { return a }
    duration:=channel.times[upper]-channel.times[lower]; t:=(time-channel.times[lower])/duration
    if channel.interpolation==.Cubic_Spline {
        t2,t3:=t*t,t*t*t; tangent_a:=channel.values[lower*3+2]; tangent_b:=channel.values[upper*3]
        result:=a*(2*t3-3*t2+1)+tangent_a*(duration*(t3-2*t2+t))+b*(-2*t3+3*t2)+tangent_b*(duration*(t3-t2))
        if channel.path==.Rotation { return cast([4]f32)(km.quat_normalize(km.Quat(result))) }; return result
    }
    if channel.path==.Rotation { return cast([4]f32)(km.quat_slerp(km.Quat(a),km.Quat(b),t)) }; return a*(1-t)+b*t
}
@(private="package")
animation_sample_clip :: proc(clip:^Animation_Clip,time:f32,pose:[]km.Transform) {
    for channel in clip.channels {
        if channel.path==.Weights { continue }
        value:=animation_sample_channel(channel,time); local:=&pose[channel.node]
        switch channel.path {
        case .Translation: local.position={value[0],value[1],value[2]}
        case .Rotation: local.rotation=km.Quat(value)
        case .Scale: local.scale={value[0],value[1],value[2]}
        case .Weights:
        }
    }
}
/// Samples missing channels from bind pose and blends source/target local TRS once.
animation_sample_pose :: proc(model:^Animation_Model,player:^Animation_Player,allocator:=context.allocator)->([]km.Transform,editor.Scene_Error) {
    if !animation_model_valid(model) || !animation_sample_state_valid(player) { return nil,.Invalid_Operation }
    pose:=slice.clone(model.bind_pose,allocator)
    if player==nil { return pose,.None }
    if player.clip!="" { source:=animation_clip(model,player.clip); if source==nil { delete(pose,allocator); return nil,.Invalid_Operation }; animation_sample_clip(source,player.time,pose) }
    if player.blending {
        target:=animation_clip(model,player.target_clip); if target==nil { delete(pose,allocator); return nil,.Invalid_Operation }
        target_pose:=slice.clone(model.bind_pose,allocator); defer delete(target_pose,allocator); animation_sample_clip(target,player.target_time,target_pose)
        for &local,i in pose { local=km.transform_lerp(target_pose[i],local,player.blend_weight) }
    }
    for local in pose { if !animation_transform_valid(local) { delete(pose,allocator); return nil,.Invalid_Operation } }
    return pose,.None
}
/// Builds world joint matrices for arbitrary acyclic parent order, then multiplies inverse binds.
animation_skin_matrices :: proc(model:^Animation_Model,pose:[]km.Transform,allocator:=context.allocator)->([]km.Mat4,editor.Scene_Error) {
    if !animation_model_valid(model) || len(pose)!=len(model.bind_pose) { return nil,.Invalid_Operation }
    context.allocator=allocator; matrices:=make([]km.Mat4,len(pose),allocator)
    for local,i in pose {
        if !animation_transform_valid(local) { delete(matrices,allocator); return nil,.Invalid_Operation }
        joint_matrix:=km.transform_to_mat4(local); parent:=model.parents[i]
        for parent>=0 { joint_matrix=km.matrix_mul(km.transform_to_mat4(pose[parent]),joint_matrix); parent=model.parents[parent] }
        if len(model.inverse_bind)>0 { joint_matrix=km.matrix_mul(joint_matrix,model.inverse_bind[i]) }; matrices[i]=joint_matrix
    }
    return matrices,.None
}
@(private="package")
animation_weight_channel :: proc(clip:^Animation_Clip,node:u32)->^Animation_Channel { if clip!=nil { for &channel in clip.channels { if channel.path==.Weights && channel.node==node { return &channel } } }; return nil }
@(private="package")
animation_sample_weight_channel :: proc(channel:^Animation_Channel,time:f32,weights:[]f32) {
    count:=int(channel.weight_count); stride:=3 if channel.interpolation==.Cubic_Spline else 1; offset:=count if stride==3 else 0
    last:=len(channel.times)-1
    if time<=channel.times[0] { copy(weights,channel.weight_values[offset:offset+count]); return }
    if time>=channel.times[last] { start:=last*stride*count+offset; copy(weights,channel.weight_values[start:start+count]); return }
    upper:=1; for channel.times[upper]<=time { upper+=1 }; lower:=upper-1
    left:=lower*stride*count+offset; right:=upper*stride*count+offset
    if channel.interpolation==.Step { copy(weights,channel.weight_values[left:left+count]); return }
    duration:=channel.times[upper]-channel.times[lower]; t:=(time-channel.times[lower])/duration
    for &weight,i in weights {
        a,b:=channel.weight_values[left+i],channel.weight_values[right+i]
        if channel.interpolation==.Cubic_Spline {
            t2,t3:=t*t,t*t*t; tangent_a:=channel.weight_values[lower*3*count+2*count+i]; tangent_b:=channel.weight_values[upper*3*count+i]
            weight=a*(2*t3-3*t2+1)+tangent_a*(duration*(t3-2*t2+t))+b*(-2*t3+3*t2)+tangent_b*(duration*(t3-t2))
        } else { weight=a*(1-t)+b*t }
    }
}
/// Samples every glTF morph weight; missing channels crossfade from the mesh's authored bind weights.
animation_sample_weights :: proc(model:^Animation_Model,player:^Animation_Player,node:u32,bind_weights:[]f32,allocator:=context.allocator)->([]f32,editor.Scene_Error) {
    if !animation_model_valid(model) || !animation_sample_state_valid(player) || int(node)>=len(model.bind_pose) { return nil,.Invalid_Operation }
    for weight in bind_weights { if !finite_nonnegative(abs(weight)) { return nil,.Invalid_Field_Value } }
    weights:=slice.clone(bind_weights,allocator)
    if player==nil { return weights,.None }
    source:=animation_clip(model,player.clip); if source==nil && player.clip!="" { delete(weights,allocator); return nil,.Invalid_Operation }
    source_channel:=animation_weight_channel(source,node)
    if source_channel!=nil { if len(weights)!=int(source_channel.weight_count) { delete(weights,allocator); return nil,.Invalid_Operation }; animation_sample_weight_channel(source_channel,player.time,weights) }
    if player.blending {
        target:=animation_clip(model,player.target_clip); if target==nil { delete(weights,allocator); return nil,.Invalid_Operation }
        target_channel:=animation_weight_channel(target,node); target_weights:=slice.clone(bind_weights,allocator); defer delete(target_weights,allocator)
        if target_channel!=nil { if len(target_weights)!=int(target_channel.weight_count) { delete(weights,allocator); return nil,.Invalid_Operation }; animation_sample_weight_channel(target_channel,player.target_time,target_weights) }
        for &weight,i in weights { weight=target_weights[i]*(1-player.blend_weight)+weight*player.blend_weight }
    }
    for weight in weights { if !finite_nonnegative(abs(weight)) { delete(weights,allocator); return nil,.Invalid_Operation } }
    return weights,.None
}
