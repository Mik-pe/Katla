const PATH_TRANSLATION: u32 = 0u;
const PATH_ROTATION: u32 = 1u;
const PATH_SCALE: u32 = 2u;
const INTERP_LINEAR: u32 = 0u;
const INTERP_STEP: u32 = 1u;
const NO_PARENT: u32 = 0xFFFFFFFFu;

const FLAG_BLENDING: u32 = 4u;

struct SkeletonAnimParams {
    clip_index: u32,
    target_clip_index: u32,
    current_time: f32,
    target_time: f32,
    blend_weight: f32,
    joint_offset: u32,
    joint_count: u32,
    flags: u32,
}

struct AnimClipHeader {
    duration: f32,
    channel_offset: u32,
    channel_count: u32,
    _pad: u32,
}

struct AnimChannelInfo {
    target_joint: u32,
    path_type: u32,
    time_offset: u32,
    value_offset: u32,
    keyframe_count: u32,
    interpolation: u32,
    _pad0: u32,
    _pad1: u32,
}

struct JointInfo {
    inverse_bind_matrix: mat4x4f,
    parent_index: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    rest_translation: vec3f,
    _pad3: u32,
    rest_rotation: vec4f,
    rest_scale: vec3f,
    _pad4: u32,
}

@group(0) @binding(0) var<storage, read> params: array<SkeletonAnimParams>;
@group(0) @binding(1) var<storage, read> clip_headers: array<AnimClipHeader>;
@group(0) @binding(2) var<storage, read> channel_infos: array<AnimChannelInfo>;
@group(0) @binding(3) var<storage, read> keyframe_times: array<f32>;
@group(0) @binding(4) var<storage, read> keyframe_values: array<f32>;
@group(0) @binding(5) var<storage, read> joints: array<JointInfo>;
@group(0) @binding(6) var<storage, read_write> world_matrices: array<mat4x4f>;
@group(0) @binding(7) var<storage, read_write> output_matrices: array<mat4x4f>;

// ---------------------------------------------------------------------------
// Math utilities
// ---------------------------------------------------------------------------

fn lerp_vec3(a: vec3f, b: vec3f, t: f32) -> vec3f {
    return a + (b - a) * t;
}

fn quat_normalize(q: vec4f) -> vec4f {
    let len = length(q);
    if (len < 1e-6) {
        return vec4f(0.0, 0.0, 0.0, 1.0);
    }
    return q / len;
}

fn slerp(a: vec4f, b: vec4f, t: f32) -> vec4f {
    var q_a = quat_normalize(a);
    var q_b = quat_normalize(b);

    let d = dot(q_a, q_b);

    if (d < 0.0) {
        q_b = -q_b;
    }

    let abs_dot = abs(d);

    if (abs_dot > 0.9995) {
        let result = lerp_vec3(q_a.xyz, q_b.xyz, t);
        return quat_normalize(vec4f(result, q_a.w + (q_b.w - q_a.w) * t));
    }

    let theta = acos(clamp(abs_dot, -1.0, 1.0));
    let sin_theta = sin(theta);
    let w_a = sin((1.0 - t) * theta) / sin_theta;
    let w_b = sin(t * theta) / sin_theta;

    return quat_normalize(w_a * q_a + w_b * q_b);
}

fn mat4_from_trs(translation: vec3f, rotation: vec4f, scale: vec3f) -> mat4x4f {
    let q = quat_normalize(rotation);
    let x = q.x;
    let y = q.y;
    let z = q.z;
    let w = q.w;

    let x2 = x + x;
    let y2 = y + y;
    let z2 = z + z;

    let xx = x * x2;
    let xy = x * y2;
    let xz = x * z2;
    let yy = y * y2;
    let yz = y * z2;
    let zz = z * z2;
    let wx = w * x2;
    let wy = w * y2;
    let wz = w * z2;

    return mat4x4f(
        vec4f((1.0 - (yy + zz)) * scale.x, (xy + wz) * scale.x, (xz - wy) * scale.x, 0.0),
        vec4f((xy - wz) * scale.y, (1.0 - (xx + zz)) * scale.y, (yz + wx) * scale.y, 0.0),
        vec4f((xz + wy) * scale.z, (yz - wx) * scale.z, (1.0 - (xx + yy)) * scale.z, 0.0),
        vec4f(translation.x, translation.y, translation.z, 1.0),
    );
}

// ---------------------------------------------------------------------------
// Keyframe search
// ---------------------------------------------------------------------------

fn find_keyframe(offset: u32, count: u32, time: f32) -> u32 {
    if (count <= 1u) {
        return 0u;
    }

    if (time <= keyframe_times[offset]) {
        return 0u;
    }

    if (time >= keyframe_times[offset + count - 1u]) {
        return count - 1u;
    }

    var lo = 0u;
    var hi = count - 1u;

    while (lo < hi - 1u) {
        let mid = (lo + hi) >> 1u;
        if (keyframe_times[offset + mid] <= time) {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    return lo;
}

// ---------------------------------------------------------------------------
// Channel evaluation (single path type)
// ---------------------------------------------------------------------------

fn cubic_weights(alpha: f32, duration: f32) -> vec4f {
    let t2 = alpha * alpha;
    let t3 = t2 * alpha;
    return vec4f(
        2.0 * t3 - 3.0 * t2 + 1.0,
        (t3 - 2.0 * t2 + alpha) * duration,
        -2.0 * t3 + 3.0 * t2,
        (t3 - t2) * duration,
    );
}

// glTF stores each cubic keyframe as [in-tangent, value, out-tangent].
fn cubic_component(base0: u32, base1: u32, width: u32, component: u32, weights: vec4f) -> f32 {
    return dot(weights, vec4f(
        keyframe_values[base0 + width + component],
        keyframe_values[base0 + 2u * width + component],
        keyframe_values[base1 + width + component],
        keyframe_values[base1 + component],
    ));
}

fn evaluate_channel_vec3(channel: AnimChannelInfo, time: f32) -> vec3f {
    if (channel.keyframe_count == 0u) {
        return vec3f(0.0);
    }

    let k0 = find_keyframe(channel.time_offset, channel.keyframe_count, time);
    let k1 = min(k0 + 1u, channel.keyframe_count - 1u);

    let t0 = keyframe_times[channel.time_offset + k0];
    let t1 = keyframe_times[channel.time_offset + k1];

    let dur = t1 - t0;
    let alpha = clamp(select(0.0, (time - t0) / dur, dur > 1e-6), 0.0, 1.0);
    let vo = channel.value_offset;

    if (channel.interpolation == INTERP_LINEAR) {
        let a = vec3f(
            keyframe_values[vo + k0 * 3u],
            keyframe_values[vo + k0 * 3u + 1u],
            keyframe_values[vo + k0 * 3u + 2u],
        );
        let b = vec3f(
            keyframe_values[vo + k1 * 3u],
            keyframe_values[vo + k1 * 3u + 1u],
            keyframe_values[vo + k1 * 3u + 2u],
        );
        return lerp_vec3(a, b, alpha);
    } else if (channel.interpolation == INTERP_STEP) {
        return vec3f(
            keyframe_values[vo + k0 * 3u],
            keyframe_values[vo + k0 * 3u + 1u],
            keyframe_values[vo + k0 * 3u + 2u],
        );
    } else {
        let base0 = vo + k0 * 9u;
        let base1 = vo + k1 * 9u;
        let weights = cubic_weights(alpha, dur);
        return vec3f(
            cubic_component(base0, base1, 3u, 0u, weights),
            cubic_component(base0, base1, 3u, 1u, weights),
            cubic_component(base0, base1, 3u, 2u, weights),
        );
    }
}

fn evaluate_channel_quat(channel: AnimChannelInfo, time: f32) -> vec4f {
    if (channel.keyframe_count == 0u) {
        return vec4f(0.0, 0.0, 0.0, 1.0);
    }

    let k0 = find_keyframe(channel.time_offset, channel.keyframe_count, time);
    let k1 = min(k0 + 1u, channel.keyframe_count - 1u);

    let t0 = keyframe_times[channel.time_offset + k0];
    let t1 = keyframe_times[channel.time_offset + k1];

    let dur = t1 - t0;
    let alpha = clamp(select(0.0, (time - t0) / dur, dur > 1e-6), 0.0, 1.0);
    let vo = channel.value_offset;

    if (channel.interpolation == INTERP_LINEAR) {
        let a = vec4f(
            keyframe_values[vo + k0 * 4u],
            keyframe_values[vo + k0 * 4u + 1u],
            keyframe_values[vo + k0 * 4u + 2u],
            keyframe_values[vo + k0 * 4u + 3u],
        );
        let b = vec4f(
            keyframe_values[vo + k1 * 4u],
            keyframe_values[vo + k1 * 4u + 1u],
            keyframe_values[vo + k1 * 4u + 2u],
            keyframe_values[vo + k1 * 4u + 3u],
        );
        return slerp(a, b, alpha);
    } else if (channel.interpolation == INTERP_STEP) {
        return vec4f(
            keyframe_values[vo + k0 * 4u],
            keyframe_values[vo + k0 * 4u + 1u],
            keyframe_values[vo + k0 * 4u + 2u],
            keyframe_values[vo + k0 * 4u + 3u],
        );
    } else {
        let base0 = vo + k0 * 12u;
        let base1 = vo + k1 * 12u;
        let weights = cubic_weights(alpha, dur);
        return quat_normalize(vec4f(
            cubic_component(base0, base1, 4u, 0u, weights),
            cubic_component(base0, base1, 4u, 1u, weights),
            cubic_component(base0, base1, 4u, 2u, weights),
            cubic_component(base0, base1, 4u, 3u, weights),
        ));
    }
}

struct JointPose {
    translation: vec3f,
    rotation: vec4f,
    scale: vec3f,
}

fn evaluate_joint(clip_idx: u32, time: f32, joint_offset: u32, joint_index: u32) -> JointPose {
    let joint = joints[joint_offset + joint_index];
    var pose = JointPose(joint.rest_translation, joint.rest_rotation, joint.rest_scale);
    if (clip_idx == 0xffffffffu) {
        return pose;
    }
    let clip = clip_headers[clip_idx];
    let eval_time = clamp(time, 0.0, max(clip.duration, 0.0));

    for (var c = 0u; c < clip.channel_count; c = c + 1u) {
        let channel = channel_infos[clip.channel_offset + c];
        if (channel.target_joint != joint_index) {
            continue;
        }
        if (channel.path_type == PATH_TRANSLATION) {
            pose.translation = evaluate_channel_vec3(channel, eval_time);
        }
        if (channel.path_type == PATH_ROTATION) {
            pose.rotation = evaluate_channel_quat(channel, eval_time);
        }
        if (channel.path_type == PATH_SCALE) {
            pose.scale = evaluate_channel_vec3(channel, eval_time);
        }
    }
    return pose;
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) global_id: vec3u) {
    let gid = global_id.x;
    if (gid >= arrayLength(&params)) {
        return;
    }
    let skeleton = params[gid];
    let do_blend = (skeleton.flags & FLAG_BLENDING) != 0u;
    if (skeleton.clip_index != 0xffffffffu && skeleton.clip_index >= arrayLength(&clip_headers)) {
        return;
    }
    if (do_blend && skeleton.target_clip_index != 0xffffffffu && skeleton.target_clip_index >= arrayLength(&clip_headers)) {
        return;
    }

    let joint_offset = skeleton.joint_offset;
    let toward_target = 1.0 - clamp(skeleton.blend_weight, 0.0, 1.0);
    for (var j = 0u; j < skeleton.joint_count; j = j + 1u) {
        var pose = evaluate_joint(skeleton.clip_index, skeleton.current_time, joint_offset, j);
        if (do_blend) {
            let target_pose = evaluate_joint(skeleton.target_clip_index, skeleton.target_time, joint_offset, j);
            pose.translation = lerp_vec3(pose.translation, target_pose.translation, toward_target);
            pose.rotation = slerp(pose.rotation, target_pose.rotation, toward_target);
            pose.scale = lerp_vec3(pose.scale, target_pose.scale, toward_target);
        }
        let joint = joints[joint_offset + j];
        let local_mat = mat4_from_trs(pose.translation, pose.rotation, pose.scale);
        var world_mat = local_mat;
        if (joint.parent_index != NO_PARENT && joint.parent_index < j) {
            world_mat = world_matrices[joint_offset + joint.parent_index] * local_mat;
        }
        world_matrices[joint_offset + j] = world_mat;
        output_matrices[joint_offset + j] = world_mat * joint.inverse_bind_matrix;
    }
}
