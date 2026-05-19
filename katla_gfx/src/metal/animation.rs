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
use super::pipeline::MetalComputePipeline;
use super::shader;

#[cfg(test)]
use log::info;
#[cfg(test)]
use objc2_metal::MTLCommandBuffer;

#[cfg(test)]
use crate::animation::{AnimChannelInfo, AnimClipHeader, JointInfo, SkeletonAnimParams};
#[cfg(test)]
use crate::backend::command::{GpuCommandBuffer, GpuComputeEncoder};
#[cfg(test)]
use crate::backend::resource::GpuBuffer;

#[cfg(test)]
use super::buffer::MetalBuffer;

/// Workgroup size for pose compute shader (must match @workgroup_size in WGSL).
#[cfg(test)]
const POSE_COMPUTE_WORKGROUP_SIZE: u32 = 64;

/// GPU buffers for the pose compute dispatch.
#[cfg(test)]
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

#[cfg(test)]
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

    fn upload_world_matrices(&self, matrices: &[[f32; 16]]) {
        upload_slice_to_buffer(&self.world_matrices, matrices);
    }

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

    pub fn output_buffer(&self) -> Option<&MetalBuffer> {
        self.output_matrices.as_ref()
    }
}

/// Upload a typed slice into a CPU-accessible Metal buffer.
#[cfg(test)]
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
    pipeline: Option<MetalComputePipeline>,
    #[cfg(test)]
    buffers: AnimationBuffers,
    #[cfg(test)]
    skeleton_entries: Vec<SkeletonEntry>,
    #[cfg(test)]
    skeleton_count: usize,
    #[cfg(test)]
    total_joints: usize,
}

impl MetalAnimationSystem {
    pub(crate) fn new() -> Self {
        Self {
            pipeline: None,
            #[cfg(test)]
            buffers: AnimationBuffers::new(),
            #[cfg(test)]
            skeleton_entries: Vec::new(),
            #[cfg(test)]
            skeleton_count: 0,
            #[cfg(test)]
            total_joints: 0,
        }
    }

    /// Initialize the compute pipeline by compiling the given WGSL shader.
    pub(crate) fn init_pipeline_with_source(
        &mut self,
        context: &MetalContext,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = std::fs::read_to_string(shader_path).map_err(|e| {
            RendererError::InvalidOperation(format!(
                "Failed to read animation shader '{}': {}",
                shader_path.display(),
                e
            ))
        })?;

        let compiled = shader::compile_wgsl_to_metal(&context.device, &wgsl_source, &["cs_main"])?;

        let cs_fn = compiled.module.entry_points.get("cs_main").ok_or_else(|| {
            RendererError::InvalidOperation(
                "cs_main entry point not found in animation shader".into(),
            )
        })?;

        let pipeline = context.create_compute_pipeline(cs_fn, [64, 1, 1])?;
        self.pipeline = Some(pipeline);
        Ok(())
    }

    /// Number of active skeletons.
    #[cfg(test)]
    pub fn skeleton_count(&self) -> usize {
        self.skeleton_count
    }

    /// Get the skeleton copy commands (skeleton_handle_index, joint_offset, joint_count).
    #[cfg(test)]
    pub fn skeleton_copy_commands(&self) -> Vec<(u32, u32, u32)> {
        self.skeleton_entries
            .iter()
            .enumerate()
            .map(|(i, entry)| (i as u32, entry.joint_offset, entry.joint_count))
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

        self.buffers.allocate_params(context, num_skeletons)?;
        self.buffers.allocate_clip_data(
            context,
            clip_headers,
            channel_infos,
            keyframe_times,
            keyframe_values,
        )?;
        self.buffers.allocate_joints(context, total_joints)?;
        self.buffers.allocate_world(context, total_joints)?;
        self.buffers.allocate_output(context, total_joints)?;

        self.buffers
            .upload_clip_data(clip_headers, channel_infos, keyframe_times, keyframe_values);
        self.buffers.upload_joints(joint_infos);

        info!(
            "Prepared Metal animation: {} skeletons, {} joints",
            num_skeletons, total_joints
        );

        Ok(())
    }

    /// Update per-frame animation params and dispatch the compute pass.
    #[cfg(test)]
    pub fn dispatch(&mut self, context: &MetalContext, params: &[SkeletonAnimParams]) {
        let Some(ref pipeline) = self.pipeline else {
            return;
        };
        if params.is_empty() {
            return;
        }

        self.buffers.update_params(params);

        let mut cmd_buffer = context.create_command_buffer();
        cmd_buffer.begin();

        let mut encoder = cmd_buffer.begin_compute_pass();
        encoder.bind_compute_pipeline(pipeline);

        if let Some(ref buf) = self.buffers.params {
            encoder.bind_storage_buffer(buf, 0, 0);
        }
        if let Some(ref buf) = self.buffers.clip_headers {
            encoder.bind_storage_buffer(buf, 0, 1);
        }
        if let Some(ref buf) = self.buffers.channel_infos {
            encoder.bind_storage_buffer(buf, 0, 2);
        }
        if let Some(ref buf) = self.buffers.keyframe_times {
            encoder.bind_storage_buffer(buf, 0, 3);
        }
        if let Some(ref buf) = self.buffers.keyframe_values {
            encoder.bind_storage_buffer(buf, 0, 4);
        }
        if let Some(ref buf) = self.buffers.joints {
            encoder.bind_storage_buffer(buf, 0, 5);
        }
        if let Some(ref buf) = self.buffers.world_matrices {
            encoder.bind_storage_buffer(buf, 0, 6);
        }
        if let Some(ref buf) = self.buffers.output_matrices {
            encoder.bind_storage_buffer(buf, 0, 7);
        }

        let workgroups = params.len() as u32;
        let workgroup_count = workgroups.div_ceil(POSE_COMPUTE_WORKGROUP_SIZE);
        encoder.dispatch(workgroup_count, 1, 1);
        encoder.end_encoding();

        cmd_buffer.end();
        cmd_buffer.submit(context);
        cmd_buffer.inner.waitUntilCompleted();
    }

    /// Copy computed joint matrices from the output buffer to a skeleton buffer.
    #[cfg(test)]
    pub fn copy_to_skeleton(
        &self,
        _context: &MetalContext,
        skeleton_buffer: &MetalBuffer,
        joint_offset: u32,
        joint_count: u32,
    ) {
        let Some(ref output) = self.buffers.output_matrices else {
            return;
        };

        let src_offset = (joint_offset as u64) * 64;
        let size = (joint_count as u64) * 64;

        let src_ptr = output.map();
        let dst_ptr = skeleton_buffer.map();

        unsafe {
            std::ptr::copy_nonoverlapping(src_ptr.add(src_offset as usize), dst_ptr, size as usize);
        }

        output.unmap();
        skeleton_buffer.unmap();
    }

    /// Get the output buffer reference.
    #[cfg(test)]
    pub fn output_buffer(&self) -> Option<&MetalBuffer> {
        self.buffers.output_buffer()
    }

    /// Read back computed joint matrices from the output buffer.
    #[cfg(test)]
    pub fn read_output(&self) -> Vec<[f32; 16]> {
        self.buffers.read_output()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_context() -> MetalContext {
        MetalContext::init_headless().expect("Failed to create headless context")
    }

    #[test]
    fn test_animation_system_creation() {
        let system = MetalAnimationSystem::new();
        assert!(system.pipeline.is_none());
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
        assert_eq!(commands[0], (0, 0, 4));
        assert_eq!(commands[1], (1, 4, 6));
    }
}
