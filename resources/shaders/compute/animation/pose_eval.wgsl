const PATH_TRANSLATION: u32 = 0u;
const PATH_ROTATION: u32 = 1u;
const PATH_SCALE: u32 = 2u;
const INTERP_LINEAR: u32 = 0u;
const INTERP_STEP: u32 = 1u;
const INTERP_CUBIC_SPLINE: u32 = 2u;
const NO_PARENT: u32 = 0xFFFFFFFFu;

const FLAG_PLAYING: u32 = 1u;
const FLAG_LOOPING: u32 = 2u;
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
    if (count == 0u) {
        return 0u;
    }

    if (time <= keyframe_times[offset]) {
        return 0u;
    }

    if (time >= keyframe_times[offset + count - 1u]) {
        return count - 2u;
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

fn evaluate_channel_vec3(channel: AnimChannelInfo, time: f32) -> vec3f {
    if (channel.keyframe_count == 0u) {
        return vec3f(0.0);
    }

    let k0 = find_keyframe(channel.time_offset, channel.keyframe_count, time);
    let k1 = min(k0 + 1u, channel.keyframe_count - 1u);

    let t0 = keyframe_times[channel.time_offset + k0];
    let t1 = keyframe_times[channel.time_offset + k1];

    let dur = t1 - t0;
    let alpha = select(0.0, (time - t0) / dur, dur > 1e-6);
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
        // Cubic spline: glTF layout per keyframe = [value(N), tangent_out(N), tangent_in(N)]
        // For vec3 (N=3): 9 floats per keyframe
        // Offsets: 0..2 = value, 3..5 = tangent_out, 6..8 = tangent_in
        let comp = 3u;
        let bk0 = vo + k0 * 9u;
        let bk1 = vo + k1 * 9u;

        let dt = dur;
        let t2 = alpha * alpha;
        let t3 = t2 * alpha;

        var result = vec3f(0.0);
        let a_v = 2.0 * t3 - 3.0 * t2 + 1.0;
        let b_v = t3 - 2.0 * t2 + alpha;
        let c_v = -2.0 * t3 + 3.0 * t2;
        let d_v = t3 - t2;

        // X component
        let p0_x = keyframe_values[bk0 + 0u];
        let m0_x = keyframe_values[bk0 + comp + 0u] * dt;
        let p1_x = keyframe_values[bk1 + 0u];
        let m1_x = keyframe_values[bk1 + 2u * comp + 0u] * dt;
        result[0] = a_v * p0_x + b_v * m0_x + c_v * p1_x + d_v * m1_x;

        // Y component
        let p0_y = keyframe_values[bk0 + 1u];
        let m0_y = keyframe_values[bk0 + comp + 1u] * dt;
        let p1_y = keyframe_values[bk1 + 1u];
        let m1_y = keyframe_values[bk1 + 2u * comp + 1u] * dt;
        result[1] = a_v * p0_y + b_v * m0_y + c_v * p1_y + d_v * m1_y;

        // Z component
        let p0_z = keyframe_values[bk0 + 2u];
        let m0_z = keyframe_values[bk0 + comp + 2u] * dt;
        let p1_z = keyframe_values[bk1 + 2u];
        let m1_z = keyframe_values[bk1 + 2u * comp + 2u] * dt;
        result[2] = a_v * p0_z + b_v * m0_z + c_v * p1_z + d_v * m1_z;

        return result;
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
    let alpha = select(0.0, (time - t0) / dur, dur > 1e-6);
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
        // Cubic spline for quaternion: glTF layout [value(4), tangent_out(4), tangent_in(4)] = 12 values
        let comp = 4u;
        let bk0 = vo + k0 * 12u;
        let bk1 = vo + k1 * 12u;

        let dt = dur;
        let t2 = alpha * alpha;
        let t3 = t2 * alpha;

        let a_v = 2.0 * t3 - 3.0 * t2 + 1.0;
        let b_v = t3 - 2.0 * t2 + alpha;
        let c_v = -2.0 * t3 + 3.0 * t2;
        let d_v = t3 - t2;

        var result = vec4f(0.0);

        // Component 0
        let p0_0 = keyframe_values[bk0 + 0u];
        let m0_0 = keyframe_values[bk0 + comp + 0u] * dt;
        let p1_0 = keyframe_values[bk1 + 0u];
        let m1_0 = keyframe_values[bk1 + 2u * comp + 0u] * dt;
        result[0] = a_v * p0_0 + b_v * m0_0 + c_v * p1_0 + d_v * m1_0;

        // Component 1
        let p0_1 = keyframe_values[bk0 + 1u];
        let m0_1 = keyframe_values[bk0 + comp + 1u] * dt;
        let p1_1 = keyframe_values[bk1 + 1u];
        let m1_1 = keyframe_values[bk1 + 2u * comp + 1u] * dt;
        result[1] = a_v * p0_1 + b_v * m0_1 + c_v * p1_1 + d_v * m1_1;

        // Component 2
        let p0_2 = keyframe_values[bk0 + 2u];
        let m0_2 = keyframe_values[bk0 + comp + 2u] * dt;
        let p1_2 = keyframe_values[bk1 + 2u];
        let m1_2 = keyframe_values[bk1 + 2u * comp + 2u] * dt;
        result[2] = a_v * p0_2 + b_v * m0_2 + c_v * p1_2 + d_v * m1_2;

        // Component 3
        let p0_3 = keyframe_values[bk0 + 3u];
        let m0_3 = keyframe_values[bk0 + comp + 3u] * dt;
        let p1_3 = keyframe_values[bk1 + 3u];
        let m1_3 = keyframe_values[bk1 + 2u * comp + 3u] * dt;
        result[3] = a_v * p0_3 + b_v * m0_3 + c_v * p1_3 + d_v * m1_3;

        return quat_normalize(result);
    }
}

// ---------------------------------------------------------------------------
// Evaluate clip: write world transforms into world_matrices
// ---------------------------------------------------------------------------

fn evaluate_clip(
    clip_idx: u32,
    time: f32,
    joint_offset: u32,
    joint_count: u32,
) {
    let clip = clip_headers[clip_idx];
    let at_end = time >= clip.duration;
    let raw_time = select(time % clip.duration, time, clip.duration <= 0.0);
    let eval_time = select(raw_time, clip.duration - 1e-4, at_end);

    for (var j = 0u; j < joint_count; j = j + 1u) {
        var trans = joints[joint_offset + j].rest_translation;
        var rot = joints[joint_offset + j].rest_rotation;
        var sc = joints[joint_offset + j].rest_scale;

        for (var c = 0u; c < clip.channel_count; c = c + 1u) {
            let channel = channel_infos[clip.channel_offset + c];

            if (channel.target_joint != j) {
                continue;
            }

            if (channel.path_type == PATH_TRANSLATION) {
                trans = evaluate_channel_vec3(channel, eval_time);
            }
            if (channel.path_type == PATH_ROTATION) {
                rot = evaluate_channel_quat(channel, eval_time);
            }
            if (channel.path_type == PATH_SCALE) {
                sc = evaluate_channel_vec3(channel, eval_time);
            }
        }

        let local_mat = mat4_from_trs(trans, rot, sc);

        let parent_idx = joints[joint_offset + j].parent_index;

        var world_mat = local_mat;
        if (parent_idx != NO_PARENT && parent_idx < j) {
            world_mat = world_matrices[joint_offset + parent_idx] * local_mat;
        }

        world_matrices[joint_offset + j] = world_mat;
    }
}

// ---------------------------------------------------------------------------
// Apply inverse bind matrices and write to output
// ---------------------------------------------------------------------------

fn apply_ibm_and_output(joint_offset: u32, joint_count: u32) {
    for (var j = 0u; j < joint_count; j = j + 1u) {
        let ibm = joints[joint_offset + j].inverse_bind_matrix;
        output_matrices[joint_offset + j] = world_matrices[joint_offset + j] * ibm;
    }
}

// ---------------------------------------------------------------------------
// Blend two sets of world matrices via TRS decomposition + slerp
// ---------------------------------------------------------------------------

fn mat3_to_quat(m00: f32, m01: f32, m02: f32,
                 m10: f32, m11: f32, m12: f32,
                 m20: f32, m21: f32, m22: f32) -> vec4f {
    let trace = m00 + m11 + m22;

    if (trace > 0.0) {
        let s = 0.5 / sqrt(trace + 1.0);
        return quat_normalize(vec4f(
            (m12 - m21) * s,
            (m20 - m02) * s,
            (m01 - m10) * s,
            0.25 / s,
        ));
    } else if ((m00 > m11) && (m00 > m22)) {
        let s = 2.0 * sqrt(1.0 + m00 - m11 - m22);
        return quat_normalize(vec4f(
            0.25 * s,
            (m01 + m10) / s,
            (m02 + m20) / s,
            (m12 - m21) / s,
        ));
    } else if (m11 > m22) {
        let s = 2.0 * sqrt(1.0 + m11 - m00 - m22);
        return quat_normalize(vec4f(
            (m10 + m01) / s,
            0.25 * s,
            (m21 + m12) / s,
            (m20 - m02) / s,
        ));
    } else {
        let s = 2.0 * sqrt(1.0 + m22 - m00 - m11);
        return quat_normalize(vec4f(
            (m20 + m02) / s,
            (m21 + m12) / s,
            0.25 * s,
            (m01 - m10) / s,
        ));
    }
}

fn blend_world_matrices(
    joint_offset: u32,
    joint_count: u32,
    blend_weight: f32,
) {
    let w = blend_weight;

    for (var j = 0u; j < joint_count; j = j + 1u) {
        let mat_a = world_matrices[joint_offset + j];
        let mat_b = output_matrices[joint_offset + j];

        // Decompose A: translation from column 3, scale from column lengths
        let trans_a = vec3f(mat_a[3][0], mat_a[3][1], mat_a[3][2]);
        let sx_a = length(vec3f(mat_a[0][0], mat_a[0][1], mat_a[0][2]));
        let sy_a = length(vec3f(mat_a[1][0], mat_a[1][1], mat_a[1][2]));
        let sz_a = length(vec3f(mat_a[2][0], mat_a[2][1], mat_a[2][2]));

        // Decompose B
        let trans_b = vec3f(mat_b[3][0], mat_b[3][1], mat_b[3][2]);
        let sx_b = length(vec3f(mat_b[0][0], mat_b[0][1], mat_b[0][2]));
        let sy_b = length(vec3f(mat_b[1][0], mat_b[1][1], mat_b[1][2]));
        let sz_b = length(vec3f(mat_b[2][0], mat_b[2][1], mat_b[2][2]));

        // Blend translation and scale via lerp
        let blended_trans = lerp_vec3(trans_a, trans_b, w);
        let blended_scale = lerp_vec3(vec3f(sx_a, sy_a, sz_a), vec3f(sx_b, sy_b, sz_b), w);

        // Normalize the 3x3 to extract rotation, then convert to quaternion
        let rot_a = mat3_to_quat(
            mat_a[0][0] / sx_a, mat_a[0][1] / sx_a, mat_a[0][2] / sx_a,
            mat_a[1][0] / sy_a, mat_a[1][1] / sy_a, mat_a[1][2] / sy_a,
            mat_a[2][0] / sz_a, mat_a[2][1] / sz_a, mat_a[2][2] / sz_a,
        );
        let rot_b = mat3_to_quat(
            mat_b[0][0] / sx_b, mat_b[0][1] / sx_b, mat_b[0][2] / sx_b,
            mat_b[1][0] / sy_b, mat_b[1][1] / sy_b, mat_b[1][2] / sy_b,
            mat_b[2][0] / sz_b, mat_b[2][1] / sz_b, mat_b[2][2] / sz_b,
        );

        let blended_rot = slerp(rot_a, rot_b, w);

        world_matrices[joint_offset + j] = mat4_from_trs(blended_trans, blended_rot, blended_scale);
    }
}

// ---------------------------------------------------------------------------
// Main compute entry point
// ---------------------------------------------------------------------------

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) global_id: vec3u) {
    let gid = global_id.x;

    if (gid >= arrayLength(&params)) {
        return;
    }

    let skeleton = params[gid];

    // When not playing, still evaluate at the current time to freeze at the last pose
    // rather than snapping to identity.

    if (skeleton.clip_index >= arrayLength(&clip_headers)) {
        return;
    }

    let joint_offset = skeleton.joint_offset;
    let joint_count = skeleton.joint_count;
    let do_blend = (skeleton.flags & FLAG_BLENDING) != 0u;

    if (!do_blend) {
        evaluate_clip(skeleton.clip_index, skeleton.current_time, joint_offset, joint_count);
        apply_ibm_and_output(joint_offset, joint_count);
    } else {
        if (skeleton.target_clip_index >= arrayLength(&clip_headers)) {
            return;
        }

        evaluate_clip(skeleton.clip_index, skeleton.current_time, joint_offset, joint_count);

        for (var j = 0u; j < joint_count; j = j + 1u) {
            output_matrices[joint_offset + j] = world_matrices[joint_offset + j];
        }

        evaluate_clip(skeleton.target_clip_index, skeleton.target_time, joint_offset, joint_count);

        // CPU blend_weight goes 1.0 → 0.0 over the blend duration (1.0 = full source clip).
        // Invert so the shader weight represents "fraction toward target clip".
        blend_world_matrices(joint_offset, joint_count, 1.0 - skeleton.blend_weight);
        apply_ibm_and_output(joint_offset, joint_count);
    }
}
