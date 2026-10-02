//! Backend-neutral animation buffer upload service.
use super::{AnimChannelInfo, AnimClipHeader, JointInfo, SkeletonAnimParams};
use crate::RendererError;

/// Static animation data and capacities prepared by the scene system.
pub struct AnimationUpload<'a> {
    pub headers: &'a [AnimClipHeader],
    pub channels: &'a [AnimChannelInfo],
    pub times: &'a [f32],
    pub values: &'a [f32],
    pub joints: &'a [JointInfo],
    pub max_skeletons: usize,
    pub max_joints: usize,
}

/// Scene-owned animation data upload, separate from graph dispatch recording.
pub trait AnimationBufferUploader {
    fn upload_static_data(&mut self, upload: AnimationUpload<'_>) -> Result<(), RendererError>;
    fn update_params(&mut self, params: &[SkeletonAnimParams]);
}
