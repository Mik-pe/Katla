//! Metal-native GPU animation compute system.
//!
//! Manages the compute pipeline for skeletal animation pose evaluation
//! and the data buffers required for GPU-driven animation.
//!
//! Buffer layout (binding index → data):
//!
//! | Binding | Name              | Direction | Description                     |
//! |---------|-------------------|-----------|---------------------------------|
//! | 0       | params            | CPU→GPU   | per-frame `SkeletonAnimParams`  |
//! | 1       | clip_headers      | GPU-only  | `AnimClipHeader` array          |
//! | 2       | channel_infos     | GPU-only  | `AnimChannelInfo` array         |
//! | 3       | keyframe_times    | GPU-only  | f32 keyframe timestamps         |
//! | 4       | keyframe_values   | GPU-only  | f32 keyframe values             |
//! | 5       | joints            | GPU-only  | `JointInfo` array               |
//! | 6       | world_matrices    | GPU       | scratch space for world transforms |
//! | 7       | output_matrices   | GPU       | final joint matrices            |

use crate::error::RendererError;

use super::context::MetalContext;

#[cfg(test)]
use log::info;

use crate::animation::{AnimChannelInfo, AnimClipHeader, JointInfo, SkeletonAnimParams};
use crate::backend::resource::GpuBuffer;

use super::buffer::MetalBuffer;

/// GPU buffers for the pose compute dispatch.
pub(crate) struct AnimationBuffers {
    params: Option<MetalBuffer>,
    clip_headers: Option<MetalBuffer>,
    channel_infos: Option<MetalBuffer>,
    keyframe_times: Option<MetalBuffer>,
    keyframe_values: Option<MetalBuffer>,
    joints: Option<MetalBuffer>,
    world_matrices: Option<MetalBuffer>,
    output_matrices: Option<MetalBuffer>,
}

impl AnimationBuffers {
    fn new() -> Self {
        Self {
            params: None,
            clip_headers: None,
            channel_infos: None,
            keyframe_times: None,
            keyframe_values: None,
            joints: None,
            world_matrices: None,
            output_matrices: None,
        }
    }

    #[cfg(test)]
    fn allocate_params(
        &mut self,
        context: &MetalContext,
        max_skeletons: usize,
    ) -> Result<(), RendererError> {
        let size = (max_skeletons * std::mem::size_of::<SkeletonAnimParams>()) as u64;
        if size == 0 {
            return Ok(());
        }
        self.params = Some(context.create_buffer(size, true)?);
        Ok(())
    }

    #[cfg(test)]
    fn allocate_clip_data(
        &mut self,
        context: &MetalContext,
        headers: &[AnimClipHeader],
        channels: &[AnimChannelInfo],
        times: &[f32],
        values: &[f32],
    ) -> Result<(), RendererError> {
        if !headers.is_empty() {
            let size = std::mem::size_of_val(headers) as u64;
            self.clip_headers = Some(context.create_buffer(size, true)?);
        }
        if !channels.is_empty() {
            let size = std::mem::size_of_val(channels) as u64;
            self.channel_infos = Some(context.create_buffer(size, true)?);
        }
        if !times.is_empty() {
            let size = std::mem::size_of_val(times) as u64;
            self.keyframe_times = Some(context.create_buffer(size, true)?);
        }
        if !values.is_empty() {
            let size = std::mem::size_of_val(values) as u64;
            self.keyframe_values = Some(context.create_buffer(size, true)?);
        }
        Ok(())
    }

    #[cfg(test)]
    fn allocate_joints(
        &mut self,
        context: &MetalContext,
        max_joints: usize,
    ) -> Result<(), RendererError> {
        if max_joints == 0 {
            return Ok(());
        }
        let size = (max_joints * std::mem::size_of::<JointInfo>()) as u64;
        self.joints = Some(context.create_buffer(size, true)?);
        Ok(())
    }

    #[cfg(test)]
    fn allocate_world(
        &mut self,
        context: &MetalContext,
        max_joints: usize,
    ) -> Result<(), RendererError> {
        if max_joints == 0 {
            return Ok(());
        }
        let size = (max_joints * 64) as u64;
        self.world_matrices = Some(context.create_buffer(size, true)?);
        Ok(())
    }

    #[cfg(test)]
    fn allocate_output(
        &mut self,
        context: &MetalContext,
        max_joints: usize,
    ) -> Result<(), RendererError> {
        if max_joints == 0 {
            return Ok(());
        }
        let size = (max_joints * 64) as u64;
        self.output_matrices = Some(context.create_buffer(size, true)?);
        Ok(())
    }

    fn update_params(&self, params: &[SkeletonAnimParams]) {
        let Some(ref buf) = self.params else { return };
        let ptr = buf.map();
        let byte_len = std::mem::size_of_val(params);
        unsafe {
            std::ptr::copy_nonoverlapping(params.as_ptr() as *const u8, ptr, byte_len);
        }
        buf.unmap();
    }

    fn upload_clip_data(
        &self,
        headers: &[AnimClipHeader],
        channels: &[AnimChannelInfo],
        times: &[f32],
        values: &[f32],
    ) {
        upload_slice_to_buffer(&self.clip_headers, headers);
        upload_slice_to_buffer(&self.channel_infos, channels);
        upload_slice_to_buffer(&self.keyframe_times, times);
        upload_slice_to_buffer(&self.keyframe_values, values);
    }

    fn upload_joints(&self, joints: &[JointInfo]) {
        upload_slice_to_buffer(&self.joints, joints);
    }

    #[cfg(test)]
    fn upload_world_matrices(&self, matrices: &[[f32; 16]]) {
        upload_slice_to_buffer(&self.world_matrices, matrices);
    }

    #[cfg(test)]
    pub fn read_output(&self) -> Vec<[f32; 16]> {
        let Some(ref buf) = self.output_matrices else {
            return Vec::new();
        };
        let size = buf.size() as usize;
        let count = size / 64;
        if count == 0 {
            return Vec::new();
        }
        let ptr = buf.map();
        let result = unsafe { std::slice::from_raw_parts(ptr as *const [f32; 16], count) }.to_vec();
        buf.unmap();
        result
    }
}

/// Upload a typed slice into a CPU-accessible Metal buffer.
fn upload_slice_to_buffer<T: bytemuck::Pod>(buffer: &Option<MetalBuffer>, data: &[T]) {
    let Some(buf) = buffer else { return };
    if data.is_empty() {
        return;
    }
    let byte_len = std::mem::size_of_val(data);
    let ptr = buf.map();
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr() as *const u8, ptr, byte_len);
    }
    buf.unmap();
}

/// Per-entity skeleton tracking data.
#[cfg(test)]
struct SkeletonEntry {
    joint_offset: u32,
    joint_count: u32,
}

/// Metal-native GPU animation system.
///
/// Manages the compute pipeline and data buffers for skeletal animation
/// pose evaluation on the GPU.
pub struct MetalAnimationSystem {
    active_slot: usize,
    device: Option<objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLDevice>>>,
    buffers: [AnimationBuffers; super::metal_renderer::FRAMES_IN_FLIGHT],
    #[cfg(test)]
    skeleton_entries: Vec<SkeletonEntry>,
    skeleton_count: usize,
    total_joints: usize,
}

impl MetalAnimationSystem {
    pub(crate) fn new() -> Self {
        Self {
            active_slot: 0,
            device: None,
            buffers: std::array::from_fn(|_| AnimationBuffers::new()),
            #[cfg(test)]
            skeleton_entries: Vec::new(),
            skeleton_count: 0,
            total_joints: 0,
        }
    }

    pub(crate) fn initialize(&mut self, context: &MetalContext) {
        self.device = Some(context.device.clone());
    }

    /// Number of active skeletons.
    #[cfg(test)]
    pub fn skeleton_count(&self) -> usize {
        self.skeleton_count
    }

    /// Get the skeleton copy commands (handle, joint_offset, joint_count).
    #[cfg(test)]
    pub fn skeleton_copy_commands(&self) -> Vec<(crate::handle::SkeletonHandle, u32, u32)> {
        self.skeleton_entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                (
                    crate::handle::SkeletonHandle::from_raw(i as u32, 0),
                    entry.joint_offset,
                    entry.joint_count,
                )
            })
            .collect()
    }

    /// Prepare animation data for a set of entities.
    #[cfg(test)]
    pub fn prepare(
        &mut self,
        context: &MetalContext,
        entities: &[(u32, u32)],
        clip_headers: &[AnimClipHeader],
        channel_infos: &[AnimChannelInfo],
        keyframe_times: &[f32],
        keyframe_values: &[f32],
        joint_infos: &[JointInfo],
    ) -> Result<(), RendererError> {
        let num_skeletons = entities.len();
        if num_skeletons == 0 {
            self.skeleton_count = 0;
            self.total_joints = 0;
            self.skeleton_entries.clear();
            return Ok(());
        }

        let mut total_joints = 0usize;
        let mut entries = Vec::with_capacity(num_skeletons);
        for &(joint_count, _) in entities {
            entries.push(SkeletonEntry {
                joint_offset: total_joints as u32,
                joint_count,
            });
            total_joints += joint_count as usize;
        }

        self.skeleton_entries = entries;
        self.skeleton_count = num_skeletons;
        self.total_joints = total_joints;

        for buffers in &mut self.buffers {
            buffers.allocate_params(context, num_skeletons)?;
            buffers.allocate_clip_data(
                context,
                clip_headers,
                channel_infos,
                keyframe_times,
                keyframe_values,
            )?;
            buffers.allocate_joints(context, total_joints)?;
            buffers.allocate_world(context, total_joints)?;
            buffers.allocate_output(context, total_joints)?;
            buffers.upload_clip_data(clip_headers, channel_infos, keyframe_times, keyframe_values);
            buffers.upload_joints(joint_infos);
        }

        info!(
            "Prepared Metal animation: {} skeletons, {} joints",
            num_skeletons, total_joints
        );

        Ok(())
    }

    /// Stage pose parameters for the active graph frame slot.
    pub fn update_params(&mut self, params: &[SkeletonAnimParams]) {
        self.buffers[self.active_slot].update_params(params);
    }

    pub(crate) fn select_slot(&mut self, slot: usize) {
        self.active_slot = slot;
    }

    pub(crate) fn builtin_buffer(
        &self,
        role: crate::render_graph::BuiltinBuffer,
    ) -> Option<&MetalBuffer> {
        use crate::render_graph::BuiltinBuffer::*;
        let buffers = &self.buffers[self.active_slot];
        match role {
            AnimationParams => buffers.params.as_ref(),
            AnimationClips => buffers.clip_headers.as_ref(),
            AnimationChannels => buffers.channel_infos.as_ref(),
            AnimationTimes => buffers.keyframe_times.as_ref(),
            AnimationValues => buffers.keyframe_values.as_ref(),
            AnimationJoints => buffers.joints.as_ref(),
            AnimationWorld => buffers.world_matrices.as_ref(),
            AnimationOutput => buffers.output_matrices.as_ref(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::SkeletonHandle;

    fn create_context() -> MetalContext {
        MetalContext::init_headless().expect("Failed to create headless context")
    }

    #[test]
    fn test_animation_system_creation() {
        let system = MetalAnimationSystem::new();
        assert_eq!(system.skeleton_count(), 0);
    }

    #[test]
    fn test_animation_buffers_allocate_params() {
        let ctx = create_context();
        let mut buffers = AnimationBuffers::new();

        let result = buffers.allocate_params(&ctx, 4);
        assert!(result.is_ok(), "allocate_params failed: {:?}", result.err());
        assert!(buffers.params.is_some());

        let buf = buffers.params.as_ref().unwrap();
        assert_eq!(
            buf.size(),
            (4 * std::mem::size_of::<SkeletonAnimParams>()) as u64
        );
    }

    #[test]
    fn test_animation_buffers_upload_params() {
        let ctx = create_context();
        let mut buffers = AnimationBuffers::new();
        buffers.allocate_params(&ctx, 2).unwrap();

        let params = vec![
            SkeletonAnimParams {
                clip_index: 0,
                target_clip_index: 0,
                current_time: 1.0,
                target_time: 0.0,
                blend_weight: 0.0,
                joint_offset: 0,
                joint_count: 4,
                flags: 0,
            },
            SkeletonAnimParams {
                clip_index: 1,
                target_clip_index: 0,
                current_time: 0.5,
                target_time: 0.0,
                blend_weight: 0.5,
                joint_offset: 4,
                joint_count: 4,
                flags: 0,
            },
        ];

        buffers.update_params(&params);

        let buf = buffers.params.as_ref().unwrap();
        let ptr = buf.map() as *const SkeletonAnimParams;
        let read = unsafe { std::slice::from_raw_parts(ptr, 2) };
        assert_eq!(read[0].clip_index, 0);
        assert_eq!(read[0].current_time, 1.0);
        assert_eq!(read[1].clip_index, 1);
        assert_eq!(read[1].blend_weight, 0.5);
        buf.unmap();
    }

    #[test]
    fn test_animation_buffers_allocate_clip_data() {
        let ctx = create_context();
        let mut buffers = AnimationBuffers::new();

        let headers = vec![AnimClipHeader {
            duration: 1.0,
            channel_offset: 0,
            channel_count: 3,
            _pad: 0,
        }];
        let channels = vec![AnimChannelInfo {
            target_joint: 0,
            path_type: 0,
            time_offset: 0,
            value_offset: 0,
            keyframe_count: 2,
            interpolation: 0,
            _pad: [0; 2],
        }];
        let times = vec![0.0f32, 1.0];
        let values = vec![0.0f32, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0];

        let result = buffers.allocate_clip_data(&ctx, &headers, &channels, &times, &values);
        assert!(
            result.is_ok(),
            "allocate_clip_data failed: {:?}",
            result.err()
        );

        buffers.upload_clip_data(&headers, &channels, &times, &values);

        assert!(buffers.clip_headers.is_some());
        assert!(buffers.channel_infos.is_some());
        assert!(buffers.keyframe_times.is_some());
        assert!(buffers.keyframe_values.is_some());
    }

    #[test]
    fn test_animation_buffers_allocate_joints() {
        let ctx = create_context();
        let mut buffers = AnimationBuffers::new();

        let joints = vec![JointInfo {
            inverse_bind_matrix: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            parent_index: 0xFFFFFFFF,
            _pad: [0; 3],
            rest_translation: [0.0; 3],
            _pad2: 0,
            rest_rotation: [0.0, 0.0, 0.0, 1.0],
            rest_scale: [1.0, 1.0, 1.0],
            _pad3: 0,
        }];

        let result = buffers.allocate_joints(&ctx, 1);
        assert!(result.is_ok(), "allocate_joints failed: {:?}", result.err());

        buffers.upload_joints(&joints);

        let buf = buffers.joints.as_ref().unwrap();
        let ptr = buf.map() as *const JointInfo;
        let read = unsafe { &*ptr };
        assert_eq!(read.parent_index, 0xFFFFFFFF);
        assert_eq!(read.rest_rotation[3], 1.0);
        buf.unmap();
    }

    #[test]
    fn test_animation_buffers_allocate_world_and_output() {
        let ctx = create_context();
        let mut buffers = AnimationBuffers::new();

        let result = buffers.allocate_world(&ctx, 4);
        assert!(result.is_ok(), "allocate_world failed: {:?}", result.err());
        assert!(buffers.world_matrices.is_some());
        assert_eq!(buffers.world_matrices.as_ref().unwrap().size(), 256); // 4 * 64

        let result = buffers.allocate_output(&ctx, 4);
        assert!(result.is_ok(), "allocate_output failed: {:?}", result.err());
        assert!(buffers.output_matrices.is_some());
        assert_eq!(buffers.output_matrices.as_ref().unwrap().size(), 256);
    }

    #[test]
    fn test_animation_buffers_empty_allocations() {
        let ctx = create_context();
        let mut buffers = AnimationBuffers::new();

        // Zero-size allocations should be no-ops
        assert!(buffers.allocate_params(&ctx, 0).is_ok());
        assert!(buffers.allocate_joints(&ctx, 0).is_ok());
        assert!(buffers.allocate_world(&ctx, 0).is_ok());
        assert!(buffers.allocate_output(&ctx, 0).is_ok());
        assert!(buffers.allocate_clip_data(&ctx, &[], &[], &[], &[]).is_ok());

        assert!(buffers.params.is_none());
        assert!(buffers.joints.is_none());
        assert!(buffers.world_matrices.is_none());
        assert!(buffers.output_matrices.is_none());
    }

    #[test]
    fn test_animation_buffers_read_output() {
        let ctx = create_context();
        let mut buffers = AnimationBuffers::new();
        buffers.allocate_output(&ctx, 2).unwrap();

        // Write test data
        let identity: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let test_data = vec![identity, identity];
        buffers.upload_world_matrices(&test_data);

        // Read from output (separate buffer, so this reads zeros since we didn't dispatch)
        let output = buffers.read_output();
        assert_eq!(output.len(), 2);
    }

    #[test]
    fn test_animation_system_skeleton_copy_commands() {
        let ctx = create_context();
        let mut system = MetalAnimationSystem::new();

        let entities = vec![(4u32, 1u32), (6u32, 1u32)];
        let result = system.prepare(
            &ctx,
            &entities,
            &[AnimClipHeader {
                duration: 1.0,
                channel_offset: 0,
                channel_count: 1,
                _pad: 0,
            }],
            &[AnimChannelInfo {
                target_joint: 0,
                path_type: 0,
                time_offset: 0,
                value_offset: 0,
                keyframe_count: 1,
                interpolation: 0,
                _pad: [0; 2],
            }],
            &[0.0f32],
            &[0.0f32, 0.0, 0.0],
            &[JointInfo {
                inverse_bind_matrix: [
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                ],
                parent_index: 0xFFFFFFFF,
                _pad: [0; 3],
                rest_translation: [0.0; 3],
                _pad2: 0,
                rest_rotation: [0.0, 0.0, 0.0, 1.0],
                rest_scale: [1.0, 1.0, 1.0],
                _pad3: 0,
            }],
        );

        assert!(result.is_ok(), "prepare failed: {:?}", result.err());
        assert_eq!(system.skeleton_count(), 2);

        let commands = system.skeleton_copy_commands();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0], (SkeletonHandle::from_raw(0, 0), 0, 4));
        assert_eq!(commands[1], (SkeletonHandle::from_raw(1, 0), 4, 6));
    }
}

impl crate::AnimationBufferUploader for MetalAnimationSystem {
    fn upload_static_data(
        &mut self,
        upload: crate::animation::AnimationUpload<'_>,
    ) -> Result<(), RendererError> {
        use objc2_metal::MTLDevice;
        let device = self.device.as_ref().ok_or_else(|| {
            RendererError::InitializationFailed("Animation device not initialized".into())
        })?;
        let allocate = |size: usize| -> Result<MetalBuffer, RendererError> {
            let size = size.max(16);
            let native = device
                .newBufferWithLength_options(
                    size,
                    objc2_metal::MTLResourceOptions::StorageModeShared,
                )
                .ok_or_else(|| RendererError::AllocationFailed {
                    resource: "animation buffer".into(),
                    reason: "device refused allocation".into(),
                })?;
            Ok(MetalBuffer::new(native, size as u64))
        };
        for buffers in &mut self.buffers {
            buffers.params = Some(allocate(
                upload.max_skeletons.max(1) * std::mem::size_of::<SkeletonAnimParams>(),
            )?);
            buffers.clip_headers = Some(allocate(std::mem::size_of_val(upload.headers))?);
            buffers.channel_infos = Some(allocate(
                std::mem::size_of_val(upload.channels)
                    .max(std::mem::size_of::<crate::animation::AnimChannelInfo>()),
            )?);
            buffers.keyframe_times = Some(allocate(std::mem::size_of_val(upload.times))?);
            buffers.keyframe_values = Some(allocate(std::mem::size_of_val(upload.values))?);
            buffers.joints = Some(allocate(
                upload.max_joints.max(1) * std::mem::size_of::<JointInfo>(),
            )?);
            buffers.world_matrices = Some(allocate(upload.max_joints.max(1) * 64)?);
            buffers.output_matrices = Some(allocate(upload.max_joints.max(1) * 64)?);
            buffers.upload_clip_data(upload.headers, upload.channels, upload.times, upload.values);
            buffers.upload_joints(upload.joints);
        }
        self.skeleton_count = upload.max_skeletons;
        self.total_joints = upload.max_joints;
        Ok(())
    }
    fn update_params(&mut self, params: &[SkeletonAnimParams]) {
        MetalAnimationSystem::update_params(self, params);
    }
}
