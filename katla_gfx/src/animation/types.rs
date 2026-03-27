use bytemuck::{Pod, Zeroable};

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct SkeletonAnimParams {
    pub clip_index: u32,
    pub target_clip_index: u32,
    pub current_time: f32,
    pub target_time: f32,
    pub blend_weight: f32,
    pub joint_offset: u32,
    pub joint_count: u32,
    pub flags: u32,
}

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct AnimClipHeader {
    pub duration: f32,
    pub channel_offset: u32,
    pub channel_count: u32,
    pub _pad: u32,
}

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct AnimChannelInfo {
    pub target_joint: u32,
    pub path_type: u32,
    pub time_offset: u32,
    pub value_offset: u32,
    pub keyframe_count: u32,
    pub interpolation: u32,
    pub _pad: [u32; 2],
}

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct JointInfo {
    pub inverse_bind_matrix: [f32; 16],
    pub parent_index: u32,
    pub _pad: [u32; 3],
    pub rest_translation: [f32; 3],
    pub _pad2: u32,
    pub rest_rotation: [f32; 4],
    pub rest_scale: [f32; 3],
    pub _pad3: u32,
}
