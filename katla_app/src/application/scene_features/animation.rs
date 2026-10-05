//! Scene-owned animation data and explicit pose evaluation commands.

use katla_ecs::World;
use katla_gfx::animation::{
    AnimChannelInfo, AnimClipHeader, AnimationBufferUploader, AnimationUpload, JointInfo,
    SkeletonAnimParams,
};
use katla_gfx::render_graph::{
    BufferAccess, BufferByteRange, BufferDesc, BufferMemoryPolicy, BufferUsage, BufferUsages,
    ComputeBinding, ComputeCommand, ComputeDispatch, ComputeDispatchSize, ComputePipelineDesc,
    PassDesc, PassId, PassType, ResourceAccessMode, ResourceAccessStage, ResourceId,
};
use katla_gfx::renderer::frame_scope::FrameToken;
use katla_gfx::{BufferHandle, GpuRenderer, RendererError, SkeletonHandle};

use crate::components::DrawableComponent;
use crate::resources::ResourceManager;
use crate::systems::gpu_animation_system::GpuAnimationSystem;
use crate::{AppError, AppResult, FrameGraph, Renderer};

const BUFFER_NAMES: [&str; 8] = [
    "scene_animation_params",
    "scene_animation_clips",
    "scene_animation_channels",
    "scene_animation_times",
    "scene_animation_values",
    "scene_animation_joints",
    "scene_animation_world",
    "scene_animation_output",
];
const POSE_PASS: &str = "animation_pose_eval";
const COPY_PASS: &str = "animation_skeleton_copy";

#[derive(Default)]
struct AnimationData {
    bytes: [Vec<u8>; 6],
    params: Vec<SkeletonAnimParams>,
    max_skeletons: usize,
    max_joints: usize,
    revision: u64,
}

impl AnimationBufferUploader for AnimationData {
    fn upload_static_data(&mut self, upload: AnimationUpload<'_>) -> Result<(), RendererError> {
        let revision = self.revision.checked_add(1).ok_or_else(|| {
            RendererError::InvalidOperation("Animation data revision exhausted".into())
        })?;
        self.bytes = [
            Vec::new(),
            bytemuck::cast_slice(upload.headers).to_vec(),
            bytemuck::cast_slice(upload.channels).to_vec(),
            bytemuck::cast_slice(upload.times).to_vec(),
            bytemuck::cast_slice(upload.values).to_vec(),
            bytemuck::cast_slice(upload.joints).to_vec(),
        ];
        self.max_skeletons = upload.max_skeletons;
        self.max_joints = upload.max_joints;
        self.revision = revision;
        Ok(())
    }

    fn update_params(&mut self, params: &[SkeletonAnimParams]) {
        self.params.clear();
        self.params.extend_from_slice(params);
    }
}

impl AnimationData {
    fn clear_static(&mut self) -> AppResult<()> {
        self.params.clear();
        if self.max_skeletons == 0 && self.max_joints == 0 && self.bytes.iter().all(Vec::is_empty) {
            return Ok(());
        }
        self.revision =
            self.revision
                .checked_add(1)
                .ok_or_else(|| AppError::RendererInitFailed {
                    reason: "Animation data revision exhausted".into(),
                })?;
        self.bytes = std::array::from_fn(|_| Vec::new());
        self.max_skeletons = 0;
        self.max_joints = 0;
        Ok(())
    }

    fn sizes(&self) -> AppResult<[u64; 8]> {
        let bytes = |count: usize, stride: usize| {
            count
                .max(1)
                .checked_mul(stride)
                .map(|size| size as u64)
                .ok_or_else(|| AppError::RendererInitFailed {
                    reason: "Animation buffer size overflow".into(),
                })
        };
        Ok([
            bytes(
                self.max_skeletons,
                std::mem::size_of::<SkeletonAnimParams>(),
            )?,
            self.bytes[1]
                .len()
                .max(std::mem::size_of::<AnimClipHeader>()) as u64,
            self.bytes[2]
                .len()
                .max(std::mem::size_of::<AnimChannelInfo>()) as u64,
            self.bytes[3].len().max(std::mem::size_of::<f32>()) as u64,
            self.bytes[4].len().max(std::mem::size_of::<f32>()) as u64,
            self.bytes[5].len().max(std::mem::size_of::<JointInfo>()) as u64,
            bytes(self.max_joints, 64)?,
            bytes(self.max_joints, 64)?,
        ])
    }
}

struct AnimationSlot {
    buffers: [BufferHandle; 8],
    sizes: [u64; 8],
    uploaded_revision: Option<u64>,
}

/// CPU preparation and GPU allocations belonging to one installed scene feature.
pub(crate) struct AnimationFeatures {
    shader: ComputePipelineDesc,
    data: AnimationData,
    slots: Vec<AnimationSlot>,
    resources: Option<[ResourceId; 8]>,
    imported_sizes: [u64; 8],
    skeleton_resources: Vec<(SkeletonHandle, ResourceId)>,
    next_skeleton_resource: u64,
    skinning_accesses: Vec<BufferAccess>,
}

impl AnimationFeatures {
    pub(crate) fn new(renderer: &mut Renderer, resources: &ResourceManager) -> AppResult<Self> {
        let shader = ComputePipelineDesc {
            wgsl: std::fs::read_to_string(
                resources.shader_path("compute/animation/pose_eval.wgsl"),
            )?,
            entry: "cs_main".into(),
        };
        shader
            .interface()
            .map_err(|reason| AppError::RendererInitFailed { reason })?;
        let data = AnimationData::default();
        let sizes = data.sizes()?;
        let mut slots: Vec<AnimationSlot> = Vec::with_capacity(renderer.frame_slot_count());
        for _ in 0..renderer.frame_slot_count() {
            match allocate_slot(renderer, sizes) {
                Ok(slot) => slots.push(slot),
                Err(error) => {
                    for slot in slots {
                        for buffer in slot.buffers {
                            let _ = renderer.destroy_buffer(buffer);
                        }
                    }
                    return Err(error);
                }
            }
        }
        if slots.is_empty() {
            return Err(AppError::RendererInitFailed {
                reason: "Renderer has no reusable frame slots".into(),
            });
        }
        Ok(Self {
            shader,
            data,
            slots,
            resources: None,
            imported_sizes: sizes,
            skeleton_resources: Vec::new(),
            next_skeleton_resource: 0,
            skinning_accesses: Vec::new(),
        })
    }

    pub(crate) fn install_graph(&mut self, graph: &mut FrameGraph) -> AppResult<()> {
        if self.resources.is_some() {
            return Err(AppError::RendererInitFailed {
                reason: "Animation feature is already installed".into(),
            });
        }
        let slot = &self.slots[0];
        let mut resources = [ResourceId(0); 8];
        for index in 0..8 {
            resources[index] = graph
                .import_buffer(
                    BUFFER_NAMES[index],
                    slot.buffers[index],
                    buffer_desc(index, slot.sizes[index]),
                )
                .map_err(graph_error)?;
        }
        let dispatch = self.dispatch(resources, 0);
        let accesses = dispatch
            .accesses()
            .map_err(|reason| AppError::RendererInitFailed { reason })?;
        graph
            .insert_pass(
                0,
                PassDesc::new(COPY_PASS, PassType::Transfer, vec![], vec![]),
            )
            .map_err(graph_error)?;
        graph
            .insert_pass(
                0,
                PassDesc::new(POSE_PASS, PassType::Compute, vec![], vec![])
                    .with_buffer_accesses(accesses)
                    .with_commands([ComputeCommand::Dispatch(dispatch)]),
            )
            .map_err(graph_error)?;
        self.resources = Some(resources);
        Ok(())
    }

    pub(crate) fn warm_pipeline(
        &self,
        renderer: &mut Renderer,
        graph: &FrameGraph,
    ) -> AppResult<()> {
        graph
            .prepare_compute_pipeline(renderer, &self.shader)
            .map_err(graph_error)
    }

    pub(crate) fn prepare_frame(
        &mut self,
        renderer: &mut Renderer,
        graph: &mut FrameGraph,
        frame: &FrameToken,
        world: &mut World,
        cpu: &mut GpuAnimationSystem,
    ) -> AppResult<()> {
        let resources = self.resources.ok_or_else(|| AppError::RendererInitFailed {
            reason: "Animation graph resources are not installed".into(),
        })?;
        cpu.prepare(world, &mut self.data)?;
        if cpu.skeleton_count() == 0 {
            self.data.clear_static()?;
        }
        self.data.params.clear();
        cpu.update_params(world, &mut self.data);
        let sizes = self.data.sizes()?;
        let slot =
            self.slots
                .get_mut(frame.slot())
                .ok_or_else(|| AppError::RendererInitFailed {
                    reason: "Animation frame slot is outside renderer ownership".into(),
                })?;
        if slot.sizes != sizes {
            let replacement = allocate_slot(renderer, sizes)?;
            let previous = std::mem::replace(slot, replacement);
            for buffer in previous.buffers {
                renderer.destroy_buffer(buffer)?;
            }
        }
        for index in 0..8 {
            if self.imported_sizes[index] != sizes[index] {
                graph
                    .redefine_imported_buffer(
                        resources[index],
                        slot.buffers[index],
                        buffer_desc(index, sizes[index]),
                    )
                    .map_err(graph_error)?;
                self.imported_sizes[index] = sizes[index];
            } else {
                graph
                    .rebind_imported_buffer(resources[index], slot.buffers[index])
                    .map_err(graph_error)?;
            }
        }
        if slot.uploaded_revision != Some(self.data.revision) {
            for (index, size) in sizes.iter().enumerate().take(6).skip(1) {
                let data = padded_bytes(&self.data.bytes[index], *size)?;
                renderer.write_buffer(frame, slot.buffers[index], 0, &data)?;
            }
            slot.uploaded_revision = Some(self.data.revision);
        }
        let params = padded_bytes(bytemuck::cast_slice(&self.data.params), sizes[0])?;
        renderer.write_buffer(frame, slot.buffers[0], 0, &params)?;
        let count =
            u32::try_from(cpu.skeleton_count()).map_err(|_| AppError::RendererInitFailed {
                reason: "Animation skeleton count exceeds dispatch capacity".into(),
            })?;
        let dispatch = self.dispatch(resources, count.div_ceil(64));
        let accesses = dispatch
            .accesses()
            .map_err(|reason| AppError::RendererInitFailed { reason })?;
        graph
            .set_pass_commands(
                pass_id(graph, POSE_PASS)?,
                vec![ComputeCommand::Dispatch(dispatch)],
                accesses,
            )
            .map_err(graph_error)?;
        let mut commands = Vec::new();
        let mut accesses = Vec::new();
        self.skinning_accesses.clear();
        for entity in cpu.entities() {
            let Some(drawable) = world.get_component::<DrawableComponent>(entity) else {
                continue;
            };
            let Some(info) = cpu.entity_info(entity) else {
                continue;
            };
            if drawable.skeleton_handle.is_none() || info.joint_count == 0 {
                continue;
            }
            let skeleton = drawable.skeleton_handle;
            let target = renderer.skeleton_buffer_handle(frame, skeleton)?;
            let size = u64::from(info.joint_count) * 64;
            let desc =
                renderer
                    .buffer_descriptor(target)
                    .ok_or_else(|| AppError::RendererInitFailed {
                        reason: "Skeleton buffer has no live allocation descriptor".into(),
                    })?;
            if desc.size < size
                || !desc
                    .usages
                    .contains(BufferUsages::STORAGE | BufferUsages::TRANSFER_DESTINATION)
            {
                return Err(AppError::RendererInitFailed {
                    reason: "Skeleton storage does not support the declared pose copy".into(),
                });
            }
            let resource = if let Some((_, resource)) = self
                .skeleton_resources
                .iter()
                .find(|(handle, _)| *handle == skeleton)
            {
                graph
                    .redefine_imported_buffer(*resource, target, desc)
                    .map_err(graph_error)?;
                *resource
            } else {
                let next = self.next_skeleton_resource.checked_add(1).ok_or_else(|| {
                    AppError::RendererInitFailed {
                        reason: "Animation graph resource identity exhausted".into(),
                    }
                })?;
                let resource = graph
                    .import_buffer(
                        format!("scene_animation_skeleton_{}", self.next_skeleton_resource),
                        target,
                        desc,
                    )
                    .map_err(graph_error)?;
                self.next_skeleton_resource = next;
                self.skeleton_resources.push((skeleton, resource));
                resource
            };
            let source_offset = u64::from(info.joint_offset) * 64;
            if source_offset
                .checked_add(size)
                .is_none_or(|end| end > sizes[7])
            {
                return Err(AppError::RendererInitFailed {
                    reason: "Animation skeleton copy exceeds pose output".into(),
                });
            }
            commands.push(ComputeCommand::CopyBuffer {
                source: resources[7],
                destination: resource,
                source_offset,
                destination_offset: 0,
                size,
            });
            accesses.push(BufferAccess::new(
                resources[7],
                ResourceAccessMode::Read,
                BufferUsage::TransferSource,
                ResourceAccessStage::Transfer,
                BufferByteRange::new(source_offset, size),
            ));
            accesses.push(BufferAccess::new(
                resource,
                ResourceAccessMode::Write,
                BufferUsage::TransferDestination,
                ResourceAccessStage::Transfer,
                BufferByteRange::new(0, size),
            ));
            self.skinning_accesses.push(BufferAccess::new(
                resource,
                ResourceAccessMode::Read,
                BufferUsage::Storage,
                ResourceAccessStage::VertexShader,
                BufferByteRange::new(0, size),
            ));
        }
        graph
            .set_pass_commands(pass_id(graph, COPY_PASS)?, commands, accesses)
            .map_err(graph_error)?;
        Ok(())
    }

    pub(crate) fn skinning_accesses(&self) -> &[BufferAccess] {
        &self.skinning_accesses
    }

    /// Retire imports after the application replaces the frame's graphics packets.
    pub(crate) fn retire_unused_imports(&mut self, graph: &mut FrameGraph) -> AppResult<()> {
        let mut index = 0;
        while index < self.skeleton_resources.len() {
            let resource = self.skeleton_resources[index].1;
            if self
                .skinning_accesses
                .iter()
                .any(|access| access.resource == resource)
            {
                index += 1;
            } else {
                graph
                    .remove_imported_buffer(resource)
                    .map_err(graph_error)?;
                self.skeleton_resources.remove(index);
            }
        }
        Ok(())
    }

    fn dispatch(&self, resources: [ResourceId; 8], workgroups: u32) -> ComputeDispatch {
        ComputeDispatch {
            pipeline: self.shader.clone(),
            bindings: resources
                .into_iter()
                .enumerate()
                .map(|(binding, resource)| ComputeBinding {
                    group: 0,
                    binding: binding as u32,
                    resource,
                    range: BufferByteRange::WHOLE,
                })
                .collect(),
            constants: Vec::new(),
            size: ComputeDispatchSize::Direct([workgroups, 1, 1]),
        }
    }
}

fn allocate_slot(renderer: &mut Renderer, sizes: [u64; 8]) -> AppResult<AnimationSlot> {
    let mut buffers = [BufferHandle::NONE; 8];
    for index in 0..8 {
        match renderer.create_buffer(buffer_desc(index, sizes[index])) {
            Ok(buffer) => buffers[index] = buffer,
            Err(error) => {
                for buffer in buffers.into_iter().filter(|buffer| buffer.is_some()) {
                    let _ = renderer.destroy_buffer(buffer);
                }
                return Err(error.into());
            }
        }
    }
    Ok(AnimationSlot {
        buffers,
        sizes,
        uploaded_revision: None,
    })
}

fn buffer_desc(index: usize, size: u64) -> BufferDesc {
    BufferDesc::new(
        size,
        BufferUsages::STORAGE | BufferUsages::TRANSFER_SOURCE | BufferUsages::TRANSFER_DESTINATION,
        if index < 6 {
            BufferMemoryPolicy::CpuVisible
        } else {
            BufferMemoryPolicy::DeviceLocal
        },
    )
}

fn padded_bytes(bytes: &[u8], capacity: u64) -> AppResult<Vec<u8>> {
    let capacity = usize::try_from(capacity).map_err(|_| AppError::RendererInitFailed {
        reason: "Animation upload capacity exceeds host address space".into(),
    })?;
    if bytes.len() > capacity {
        return Err(AppError::RendererInitFailed {
            reason: "Animation upload exceeds its slot allocation".into(),
        });
    }
    let mut result = vec![0; capacity];
    result[..bytes.len()].copy_from_slice(bytes);
    Ok(result)
}

fn pass_id(graph: &FrameGraph, name: &str) -> AppResult<PassId> {
    graph
        .pass_id(name)
        .ok_or_else(|| AppError::RendererInitFailed {
            reason: format!("Animation pass '{name}' is missing"),
        })
}

fn graph_error(error: katla_gfx::render_graph::RenderGraphError) -> AppError {
    AppError::RendererInitFailed {
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_animation_static_upload_owns_data_and_preserves_runtime_array_minimums() {
        let mut data = AnimationData::default();
        let headers = [AnimClipHeader {
            duration: 1.0,
            channel_offset: 0,
            channel_count: 0,
            _pad: 0,
        }];
        data.upload_static_data(AnimationUpload {
            headers: &headers,
            channels: &[],
            times: &[],
            values: &[],
            joints: &[],
            max_skeletons: 3,
            max_joints: 7,
        })
        .unwrap();
        assert_eq!(data.bytes[1], bytemuck::cast_slice::<_, u8>(&headers));
        assert_eq!(data.revision, 1);
        assert_eq!(data.sizes().unwrap(), [96, 16, 32, 4, 4, 128, 448, 448]);
    }

    #[test]
    fn test_animation_replaces_cpu_params_and_zeros_unused_slot_bytes() {
        let mut data = AnimationData::default();
        let params = SkeletonAnimParams {
            clip_index: 2,
            target_clip_index: 0,
            current_time: 0.5,
            target_time: 0.0,
            blend_weight: 0.0,
            joint_offset: 5,
            joint_count: 3,
            flags: 1,
        };
        data.update_params(&[params, params]);
        data.update_params(&[params]);
        let bytes = padded_bytes(bytemuck::cast_slice(&data.params), 64).unwrap();
        assert_eq!(&bytes[..32], bytemuck::bytes_of(&params));
        assert_eq!(&bytes[32..], &[0; 32]);
        assert!(padded_bytes(&[0; 65], 64).is_err());
    }

    #[test]
    fn test_animation_cpu_allocations_match_canonical_shader_interface() {
        let shader = ComputePipelineDesc {
            wgsl: include_str!("../../../../resources/shaders/compute/animation/pose_eval.wgsl")
                .into(),
            entry: "cs_main".into(),
        };
        let interface = shader.interface().unwrap();
        let sizes = AnimationData::default().sizes().unwrap();
        assert_eq!(interface.workgroup_size, [64, 1, 1]);
        assert_eq!(interface.bindings.len(), 8);
        for (index, binding) in interface.bindings.iter().enumerate() {
            assert_eq!((binding.group, binding.binding), (0, index as u32));
            assert_eq!(binding.usage, BufferUsage::Storage);
            assert_eq!(binding.minimum_buffer_bytes, sizes[index]);
            assert_eq!(
                binding.mode,
                if index < 6 {
                    ResourceAccessMode::Read
                } else {
                    ResourceAccessMode::ReadWrite
                }
            );
        }
    }

    #[test]
    fn test_animation_empty_scene_retires_static_generation_without_repeated_revision_changes() {
        let mut data = AnimationData::default();
        data.upload_static_data(AnimationUpload {
            headers: &[],
            channels: &[],
            times: &[1.0],
            values: &[2.0],
            joints: &[],
            max_skeletons: 4,
            max_joints: 12,
        })
        .unwrap();
        data.clear_static().unwrap();
        assert_eq!(data.revision, 2);
        assert_eq!(data.max_skeletons, 0);
        assert_eq!(data.max_joints, 0);
        assert!(data.bytes.iter().all(Vec::is_empty));
        assert_eq!(data.sizes().unwrap(), [32, 16, 32, 4, 4, 128, 64, 64]);
        data.clear_static().unwrap();
        assert_eq!(data.revision, 2);
    }
}
