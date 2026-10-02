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

impl AnimationBufferUploader for super::PoseComputeBuffers {
    fn upload_static_data(&mut self, upload: AnimationUpload<'_>) -> Result<(), RendererError> {
        self.wait_for_static_upload()?;
        self.allocate_params(upload.max_skeletons)?;
        self.allocate_clip_data(
            std::mem::size_of_val(upload.headers).max(std::mem::size_of::<AnimClipHeader>()) as u64,
            std::mem::size_of_val(upload.channels).max(std::mem::size_of::<AnimChannelInfo>())
                as u64,
            std::mem::size_of_val(upload.times).max(16) as u64,
            std::mem::size_of_val(upload.values).max(16) as u64,
        )?;
        self.allocate_joints(upload.max_joints)?;
        self.allocate_world(upload.max_joints)?;
        self.allocate_output(upload.max_joints)?;
        self.upload_clip_data(upload.headers, upload.channels, upload.times, upload.values);
        self.upload_joints(upload.joints);
        Ok(())
    }
    fn update_params(&mut self, params: &[SkeletonAnimParams]) {
        super::PoseComputeBuffers::update_params(self, params);
    }
}
