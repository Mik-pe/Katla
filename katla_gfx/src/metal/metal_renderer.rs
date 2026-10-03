//! Metal backend implementation of the GpuRenderer trait.
//!
//! MetalRenderer wraps MetalContext and provides the same rendering API as
//! VulkanRenderer, allowing katla_app to be generic over the graphics backend.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::MTLTexture;

use crate::backend::command::GpuCommandBuffer;
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::handle::{
    BufferHandle, BufferMarker, MaterialHandle, MaterialMarker, MeshHandle, MeshMarker,
    ResourceStorage, SkeletonHandle, SkeletonMarker, TextureHandle, TextureMarker,
};

use crate::renderer::MAX_OBJECTS_PER_FRAME;
use crate::renderer::gpu_renderer::GpuRenderer;
use crate::renderer::pipeline_descriptor::PipelineDescriptor;
use crate::renderer::types::{DrawList, InstanceData};
use crate::size::Size2D;
use crate::texture::{ImageFormat, TextureDescriptor};

use super::argument_buffer::MetalBindlessTextureManager;
use super::buffer::MetalBuffer;
use super::context::MetalContext;
use super::texture::MetalTextureView;
use super::ui_renderer::MetalUIRenderer;

pub(crate) const OBJECT_UNIFORM_SIZE: u64 = 16 * 4 + 4 * 4 + 4 * 4 + 4 * 4;
pub(crate) const FRAMES_IN_FLIGHT: usize = 3;

/// Map the monotonically increasing frame index to the slot that owns mutable GPU data.
#[inline]
pub(crate) const fn frame_slot(frame_index: u32) -> usize {
    (frame_index as usize) % FRAMES_IN_FLIGHT
}

fn validate_object_buffer_capacity(
    draw_list: &DrawList,
    buffer_size: usize,
) -> Result<(), RendererError> {
    let Some(max_slot_end) = draw_list
        .draws
        .iter()
        .map(|draw| {
            draw.instance_index
                .saturating_add(draw.instance_count().max(1))
        })
        .max()
    else {
        return Ok(());
    };

    let required_size = (max_slot_end as usize)
        .checked_mul(OBJECT_UNIFORM_SIZE as usize)
        .ok_or_else(|| {
            RendererError::InvalidOperation(format!(
                "Object uniform size overflow for instance slot range ending at {max_slot_end}"
            ))
        })?;

    if required_size > buffer_size {
        let capacity = buffer_size / OBJECT_UNIFORM_SIZE as usize;
        return Err(RendererError::InvalidOperation(format!(
            "Draw list requires object slots up to {max_slot_end}, but the Metal object buffer only has {capacity} slots"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod object_buffer_capacity_tests {
    use super::*;
    use crate::renderer::types::{DrawCall, InstanceData};

    fn draw_list_with_counts(draws: &[(u32, u32)]) -> DrawList {
        let mut list = DrawList::new();
        for &(index, count) in draws {
            let mut draw = if count > 1 {
                DrawCall::instanced(
                    MeshHandle::NONE,
                    MaterialHandle::NONE,
                    std::iter::repeat_n(InstanceData::default(), count as usize).collect(),
                )
            } else {
                DrawCall::new(MeshHandle::NONE, MaterialHandle::NONE)
            };
            draw.instance_index = index;
            list.draws.push(draw);
        }
        list
    }

    fn draw_list(indices: &[u32]) -> DrawList {
        draw_list_with_counts(&indices.iter().map(|&i| (i, 1)).collect::<Vec<_>>())
    }

    #[test]
    fn empty_draw_list_needs_no_object_storage() {
        assert!(validate_object_buffer_capacity(&DrawList::new(), 0).is_ok());
    }

    #[test]
    fn highest_instance_index_must_fit_the_uploaded_buffer() {
        let two_slots = OBJECT_UNIFORM_SIZE as usize * 2;
        assert!(validate_object_buffer_capacity(&draw_list(&[0, 1]), two_slots).is_ok());

        let error = validate_object_buffer_capacity(&draw_list(&[0, 2]), two_slots)
            .expect_err("instance index 2 must not fit a two-slot object buffer");
        assert!(error.to_string().contains("up to 3"));
        assert!(error.to_string().contains("2 slots"));
    }

    #[test]
    fn instanced_range_must_fit_not_just_its_base_slot() {
        let three_slots = OBJECT_UNIFORM_SIZE as usize * 3;
        // Base slot 1 with 2 instances occupies slots 1 and 2 — fits exactly.
        assert!(
            validate_object_buffer_capacity(&draw_list_with_counts(&[(1, 2)]), three_slots).is_ok()
        );

        // Base slot 1 with 3 instances spills into slot 3 — must not pass on
        // a buffer that only covers slots 0..=2.
        let error = validate_object_buffer_capacity(&draw_list_with_counts(&[(1, 3)]), three_slots)
            .expect_err("an instanced range past the buffer end must be rejected");
        assert!(error.to_string().contains("up to 4"));
        assert!(error.to_string().contains("3 slots"));
    }
}

/// A mesh stored in Metal GPU buffers.
///
/// Creation validates bytes against the typed [`MeshDescriptor`](crate::renderer::registry::MeshDescriptor);
/// Metal encodes `TriangleList` only, so the descriptor itself is not stored.
/// `vertex_count`/`vertex_stride` are recorded so dynamic updates validate
/// exactly like Vulkan's; interleaved storage keeps the whole blob in
/// `vertex_buffer`.
pub(crate) struct MetalMesh {
    pub(crate) vertex_buffer: MetalBuffer,
    pub(crate) index_buffer: MetalBuffer,
    pub(crate) index_count: u32,
    pub(crate) vertex_count: u32,
    pub(crate) vertex_stride: u32,
    pub(crate) layout: crate::vertex::VertexLayout,
    pub(crate) usage: crate::renderer::registry::MeshUsage,
}

/// A material: compilation identity, compiled pipeline variants, and typed
/// texture bindings.
///
/// Each render-target configuration the material renders into resolves to
/// one [`PipelineVariantKey`](crate::renderer::pipeline_variant::PipelineVariantKey)
/// whose native pipeline lives in `variants`; the same identity is shared
/// with the Vulkan backend.
pub(crate) struct MetalMaterial {
    pub(crate) dependencies: std::collections::BTreeSet<std::path::PathBuf>,
    pub(crate) descriptor: crate::renderer::pipeline_descriptor::PipelineDescriptor,
    pub(crate) interface: crate::renderer::graphics_interface::GraphicsInterface,
    pub(crate) variants: std::collections::HashMap<
        crate::renderer::pipeline_variant::PipelineVariantKey,
        std::sync::Arc<super::pipeline::MetalGraphicsPipeline>,
    >,
    pub(crate) textures: crate::renderer::registry::MaterialTextures,
    pub(crate) pending_reload:
        Option<std::sync::mpsc::Receiver<Result<MetalMaterialReplacement, String>>>,
}

pub(crate) struct MetalMaterialReplacement {
    pub(crate) dependencies: std::collections::BTreeSet<std::path::PathBuf>,
    pub(crate) interface: crate::renderer::graphics_interface::GraphicsInterface,
    pub(crate) variants: std::collections::HashMap<
        crate::renderer::pipeline_variant::PipelineVariantKey,
        std::sync::Arc<super::pipeline::MetalGraphicsPipeline>,
    >,
}

/// A texture stored with its bindless slot.
pub(crate) struct MetalTextureEntry {
    pub(crate) texture: super::texture::MetalTexture,
    pub(crate) _view: MetalTextureView,
    pub(crate) bindless_slot: Option<u32>,
}

pub struct MetalRenderer {
    pub(crate) context: MetalContext,
    pub(crate) object_storage_buffers: [Option<MetalBuffer>; FRAMES_IN_FLIGHT],
    pub(crate) current_drawable_texture: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
    pub(crate) drawable_texture_view: Option<MetalTextureView>,
    pub(crate) frame_index: u32,
    /// The currently open frame-scoped token (see `renderer::frame_scope`).
    pub(crate) active_frame: Option<crate::renderer::frame_scope::FrameToken>,
    /// Monotonic counter handed to successive acquired frames.
    pub(crate) frame_generation: u64,
    /// Why the open frame is poisoned (a render failure); `present` refuses to submit.
    pub(crate) frame_poisoned: Option<String>,
    /// Whether the open frame acquired the current drawable from the surface
    /// (and must release it when aborted), as opposed to a headless drawable
    /// installed by `set_headless_drawable`.
    pub(crate) frame_owns_drawable: bool,
    pub(crate) meshes: ResourceStorage<MetalMesh, MeshMarker>,
    pub(crate) persistent_buffers: super::residency::PersistentBufferResidency,
    pub(crate) materials: ResourceStorage<MetalMaterial, MaterialMarker>,
    pub(crate) textures: ResourceStorage<MetalTextureEntry, TextureMarker>,
    pub(crate) skeletons: [ResourceStorage<MetalBuffer, SkeletonMarker>; FRAMES_IN_FLIGHT],
    pub(crate) buffer_history_retirement:
        std::cell::RefCell<super::buffer_history_retirement::BufferHistoryRetirement>,
    pub(crate) buffer_history: std::cell::RefCell<crate::render_graph::BufferExecutionHistory>,
    pub(crate) compute_pipelines: std::collections::HashMap<
        crate::render_graph::ComputePipelineDesc,
        super::pipeline::MetalComputePipeline,
    >,
    pub(crate) graph_buffers: ResourceStorage<super::buffer::MetalGraphBuffer, BufferMarker>,
    pub(crate) graph_buffer_owners:
        std::collections::HashMap<BufferHandle, super::submission::SubmissionCompletion>,
    pub(crate) bindless_manager: MetalBindlessTextureManager,
    pub(crate) default_texture: Option<TextureHandle>,
    pub(crate) size: Size2D,
    pub(crate) drawable_size: Size2D,
    pub(crate) frame_slots: Vec<super::frame_lifecycle::MetalFrameSlot>,
    pub(crate) last_submitted_slot: Option<usize>,
    pub(crate) last_submission: Option<(usize, u64, super::submission::SubmissionCompletion)>,
    pub(crate) pending_frame: Option<super::frame_lifecycle::MetalPendingFrame>,
    pub(crate) pending_buffer_accesses:
        std::cell::RefCell<Vec<super::frame_lifecycle::MetalBufferExecution>>,
    pub(crate) defined_output_contents: std::collections::HashSet<(u64, u8)>,
    pub(crate) frame_metrics: super::frame_lifecycle::MetalFrameMetrics,
    pub(crate) texture_uploads: super::texture_upload::TextureUploadQueue,
    pub(crate) texture_readbacks: super::texture_readback::MetalTextureReadbacks,
    skeleton_buffer_handles: std::collections::HashMap<(usize, SkeletonHandle), BufferHandle>,
    pub(crate) ui_renderers: [MetalUIRenderer; FRAMES_IN_FLIGHT],
    pub(crate) shared_sampler: Option<super::sampler::MetalSamplerState>,
    pub(crate) packet_samplers: Vec<(
        crate::renderer::frame_bindings::SamplingMode,
        super::sampler::MetalSamplerState,
    )>,
    pub(crate) capabilities: crate::renderer::types::GpuCapabilities,
    pub(crate) timestamp_queries: Option<super::timestamp_queries::MetalTimestampQueries>,
}

impl MetalRenderer {
    pub fn init(
        display: &dyn raw_window_handle::HasDisplayHandle,
        window: &dyn raw_window_handle::HasWindowHandle,
        validation_mode: crate::error::ValidationMode,
        _app_name: std::ffi::CString,
        _engine_name: std::ffi::CString,
    ) -> Result<Self, RendererError> {
        if validation_mode.is_enabled() {
            log::info!("Metal validation requested");
            let already_set = std::env::var("METAL_DEVICE_WRAPPER_TYPE").is_ok();
            if !already_set {
                log::warn!(
                    "METAL_DEVICE_WRAPPER_TYPE not set before process launch. \
                     Metal validation requires env vars to be set externally. \
                     Run: METAL_DEVICE_WRAPPER_TYPE=1 cargo run -- -s"
                );
            }
        }

        let context = MetalContext::init(window, display)?;
        let mut renderer = Self::new(context)?;

        let ds = renderer.context.surface.layer.drawableSize();
        let dw = ds.width as u32;
        let dh = ds.height as u32;
        if dw > 0 && dh > 0 {
            renderer.drawable_size = Size2D::new(dw, dh);
            renderer.size = Size2D::new(dw, dh);
        }

        Ok(renderer)
    }

    /// Create a headless Metal renderer without a window.
    ///
    /// Uses an offscreen CAMetalLayer at the specified resolution.
    /// Suitable for automated rendering and screenshot capture.
    pub fn init_headless(
        width: u32,
        height: u32,
        _validation_mode: crate::error::ValidationMode,
        _app_name: std::ffi::CString,
        _engine_name: std::ffi::CString,
    ) -> Result<Self, RendererError> {
        let context = MetalContext::init_headless_with_size(width, height)?;
        let mut renderer = Self::new(context)?;

        renderer.drawable_size = Size2D::new(width, height);
        renderer.size = Size2D::new(width, height);

        Ok(renderer)
    }

    /// Set the offscreen texture as the current drawable for headless rendering.
    ///
    /// This replaces the normal `acquire_next_drawable()` from CAMetalLayer.
    /// The texture must have Shared storage mode for CPU readback.
    pub fn set_headless_drawable(&mut self, texture: Retained<ProtocolObject<dyn MTLTexture>>) {
        self.frame_owns_drawable = false;
        self.current_drawable_texture = Some(texture.clone());
        self.drawable_texture_view = Some(super::texture::MetalTextureView::new(
            texture,
            super::texture::MetalTexture::new(
                self.current_drawable_texture.clone().unwrap(),
                ImageFormat::B8G8R8A8Srgb,
            ),
        ));
    }

    /// Submission timing and bounded CPU lead from the three-slot scheduler.
    pub fn frame_metrics(&self) -> &super::frame_lifecycle::MetalFrameMetrics {
        &self.frame_metrics
    }

    /// Observe the latest frame owner without waiting for native completion.
    pub fn capture_submission_snapshot(
        &self,
    ) -> Option<crate::render_graph::capture::CapturedSubmission> {
        let (slot, generation, completion) = if let Some(pending) = &self.pending_frame {
            let slot = self.frame_index();
            (
                slot,
                self.frame_slots[slot].generation,
                &pending.command.completion,
            )
        } else {
            let (slot, generation, completion) = self.last_submission.as_ref()?;
            (*slot, *generation, completion)
        };
        Some(crate::render_graph::capture::CapturedSubmission {
            frame_slot: slot,
            generation,
            command_allocator: slot,
            feedback_identity: format!("slot.{slot}.generation.{generation}"),
            feedback: completion.feedback_snapshot(),
        })
    }

    /// Wait for the exact submission producing the most recently presented output.
    pub fn wait_for_last_submission(&self) -> Result<(), RendererError> {
        let slot = self.last_submitted_slot.ok_or_else(|| {
            RendererError::InvalidOperation("No Metal frame has been submitted".into())
        })?;
        if let Some(submission) = &self.frame_slots[slot].submission {
            submission.wait_until_completed()?;
        }
        Ok(())
    }

    /// Take back the offscreen texture after rendering (for readback).
    pub fn take_headless_texture(&mut self) -> Option<Retained<ProtocolObject<dyn MTLTexture>>> {
        self.current_drawable_texture.take()
    }

    pub(crate) fn new(context: MetalContext) -> Result<Self, RendererError> {
        let frame_slots = (0..FRAMES_IN_FLIGHT)
            .map(|slot| super::frame_lifecycle::MetalFrameSlot::new(&context, slot))
            .collect::<Result<Vec<_>, _>>()?;
        let features = context.detect_features();
        let bindless_manager = MetalBindlessTextureManager::new(features.max_bindless_textures)?;

        let persistent_buffers = super::residency::PersistentBufferResidency::new(
            &context.device,
            "renderer_persistent_buffers",
        )?;
        let mut renderer = Self {
            context,
            persistent_buffers,
            object_storage_buffers: [const { None }; FRAMES_IN_FLIGHT],
            current_drawable_texture: None,
            drawable_texture_view: None,
            frame_index: 0,
            active_frame: None,
            frame_generation: 1,
            frame_poisoned: None,
            frame_owns_drawable: false,
            meshes: ResourceStorage::new(),
            materials: ResourceStorage::new(),
            textures: ResourceStorage::new(),
            skeletons: std::array::from_fn(|_| ResourceStorage::new()),
            graph_buffers: ResourceStorage::new(),
            graph_buffer_owners: std::collections::HashMap::new(),
            buffer_history_retirement: Default::default(),
            buffer_history: Default::default(),
            compute_pipelines: std::collections::HashMap::new(),
            bindless_manager,
            default_texture: None,
            size: Size2D::default(),
            drawable_size: Size2D::default(),
            frame_slots,
            last_submitted_slot: None,
            last_submission: None,
            pending_frame: None,
            pending_buffer_accesses: Default::default(),
            defined_output_contents: Default::default(),
            frame_metrics: Default::default(),
            texture_uploads: super::texture_upload::TextureUploadQueue::default(),
            texture_readbacks: Default::default(),
            skeleton_buffer_handles: Default::default(),
            ui_renderers: std::array::from_fn(|_| MetalUIRenderer::new()),
            shared_sampler: None,
            packet_samplers: Vec::new(),
            capabilities: {
                use crate::renderer::types::{GpuCapabilities, GpuVendor};
                GpuCapabilities {
                    max_texture_size: 16384,
                    max_bindless_textures: features.max_bindless_textures,
                    supports_compute: true,
                    clip_y_down: false,
                    max_frames_in_flight: FRAMES_IN_FLIGHT,
                    vendor: GpuVendor::Apple,
                }
            },
            timestamp_queries: None,
        };

        let default_tex = renderer.create_texture_solid([255, 255, 255, 255])?;
        renderer.default_texture = Some(default_tex);

        // Texture registration is valid before a shader layout exists. The argument
        // buffer itself is initialized lazily from the first compiled fragment
        // function so Metal, rather than Katla, owns the concrete layout ABI.
        if let Some(entry) = renderer.textures.get(default_tex) {
            renderer
                .bindless_manager
                .set_default_texture(&entry._view.inner);
        }

        // Create shared sampler for texture sampling
        renderer.shared_sampler = Some(renderer.context.create_sampler()?);

        renderer.timestamp_queries = Some(super::timestamp_queries::MetalTimestampQueries::new()?);
        if renderer.timestamp_queries.is_some() {
            log::info!("Metal timestamp queries initialized");
        }

        Ok(renderer)
    }

    fn ensure_uniform_buffers(&mut self) -> Result<(), RendererError> {
        let frame_idx = frame_slot(self.frame_index);
        if self.object_storage_buffers[frame_idx].is_none() {
            let object_size = MAX_OBJECTS_PER_FRAME as u64 * OBJECT_UNIFORM_SIZE;
            self.object_storage_buffers[frame_idx] =
                Some(self.context.create_buffer(object_size, true)?);
        }
        Ok(())
    }

    pub(crate) fn current_object_storage_buffer(&self) -> Option<&MetalBuffer> {
        let idx = frame_slot(self.frame_index);
        self.object_storage_buffers[idx].as_ref()
    }

    pub(crate) fn execute_metal_passes(
        &mut self,
        frame: &crate::renderer::frame_scope::FrameToken,
        pending: std::collections::HashMap<usize, crate::render_graph::PassExecutionData>,
        frame_graph: &crate::render_graph::FrameGraph<Self>,
        _frame_idx: usize,
    ) -> Result<crate::render_graph::ResourceExecutionTrace, RendererError> {
        let plan = super::execution_plan::MetalExecutionPlan::compile(
            frame_graph,
            ImageFormat::B8G8R8A8Srgb,
            Some(self),
        )
        .map_err(|error| RendererError::InvalidOperation(error.to_string()))?;

        for record in plan.sync_records() {
            log::trace!(
                "[Metal sync] before pass {:?}: resource {} bytes {:?} via {:?}",
                record.pass,
                record.resource.0,
                record.buffer_range,
                record.coverage
            );
        }

        self.render_frame(
            frame,
            &plan,
            pending,
            frame_graph,
            frame_graph.execution_trace_enabled(),
        )
    }

    /// Register a Metal texture with the bindless system (render graph backend).
    pub(crate) fn register_metal_bindless_texture(
        &mut self,
        texture: &objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLTexture>>,
    ) -> Result<u32, RendererError> {
        self.bindless_manager
            .register_transient_texture(texture)
            .map_err(|e| {
                RendererError::InvalidOperation(format!(
                    "Failed to register bindless texture: {}",
                    e
                ))
            })
    }

    pub(crate) fn update_metal_bindless_texture(
        &mut self,
        slot: u32,
        texture: &objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLTexture>>,
    ) -> Result<(), RendererError> {
        self.bindless_manager
            .update_transient_texture(slot, texture)
            .map_err(|e| {
                RendererError::InvalidOperation(format!(
                    "Failed to update bindless texture slot {}: {}",
                    slot, e
                ))
            })
    }

    /// Get the active ownership slot for per-frame resources.
    pub(crate) fn frame_index(&self) -> usize {
        frame_slot(self.frame_index)
    }
}

impl MetalRenderer {
    pub(crate) fn execute_draw_calls(&mut self, draw_list: &DrawList) -> Result<(), RendererError> {
        self.ensure_uniform_buffers()?;

        let object_buf = self.current_object_storage_buffer().unwrap();
        let buf_size = object_buf.size() as usize;
        validate_object_buffer_capacity(draw_list, buf_size)?;
        let ptr = object_buf.map();

        for draw in &draw_list.draws {
            let base = draw.instance_index as usize;
            let count = draw.instance_count().max(1) as usize;

            let emission_slot = self.resolve_emission_texture_slot_impl(draw.emission) as f32;
            let material_params = draw.material_params(emission_slot);

            let tex_indices: [u32; 4] = self.resolve_material_texture_slots_impl(draw.material);

            for (i, instance) in draw
                .instances
                .iter()
                .chain(std::iter::repeat(&InstanceData::default()))
                .take(count)
                .enumerate()
            {
                let offset = (base + i) * OBJECT_UNIFORM_SIZE as usize;
                debug_assert!(offset + OBJECT_UNIFORM_SIZE as usize <= buf_size);
                let dst = unsafe { ptr.add(offset) };

                unsafe {
                    std::ptr::copy_nonoverlapping(
                        instance.model_matrix.as_ptr(),
                        dst as *mut f32,
                        16,
                    );
                    std::ptr::copy_nonoverlapping(
                        instance.color.as_ptr(),
                        dst.add(64) as *mut f32,
                        4,
                    );
                    std::ptr::copy_nonoverlapping(
                        material_params.as_ptr(),
                        dst.add(80) as *mut f32,
                        4,
                    );
                    std::ptr::copy_nonoverlapping(tex_indices.as_ptr(), dst.add(96) as *mut u32, 4);
                }
            }
        }

        object_buf.unmap();

        Ok(())
    }
}

impl GpuRenderer for MetalRenderer {
    fn acquire_frame(
        &mut self,
    ) -> Result<crate::renderer::frame_scope::FrameAcquisition, RendererError> {
        super::frame_lifecycle::acquire_frame(self)
    }

    fn execute_draw_calls(
        &mut self,
        frame: &crate::renderer::frame_scope::FrameToken,
        draw_list: &DrawList,
    ) -> Result<(), RendererError> {
        self.frame_write_check(frame)?;
        MetalRenderer::execute_draw_calls(self, draw_list)
    }

    fn present(
        &mut self,
        frame: crate::renderer::frame_scope::FrameToken,
    ) -> Result<crate::renderer::frame_scope::PresentOutcome, RendererError> {
        MetalRenderer::present_frame(self, frame)
    }

    fn abort(
        &mut self,
        frame: crate::renderer::frame_scope::FrameToken,
    ) -> Result<(), RendererError> {
        super::frame_lifecycle::abort_frame(self, frame);
        Ok(())
    }

    fn swapchain_extent(&self) -> Size2D {
        self.drawable_size
    }

    fn current_frame(&self) -> usize {
        self.frame_index()
    }

    fn num_images(&self) -> usize {
        FRAMES_IN_FLIGHT
    }

    fn wait_for_device(&self) {
        let mut successful = true;
        for slot in &self.frame_slots {
            if let Some(submission) = &slot.submission
                && let Err(error) = submission.wait_until_completed()
            {
                log::error!("Metal device drain failed: {error}");
                successful = false;
            }
        }
        if let Err(error) = self.texture_readbacks.wait_pending() {
            log::error!("Metal readback drain failed: {error}");
            successful = false;
        }
        if successful {
            self.buffer_history.borrow_mut().clear();
            self.buffer_history_retirement.borrow_mut().clear();
        }
    }

    fn create_buffer(
        &mut self,
        desc: crate::render_graph::BufferDesc,
    ) -> Result<BufferHandle, RendererError> {
        if desc.size == 0 || desc.usages.is_empty() {
            return Err(RendererError::InvalidOperation(
                "Buffer size and usage must be non-empty".into(),
            ));
        }
        let cpu_accessible = matches!(
            desc.memory,
            crate::render_graph::BufferMemoryPolicy::CpuVisible
                | crate::render_graph::BufferMemoryPolicy::Readback
        );
        let buffer = self.context.create_buffer(desc.size, cpu_accessible)?;
        self.persistent_buffers.replace(&[], &[&buffer.inner])?;
        Ok(self
            .graph_buffers
            .insert(super::buffer::MetalGraphBuffer::new(buffer, desc)))
    }

    fn capture_submission_snapshot(
        &self,
    ) -> Option<crate::render_graph::capture::CapturedSubmission> {
        MetalRenderer::capture_submission_snapshot(self)
    }

    fn buffer_descriptor(&self, handle: BufferHandle) -> Option<crate::render_graph::BufferDesc> {
        self.graph_buffers.get(handle).map(|buffer| buffer.desc)
    }

    fn frame_slot_count(&self) -> usize {
        FRAMES_IN_FLIGHT
    }

    fn create_buffer_with_data(
        &mut self,
        desc: crate::render_graph::BufferDesc,
        data: &[u8],
    ) -> Result<BufferHandle, RendererError> {
        if data.len() as u64 > desc.size {
            return Err(RendererError::InvalidOperation(
                "Initial data exceeds buffer capacity".into(),
            ));
        }
        let handle = self.create_buffer(desc)?;
        let target = self
            .graph_buffers
            .get(handle)
            .ok_or_else(|| RendererError::InvalidOperation("Buffer creation failed".into()))?;
        let upload = self.context.create_buffer(desc.size, true)?;
        unsafe {
            std::ptr::write_bytes(upload.map(), 0, desc.size as usize);
            std::ptr::copy_nonoverlapping(data.as_ptr(), upload.map(), data.len());
        }
        upload.unmap();
        let mut command = self.context.create_command_buffer();
        command.begin();
        let mut blit = crate::backend::command::GpuCommandBuffer::begin_blit_pass(&mut command);
        crate::backend::command::GpuBlitEncoder::copy_buffer_to_buffer(
            &mut blit,
            &upload,
            0,
            &target.buffer,
            0,
            desc.size,
        );
        crate::backend::command::GpuBlitEncoder::end_encoding(blit);
        command.end();
        command.submit(&self.context);
        command.wait_until_completed()?;
        Ok(handle)
    }

    fn graph_texture_source(
        &self,
        resource: crate::render_graph::ResourceId,
    ) -> Option<crate::renderer::texture_readback::GraphTextureSource> {
        self.texture_readbacks.source(resource)
    }

    fn queue_texture_readback(
        &mut self,
        source: crate::renderer::texture_readback::GraphTextureSource,
        region: crate::renderer::texture_readback::TextureReadbackRegion,
    ) -> Result<crate::renderer::texture_readback::TextureReadbackTicket, RendererError> {
        self.texture_readbacks.queue(&self.context, source, region)
    }

    fn poll_texture_readback(
        &mut self,
        ticket: crate::renderer::texture_readback::TextureReadbackTicket,
    ) -> Result<Option<crate::renderer::texture_readback::TextureReadbackData>, RendererError> {
        self.texture_readbacks.poll(ticket)
    }

    fn skeleton_buffer_handle(
        &mut self,
        frame: &crate::renderer::frame_scope::FrameToken,
        skeleton: SkeletonHandle,
    ) -> Result<BufferHandle, RendererError> {
        self.frame_write_check(frame)?;
        let slot = frame.slot();
        let buffer =
            self.skeletons[slot]
                .get(skeleton)
                .ok_or_else(|| RendererError::StaleHandle {
                    resource: "skeleton".into(),
                    detail: format!("handle {}", skeleton.index()),
                })?;
        let desc = crate::render_graph::BufferDesc::new(
            buffer.size(),
            crate::render_graph::BufferUsages::STORAGE
                | crate::render_graph::BufferUsages::TRANSFER_DESTINATION
                | crate::render_graph::BufferUsages::TRANSFER_SOURCE,
            crate::render_graph::BufferMemoryPolicy::CpuVisible,
        );
        let graph_buffer = super::buffer::MetalGraphBuffer::new(buffer.clone(), desc);
        if let Some(handle) = self.skeleton_buffer_handles.get(&(slot, skeleton)).copied()
            && let Some(target) = self.graph_buffers.get_mut(handle)
        {
            *target = graph_buffer;
            return Ok(handle);
        }
        let handle = self.graph_buffers.insert(graph_buffer);
        self.skeleton_buffer_handles
            .insert((slot, skeleton), handle);
        Ok(handle)
    }

    fn read_buffer_completed(
        &mut self,
        handle: BufferHandle,
        range: crate::render_graph::BufferByteRange,
    ) -> Result<Option<Vec<u8>>, RendererError> {
        let buffer = self
            .graph_buffers
            .get(handle)
            .ok_or_else(|| RendererError::StaleHandle {
                resource: "buffer".into(),
                detail: format!("handle {}", handle.index()),
            })?;
        if buffer.desc.memory != crate::render_graph::BufferMemoryPolicy::Readback {
            return Err(RendererError::InvalidOperation(
                "Completed buffer reads require readback memory".into(),
            ));
        }
        let bytes = range
            .size
            .min(buffer.desc.size.saturating_sub(range.offset));
        if bytes == 0
            || range.offset >= buffer.desc.size
            || range.size != u64::MAX && range.size > buffer.desc.size - range.offset
        {
            return Err(RendererError::InvalidOperation(
                "Readback range exceeds buffer capacity".into(),
            ));
        }
        let Some(owner) = self.graph_buffer_owners.get(&handle) else {
            return Ok(None);
        };
        if !owner.is_complete() {
            return Ok(None);
        }
        if owner.feedback_snapshot() == crate::render_graph::capture::CapturedFeedback::Failed {
            return Err(RendererError::InvalidOperation(
                "Readback's owning submission failed".into(),
            ));
        }
        let data = unsafe {
            std::slice::from_raw_parts(
                buffer
                    .buffer
                    .map()
                    .add((buffer.offset + range.offset) as usize),
                bytes as usize,
            )
        }
        .to_vec();
        Ok(Some(data))
    }

    fn write_buffer(
        &mut self,
        frame: &crate::renderer::frame_scope::FrameToken,
        handle: BufferHandle,
        offset: u64,
        data: &[u8],
    ) -> Result<(), RendererError> {
        self.frame_write_check(frame)?;
        if self
            .graph_buffer_owners
            .get(&handle)
            .is_some_and(|owner| !owner.is_complete())
        {
            return Err(RendererError::InvalidOperation(
                "Buffer is still owned by an in-flight submission".into(),
            ));
        }
        let buffer = self
            .graph_buffers
            .get(handle)
            .ok_or_else(|| RendererError::StaleHandle {
                resource: "buffer".into(),
                detail: format!("handle {}", handle.index()),
            })?;
        if !matches!(
            buffer.desc.memory,
            crate::render_graph::BufferMemoryPolicy::CpuVisible
                | crate::render_graph::BufferMemoryPolicy::Readback
        ) {
            return Err(RendererError::InvalidOperation(
                "CPU buffer writes require CPU-visible memory".into(),
            ));
        }
        let end = offset
            .checked_add(data.len() as u64)
            .ok_or_else(|| RendererError::InvalidOperation("Buffer write range overflow".into()))?;
        if end > buffer.desc.size {
            return Err(RendererError::InvalidOperation(
                "Buffer write exceeds its declared range".into(),
            ));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                data.as_ptr(),
                buffer.buffer.map().add((buffer.offset + offset) as usize),
                data.len(),
            );
        }
        buffer
            .buffer
            .flush(buffer.offset + offset, data.len() as u64);
        Ok(())
    }

    fn destroy_buffer(&mut self, handle: BufferHandle) -> Result<(), RendererError> {
        let buffer = self.graph_buffers.get(handle).ok_or_else(|| {
            RendererError::InvalidOperation(format!("Unknown buffer handle {}", handle.index()))
        })?;
        self.persistent_buffers
            .replace(&[&buffer.buffer.inner], &[])?;
        self.graph_buffer_owners.remove(&handle);
        self.graph_buffers.remove(handle).map(drop).ok_or_else(|| {
            RendererError::InvalidOperation(format!("Unknown buffer handle {}", handle.index()))
        })
    }

    fn capabilities(&self) -> &crate::renderer::types::GpuCapabilities {
        &self.capabilities
    }

    fn supports_feature(&self, _feature: crate::renderer::features::RendererFeature) -> bool {
        true
    }

    fn destroy(&mut self) {
        if let Err(error) = self.persistent_buffers.clear() {
            self.frame_poisoned = Some(error.to_string());
        }
        self.meshes = ResourceStorage::new();
        self.materials = ResourceStorage::new();
        self.textures = ResourceStorage::new();
        self.skeletons = std::array::from_fn(|_| ResourceStorage::new());
        self.graph_buffers = ResourceStorage::new();
    }

    fn create_mesh<T, U>(
        &mut self,
        vertices: &[T],
        indices: &[U],
        topology: crate::renderer::registry::PrimitiveTopology,
    ) -> Result<MeshHandle, RendererError>
    where
        T: crate::vertex::Vertex,
        U: crate::renderer::registry::MeshIndexElement,
    {
        self.create_mesh_from_vertices(vertices, indices, topology)
    }

    fn mesh_index_format(&self, mesh: MeshHandle) -> Option<crate::backend::command::IndexType> {
        self.meshes
            .contains(mesh)
            .then_some(crate::backend::command::IndexType::Uint32)
    }

    fn mesh_vertex_count(&self, mesh: MeshHandle) -> Option<u32> {
        self.meshes.get(mesh).map(|m| m.vertex_count)
    }

    fn mesh_index_count(&self, mesh: MeshHandle) -> Option<u32> {
        self.meshes.get(mesh).map(|m| m.index_count)
    }

    fn create_mesh_dynamic(
        &mut self,
        descriptor: &crate::renderer::registry::MeshDescriptor,
        vertex_data: &[u8],
        indices: &[u32],
    ) -> Result<MeshHandle, RendererError> {
        self.register_mesh_raw_impl(descriptor, vertex_data, indices)
    }

    fn update_mesh_dynamic(
        &mut self,
        mesh: MeshHandle,
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError> {
        self.update_mesh_dynamic_impl(mesh, vertex_data, vertex_count, indices)
    }

    fn create_texture(
        &mut self,
        desc: &TextureDescriptor,
        data: &[u8],
    ) -> Result<TextureHandle, RendererError> {
        self.create_texture_impl(desc, data)
    }

    fn create_texture_solid(&mut self, color: [u8; 4]) -> Result<TextureHandle, RendererError> {
        self.create_texture_solid_impl(color)
    }

    fn update_texture_region(
        &mut self,
        handle: TextureHandle,
        region: crate::texture::TextureUploadRegion,
        data: &[u8],
    ) -> Result<(), RendererError> {
        self.update_texture_region_impl(handle, region, data)
    }

    fn pending_texture_uploads(&self) -> Vec<(TextureHandle, crate::texture::TextureUploadRegion)> {
        self.pending_texture_uploads_impl()
    }

    fn texture_upload_metrics(&self) -> Option<crate::texture::TextureUploadMetrics> {
        Some(self.texture_uploads.metrics())
    }

    fn update_texture(&mut self, handle: TextureHandle, data: &[u8]) -> Result<(), RendererError> {
        self.update_texture_impl(handle, data)
    }

    fn get_bindless_slot(&self, handle: TextureHandle) -> Option<u32> {
        self.get_bindless_slot_impl(handle)
    }

    fn get_texture_at_slot(&self, slot: u32) -> Option<TextureHandle> {
        self.get_texture_at_slot_impl(slot)
    }

    fn get_texture_bindless_index(&self, handle: TextureHandle) -> u32 {
        self.get_bindless_slot(handle).unwrap_or(0)
    }

    fn default_texture(&self) -> TextureHandle {
        self.default_texture_impl()
    }

    fn destroy_mesh(&mut self, handle: MeshHandle) {
        if let Some(mesh) = self.meshes.get(handle) {
            if let Err(error) = self
                .persistent_buffers
                .replace(&[&mesh.vertex_buffer.inner, &mesh.index_buffer.inner], &[])
            {
                self.frame_poisoned = Some(error.to_string());
            }
            self.meshes.remove(handle);
        }
    }

    fn destroy_texture(&mut self, handle: TextureHandle) {
        self.destroy_texture_impl(handle)
    }

    fn compile_material(
        &mut self,
        descriptor: &PipelineDescriptor,
    ) -> Result<MaterialHandle, RendererError> {
        self.compile_material_impl(descriptor)
    }

    fn material_descriptor(&self, material: MaterialHandle) -> Option<&PipelineDescriptor> {
        self.materials.get(material).map(|value| &value.descriptor)
    }

    fn material_textures(
        &self,
        material: MaterialHandle,
    ) -> Option<crate::renderer::registry::MaterialTextures> {
        self.materials.get(material).map(|value| value.textures)
    }

    fn set_material_textures(
        &mut self,
        material: MaterialHandle,
        textures: crate::renderer::registry::MaterialTextures,
    ) {
        self.set_material_textures_impl(material, textures)
    }

    fn recompile_materials_for_shader(&mut self, shader_path: &std::path::Path) -> usize {
        self.recompile_materials_for_shader_impl(shader_path)
    }

    fn destroy_material(&mut self, handle: MaterialHandle) {
        self.destroy_material_impl(handle)
    }

    fn destroy_skeleton(&mut self, handle: SkeletonHandle) {
        self.destroy_skeleton_impl(handle)
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), RendererError> {
        self.size = Size2D::new(width, height);
        self.context.surface.resize(width, height);
        let ds = self.context.surface.layer.drawableSize();
        let dw = ds.width as u32;
        let dh = ds.height as u32;
        if dw > 0 && dh > 0 {
            self.drawable_size = Size2D::new(dw, dh);
        }
        Ok(())
    }

    fn create_skeleton(&mut self, joint_count: usize) -> Result<SkeletonHandle, RendererError> {
        self.create_skeleton_impl(joint_count)
    }

    // -- Shadows --

    // -- Animation --

    // -- Pipeline Initialization --

    // -- UI Rendering --

    fn begin_timestamp(&mut self, label: &str) {
        if let Some(ref mut tq) = self.timestamp_queries {
            tq.begin(label);
        }
    }

    fn end_timestamp(&mut self, label: &str) {
        if let Some(ref mut tq) = self.timestamp_queries {
            tq.end(label);
        }
    }

    fn read_timestamps(&self) -> Vec<crate::renderer::types::GpuTimestamp> {
        if let Some(ref tq) = self.timestamp_queries {
            for slot in &self.frame_slots {
                if let Some(timestamps) = &slot.timestamps {
                    tq.cache_completed(timestamps);
                }
            }
            tq.cached_results()
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::gpu_renderer::GpuRenderer;
    use crate::renderer::types::{DrawCall, DrawList};

    fn create_renderer() -> MetalRenderer {
        let context = MetalContext::init_headless().expect("Failed to create headless context");
        MetalRenderer::new(context).expect("Failed to create MetalRenderer")
    }

    #[test]
    fn test_foreign_renderer_token_rejects_write_render_and_present_without_consuming_owner() {
        use crate::render_graph::{
            BufferDesc, BufferMemoryPolicy, BufferUsages, FrameGraphBuilder, PassType, SimplePass,
        };
        use crate::render_pass::{AttachmentOps, ClearValue};
        use crate::renderer::frame_scope::SurfaceStatus;
        let mut first = create_renderer();
        let mut second = create_renderer();
        let foreign = super::super::test_support::acquire(&mut first, 8);
        let own = super::super::test_support::acquire(&mut second, 8);
        assert_eq!(foreign.slot(), own.slot());
        assert_ne!(foreign, own);
        let copied = own;
        let buffer = second
            .create_buffer(BufferDesc::new(
                4,
                BufferUsages::STORAGE,
                BufferMemoryPolicy::CpuVisible,
            ))
            .unwrap();
        second.write_buffer(&copied, buffer, 0, &[7; 4]).unwrap();
        assert!(matches!(
            second.write_buffer(&foreign, buffer, 0, &[99; 4]),
            Err(RendererError::InvalidOperation(_))
        ));
        let bytes = unsafe {
            std::slice::from_raw_parts(second.graph_buffers.get(buffer).unwrap().buffer.map(), 4)
        };
        assert_eq!(bytes, [7; 4]);
        let mut graph = FrameGraphBuilder::new()
            .add_pass(
                SimplePass::new("clear", PassType::Graphics)
                    .without_depth()
                    .write("backbuffer")
                    .attachment("backbuffer", AttachmentOps::clear(ClearValue::OPAQUE_BLACK)),
            )
            .build::<MetalRenderer>()
            .unwrap();
        assert!(matches!(
            second.render(&foreign, &mut graph, |_| {}),
            Err(RendererError::InvalidOperation(_))
        ));
        assert!(second.frame_poisoned.is_none());
        assert_eq!(second.active_frame, Some(own));
        second.render(&copied, &mut graph, |_| {}).unwrap();
        assert!(matches!(
            second.present(foreign),
            Err(RendererError::InvalidOperation(_))
        ));
        assert_eq!(second.active_frame, Some(own));
        assert!(second.pending_frame.is_some());
        assert!(second.frame_slots[own.slot()].submission.is_none());
        assert_eq!(
            second.present(own).unwrap().surface.unwrap(),
            SurfaceStatus::Presented
        );
        second.wait_for_last_submission().unwrap();
        first.abort(foreign).unwrap();
    }

    #[test]
    fn test_present_requires_recorded_submission_and_keeps_aborted_slot_reusable() {
        use crate::render_graph::{FrameGraphBuilder, PassType, SimplePass};
        use crate::render_pass::{AttachmentOps, ClearValue};
        use crate::renderer::frame_scope::SurfaceStatus;
        let mut renderer = create_renderer();
        let frame = super::super::test_support::acquire(&mut renderer, 8);
        let error = renderer.present(frame).unwrap_err();
        assert!(matches!(error, RendererError::InvalidOperation(_)));
        assert!(renderer.frame_slots[frame.slot()].submission.is_none());
        assert!(renderer.last_submission.is_none());
        assert_eq!(renderer.frame_index(), frame.slot());
        renderer.abort(frame).unwrap();
        let recovered = super::super::test_support::acquire(&mut renderer, 8);
        assert_eq!(recovered.slot(), frame.slot());
        let mut graph = FrameGraphBuilder::new()
            .add_pass(
                SimplePass::new("clear", PassType::Graphics)
                    .without_depth()
                    .write("backbuffer")
                    .attachment("backbuffer", AttachmentOps::clear(ClearValue::OPAQUE_BLACK)),
            )
            .build::<MetalRenderer>()
            .unwrap();
        renderer.render(&recovered, &mut graph, |_| {}).unwrap();
        let outcome = renderer.present(recovered).unwrap();
        assert_eq!(outcome.surface.unwrap(), SurfaceStatus::Presented);
        assert!(renderer.frame_slots[recovered.slot()].submission.is_some());
        renderer.wait_for_last_submission().unwrap();
    }

    #[test]
    fn test_frame_slots_wrap_at_the_renderer_slot_count() {
        assert_eq!(frame_slot(0), 0);
        assert_eq!(frame_slot(1), 1);
        assert_eq!(frame_slot(2), 2);
        assert_eq!(frame_slot(3), 0);
        assert_eq!(frame_slot(4), 1);
        assert_eq!(frame_slot(5), 2);
    }

    #[test]
    fn test_metal_renderer_creation() {
        let renderer = create_renderer();

        assert!(
            renderer.default_texture.is_some(),
            "default_texture should be set"
        );
        assert!(renderer.materials.is_empty());
        assert!(renderer.compute_pipelines.is_empty());
        assert!(renderer.graph_buffers.is_empty());
        assert!(renderer.meshes.is_empty());
        assert_eq!(
            renderer.textures.len(),
            1,
            "only the descriptor-safe white texture exists"
        );
        assert!(renderer.object_storage_buffers.iter().all(Option::is_none));
        assert!(renderer.packet_samplers.is_empty());
        assert!(
            renderer
                .frame_slots
                .iter()
                .all(|slot| slot.timestamps.is_none())
        );
        assert!(renderer.ui_renderers.iter().all(|ui| ui.is_unallocated()));
    }

    #[test]
    fn test_metal_skeleton_create_update() {
        let mut renderer = create_renderer();

        let skeleton = renderer.create_skeleton(4).expect("create_skeleton failed");
        assert!(skeleton.is_some(), "skeleton handle should be valid");

        let identity = [[
            1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]; 4];
        let frame = super::super::test_support::acquire(&mut renderer, 16);
        let handle = renderer.skeleton_buffer_handle(&frame, skeleton).unwrap();
        renderer
            .write_buffer(&frame, handle, 0, bytemuck::cast_slice(&identity))
            .unwrap();
        assert_eq!(renderer.buffer_descriptor(handle).unwrap().size, 256);
        renderer.abort(frame).unwrap();
    }

    #[test]
    fn test_metal_primitive_meshes() {
        let mut renderer = create_renderer();

        let cube = crate::primitives::create_cube(&mut renderer, [1.0, 1.0, 1.0])
            .expect("cube creation should succeed");
        assert!(cube.is_some(), "cube handle should be valid");

        let sphere = crate::primitives::create_sphere(&mut renderer, 1.0, 16, 16)
            .expect("sphere creation should succeed");
        assert!(sphere.is_some(), "sphere handle should be valid");

        let plane = crate::primitives::create_plane(&mut renderer, 2.0, 2.0)
            .expect("plane creation should succeed");
        assert!(plane.is_some(), "plane handle should be valid");

        assert_ne!(
            cube, sphere,
            "different meshes should have different handles"
        );
        assert_ne!(
            sphere, plane,
            "different meshes should have different handles"
        );
    }

    #[test]
    fn test_metal_texture_creation() {
        let mut renderer = create_renderer();

        let red_tex = renderer
            .create_texture_solid([255, 0, 0, 255])
            .expect("solid texture creation should succeed");
        assert!(red_tex.is_some(), "texture handle should be valid");

        let bindless_index = renderer.get_texture_bindless_index(red_tex);
        assert_ne!(
            bindless_index, 0,
            "custom texture should have a non-zero bindless slot"
        );

        let slot = renderer.get_bindless_slot(red_tex);
        assert!(
            slot.is_some(),
            "bindless slot should be allocated for custom texture"
        );
    }

    #[test]
    fn test_metal_execute_draw_calls() {
        let mut renderer = create_renderer();

        let default_mesh = crate::primitives::create_cube(&mut renderer, [1.0, 1.0, 1.0])
            .expect("cube creation should succeed");
        let default_mat = MaterialHandle::NONE;

        let draw = DrawCall::new(default_mesh, default_mat);
        let mut draw_list = DrawList::new();
        draw_list.push(draw);

        let result = renderer.execute_draw_calls(&draw_list);
        assert!(
            result.is_ok(),
            "execute_draw_calls should succeed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_metal_mesh_dynamic_update() {
        use crate::renderer::registry::{MeshDescriptor, MeshUsage, PrimitiveTopology};
        use crate::vertex::{AttributeType, VertexLayout};
        let mut renderer = create_renderer();

        // 3 vertices of one Float4 each (position-like), explicit descriptor.
        let vertex_data: [f32; 12] = [
            -0.5, -0.5, 0.0, 1.0, 0.5, -0.5, 0.0, 1.0, 0.0, 0.5, 0.0, 1.0,
        ];
        let indices: [u32; 3] = [0, 1, 2];
        let vertex_bytes = bytemuck::cast_slice(&vertex_data);
        let descriptor = MeshDescriptor {
            layout: VertexLayout::new(vec![crate::vertex::VertexAttributeFormat::Float4]),
            attributes: vec![AttributeType::Position],
            topology: PrimitiveTopology::TriangleList,
            usage: MeshUsage::Dynamic,
            vertex_count: 3,
            index_count: 3,
            index_format: crate::backend::command::IndexType::Uint32,
        };

        let mesh = renderer
            .create_mesh_dynamic(&descriptor, vertex_bytes, &indices)
            .expect("dynamic mesh creation should succeed");
        assert_eq!(renderer.mesh_vertex_count(mesh), Some(3));
        assert_eq!(renderer.mesh_index_count(mesh), Some(3));

        let updated_verts: [f32; 12] = [
            -1.0, -1.0, 0.0, 1.0, 1.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0,
        ];
        let updated_bytes = bytemuck::cast_slice(&updated_verts);
        renderer
            .update_mesh_dynamic(mesh, updated_bytes, 3, &indices)
            .expect("same-size update should succeed");
        assert_eq!(renderer.mesh_index_count(mesh), Some(3));

        // Growth beyond the created capacity reallocate safely and publish
        // the larger counts.
        let grown_verts: [f32; 24] = [
            -1.0, -1.0, 0.0, 1.0, 1.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, -0.5, -0.5, 0.5, 1.0,
            0.5, -0.5, 0.5, 1.0, 0.0, 0.5, 0.5, 1.0,
        ];
        let grown_bytes = bytemuck::cast_slice(&grown_verts);
        let grown_indices: [u32; 6] = [0, 1, 2, 3, 4, 5];
        renderer
            .update_mesh_dynamic(mesh, grown_bytes, 6, &grown_indices)
            .expect("growing update should reallocate and succeed");
        assert_eq!(renderer.mesh_vertex_count(mesh), Some(6));
        assert_eq!(renderer.mesh_index_count(mesh), Some(6));

        // Populated → empty: valid, draws nothing.
        renderer
            .update_mesh_dynamic(mesh, &[], 0, &[])
            .expect("empty update should succeed");
        assert_eq!(renderer.mesh_vertex_count(mesh), Some(0));
        assert_eq!(renderer.mesh_index_count(mesh), Some(0));

        // Empty → populated again through growth.
        renderer
            .update_mesh_dynamic(mesh, updated_bytes, 3, &indices)
            .expect("repopulation should succeed");
        assert_eq!(renderer.mesh_vertex_count(mesh), Some(3));

        // Inconsistent payloads fail typed and change nothing.
        let error = renderer
            .update_mesh_dynamic(mesh, updated_bytes, 4, &indices)
            .expect_err("blob disagreeing with vertex_count must fail");
        assert!(matches!(
            error,
            crate::error::RendererError::InvalidDescriptor { .. }
        ));
        let error = renderer
            .update_mesh_dynamic(mesh, updated_bytes, 3, &[0, 1, 3])
            .expect_err("out-of-range index must fail");
        assert!(matches!(
            error,
            crate::error::RendererError::InvalidDescriptor { .. }
        ));
        assert_eq!(renderer.mesh_vertex_count(mesh), Some(3));
        assert_eq!(renderer.mesh_index_count(mesh), Some(3));
    }

    // --- Headless render test helpers ---

    #[test]
    fn test_headless_custom_pass_renders_without_scene_initialization() {
        use crate::render_graph::{FrameGraphBuilder, PassKind, PassType, SimplePass};
        use crate::render_pass::{AttachmentOps, ClearValue};
        let mut renderer = create_renderer();
        let source = format!(
            "{} @fragment fn fs_main()->@location(0) vec4<f32>{{return vec4<f32>(1.,0.,0.,1.);}}",
            super::super::test_support::FULLSCREEN_VERTEX
        );
        let material = super::super::test_support::material(
            &mut renderer,
            &source,
            super::super::test_support::fullscreen_descriptor(ImageFormat::B8G8R8A8Srgb),
        );
        let mut graph = FrameGraphBuilder::new()
            .export_resource("backbuffer")
            .add_pass(
                SimplePass::new("custom", PassType::Graphics)
                    .without_depth()
                    .write("backbuffer")
                    .attachment("backbuffer", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                    .with_kind(PassKind::Fullscreen),
            )
            .build::<MetalRenderer>()
            .unwrap();
        graph
            .set_pass_bindings(
                graph.pass_id("custom").unwrap(),
                super::super::test_support::vertices(
                    material,
                    crate::vertex::VertexLayout::new(vec![]),
                    3,
                ),
            )
            .unwrap();
        let frame = super::super::test_support::acquire(&mut renderer, 16);
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        renderer.present(frame).unwrap();
        let source = renderer
            .graph_texture_source(graph.resource_id("backbuffer").unwrap())
            .unwrap();
        let ticket = renderer
            .queue_texture_readback(
                source,
                crate::renderer::texture_readback::TextureReadbackRegion::pixel(8, 8),
            )
            .unwrap();
        assert_eq!(
            super::super::test_support::readback(&mut renderer, ticket).bytes,
            [0, 0, 255, 255]
        );
        assert_eq!(renderer.materials.len(), 1);
        assert!(renderer.object_storage_buffers.iter().all(Option::is_none));
        assert!(renderer.ui_renderers.iter().all(|ui| ui.is_unallocated()));
    }
}

impl Drop for MetalRenderer {
    fn drop(&mut self) {
        self.wait_for_device();
        if let Some(archive) = self.context.pipeline_archive.as_ref() {
            let stats = archive.stats();
            let origin = match stats.rejection {
                None => "opened from disk".to_string(),
                Some(rejection) => format!("rebuilt ({rejection:?})"),
            };
            log::info!(
                "Pipeline cache summary: hits={}, misses={}, registered={}, origin={origin}, open_ms={}, persistence_ms={:?}",
                stats.hits,
                stats.misses,
                stats.pipelines_registered,
                stats.open_duration.as_millis(),
                stats.last_flush_duration.map(|d| d.as_millis()),
            );
        }
    }
}
