//! Backend-agnostic renderer trait.
//!
//! Defines the [`GpuRenderer`] trait that captures the public rendering API used by
//! `katla_app`. Both `VulkanRenderer` and `MetalRenderer` implement this trait,
//! allowing `katla_app` to be generic over the graphics backend.
//!
//! The trait uses only Katla-native types (handles, descriptors, enums) so that
//! both Vulkan and Metal backends can implement it without exposing their internals.

use crate::Size2D;
use crate::error::RendererError;
use crate::handle::{BufferHandle, MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle};
use crate::render_graph::BufferDesc;
use crate::renderer::features::RendererFeature;
use crate::renderer::frame_scope::{FrameAcquisition, FrameToken, PresentOutcome};
use crate::renderer::pipeline_descriptor::PipelineDescriptor;
use crate::renderer::registry::PrimitiveTopology;
use crate::renderer::types::DrawList;
use crate::texture::TextureDescriptor;

/// Backend-agnostic renderer interface.
///
/// Covers resource creation (buffers, meshes, textures, materials and skeleton storage),
/// the frame-scoped lifecycle, and teardown. All method signatures use
/// Katla-native types only — no `vk::`, `ash::`, or Metal types appear in the trait.
///
/// # Frame-Scoped Lifecycle
///
/// Rendering one frame means owning one frame token:
///
/// 1. [`GpuRenderer::acquire_frame`] — waits for a free frame slot (and, when
///    windowed, acquires the surface image), returning [`FrameAcquisition`]:
///    `Ready(token)`, `Unavailable`, or `OutOfDate`.
/// 2. Frame-local geometry uploads and generic buffer writes take the token.
/// 3. The renderer's inherent `render` method executes the frame graph for the
///    token's frame.
/// 4. [`GpuRenderer::present`] consumes the token: submit + present, identical
///    semantics on Vulkan and Metal.
///
/// A token from an abandoned acquisition is aborted at the next `acquire_frame`:
/// nothing is submitted or presented and no slot is stranded. There is no
/// implicit call-ordering path beside this one.
pub trait GpuRenderer: Sized + 'static {
    /// Acquire one frame: wait for a free reusable slot and, when windowed,
    /// acquire the next surface image.
    ///
    /// Acquiring implicitly aborts a still-open frame from an earlier
    /// acquisition (logged at debug level), so an abandoned frame can never
    /// strand a slot.
    ///
    /// - `Ready` — render and finish with `present` (or `abort` to skip).
    /// - `Unavailable` — the surface cannot produce a frame right now; nothing
    ///   was touched, skip and retry.
    /// - `OutOfDate` — the surface is stale; recreate it and acquire again.
    fn acquire_frame(&mut self) -> Result<FrameAcquisition, RendererError>;

    /// Write per-object draw data into this frame's slot storage.
    fn execute_draw_calls(
        &mut self,
        frame: &FrameToken,
        draw_list: &DrawList,
    ) -> Result<(), RendererError>;

    /// Submit this frame's recorded work and present it. Consumes the token.
    ///
    /// Identical semantics on Vulkan and Metal: `present` returns after the
    /// submission is enqueued (Vulkan) or the presenting command buffer is
    /// committed (Metal), not after the GPU retires.
    /// An outer error guarantees no GPU submission was accepted. Every successful
    /// outcome requires the caller to advance its submitted-frame state, then
    /// inspect the outcome's surface result for recreation or presentation failure.
    fn present(&mut self, frame: FrameToken) -> Result<PresentOutcome, RendererError>;

    /// Abandon the frame without submitting or presenting. Consumes the token.
    fn abort(&mut self, frame: FrameToken) -> Result<(), RendererError>;

    /// Get the swapchain / surface extent (primary window size).
    fn swapchain_extent(&self) -> Size2D;

    /// Get the current frame index for double-buffered resources.
    fn current_frame(&self) -> usize;

    /// Number of reusable submission slots, independent of surface image count.
    fn frame_slot_count(&self) -> usize;

    /// Number of swapchain images.
    fn num_images(&self) -> usize;

    /// Block until the GPU is idle.
    fn wait_for_device(&self);

    /// Create a typed, backend-owned buffer that a render graph can import.
    fn create_buffer(&mut self, desc: BufferDesc) -> Result<BufferHandle, RendererError>;

    /// Describe a live buffer allocation without exposing its native representation.
    fn buffer_descriptor(&self, handle: BufferHandle) -> Option<BufferDesc>;

    /// Create and initialize a resource before it is used by a submitted frame.
    ///
    /// Device-local uploads complete before successful return. The allocation
    /// and initial upload are atomic: failure does not register a partial resource.
    fn create_buffer_with_data(
        &mut self,
        desc: BufferDesc,
        data: &[u8],
    ) -> Result<BufferHandle, RendererError>;

    /// Write the acquired slot's CPU-visible buffer allocation.
    ///
    /// The backend validates bounds, CPU access and ownership; the operation must
    /// never overwrite an allocation still read by another submitted frame.
    fn write_buffer(
        &mut self,
        frame: &FrameToken,
        handle: BufferHandle,
        offset: u64,
        data: &[u8],
    ) -> Result<(), RendererError>;

    /// Read CPU-visible result bytes only after their exact last committed consumer retires.
    ///
    /// Returns None while its owner is pending. Rejects stale handles, out-of-bounds
    /// ranges and allocations without the explicit Readback memory policy.
    fn read_buffer_completed(
        &mut self,
        handle: BufferHandle,
        range: crate::render_graph::BufferByteRange,
    ) -> Result<Option<Vec<u8>>, RendererError>;

    /// The latest committed export; aborted acquisitions never replace it.
    fn graph_texture_source(
        &self,
        resource: crate::render_graph::ResourceId,
    ) -> Option<super::texture_readback::GraphTextureSource>;

    /// Queue a copy from an exact retained committed graph image.
    fn queue_texture_readback(
        &mut self,
        source: super::texture_readback::GraphTextureSource,
        region: super::texture_readback::TextureReadbackRegion,
    ) -> Result<super::texture_readback::TextureReadbackTicket, RendererError>;

    /// Poll without waiting; a completed result consumes its queued copy once.
    fn poll_texture_readback(
        &mut self,
        ticket: super::texture_readback::TextureReadbackTicket,
    ) -> Result<Option<super::texture_readback::TextureReadbackData>, RendererError>;

    /// Destroy a buffer after all submitted work using it has completed.
    fn destroy_buffer(&mut self, handle: BufferHandle) -> Result<(), RendererError>;

    /// Destroy all GPU resources. Must be called before dropping.
    fn destroy(&mut self);

    /// Query GPU hardware capabilities and limits.
    fn capabilities(&self) -> &crate::renderer::types::GpuCapabilities;

    /// Report whether this backend implements an optional renderer feature.
    ///
    /// Required operations carry no flag: every backend implements them.
    /// Optional operations declare their [`RendererFeature`] and fail with
    /// `RendererError::UnsupportedFeature` when the backend reports `false`.
    /// Callers choose fallback behavior from this query, never from backend
    /// names. This method itself is required and has no default: a backend
    /// that omits it fails to compile instead of silently misreporting.
    fn supports_feature(&self, feature: RendererFeature) -> bool;

    /// Passive snapshot of observed native submission state; never waits for completion.
    fn capture_submission_snapshot(
        &self,
    ) -> Option<crate::render_graph::capture::CapturedSubmission> {
        None
    }

    /// Create a mesh from typed vertex and index data.
    ///
    /// The vertex type's trusted [`Vertex`](crate::vertex::Vertex) implementation declares the
    /// layout and attribute semantics; the index width comes from
    /// [`MeshIndexElement`](crate::renderer::registry::MeshIndexElement). Topology and static usage are explicit mesh
    /// properties recorded on the mesh. Nothing is guessed from byte
    /// shapes, and validation failures return typed errors before any GPU
    /// upload. Failed creation registers nothing.
    fn create_mesh<T, U>(
        &mut self,
        vertices: &[T],
        indices: &[U],
        topology: PrimitiveTopology,
    ) -> Result<MeshHandle, RendererError>
    where
        T: crate::vertex::Vertex,
        U: crate::renderer::registry::MeshIndexElement;

    /// Report the index format recorded for a mesh, for diagnostics and tests.
    ///
    /// Returns `None` when the handle does not reference a live mesh.
    fn mesh_index_format(&self, _mesh: MeshHandle) -> Option<crate::backend::command::IndexType> {
        None
    }

    /// Report the logical vertex count recorded for a mesh.
    ///
    /// For dynamic meshes this tracks the latest successful update. Returns
    /// `None` when the handle does not reference a live mesh.
    fn mesh_vertex_count(&self, _mesh: MeshHandle) -> Option<u32> {
        None
    }

    /// Report the logical index count recorded for a mesh.
    ///
    /// Draw encoding reads exactly this many indices. Returns `None` when
    /// the handle does not reference a live mesh.
    fn mesh_index_count(&self, _mesh: MeshHandle) -> Option<u32> {
        None
    }

    /// Create a dynamic (CPU-writable) mesh from an explicit descriptor.
    ///
    /// The descriptor carries layout, semantics, topology, and counts; the
    /// blobs are validated against it before upload. Usage is recorded as
    /// [`MeshUsage::Dynamic`](crate::renderer::registry::MeshUsage).
    fn create_mesh_dynamic(
        &mut self,
        descriptor: &crate::renderer::registry::MeshDescriptor,
        vertex_data: &[u8],
        indices: &[u32],
    ) -> Result<MeshHandle, RendererError>;

    /// Update a dynamic mesh with new vertex and index data.
    ///
    /// Backend-neutral contract: the interleaved blob must describe exactly
    /// `vertex_count` vertices of the mesh's recorded stride and every index
    /// must be in range; success publishes one internally consistent mesh
    /// (counts, contents, capacity), shrinking never reallocates, growth
    /// replaces buffers and retires the old natives until their submissions
    /// complete, and failure leaves the previous mesh state intact. An empty
    /// update (`vertex_count == 0`, no indices) makes the mesh draw nothing;
    /// the recorded `u32` index width never changes.
    fn update_mesh_dynamic(
        &mut self,
        mesh: MeshHandle,
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError>;

    /// Create a texture from a descriptor and pixel data.
    ///
    /// Fails with a typed error (invalid descriptor, allocation or upload
    /// failure, bindless exhaustion) instead of substituting a placeholder
    /// or panicking. Failed creation retains nothing.
    fn create_texture(
        &mut self,
        desc: &TextureDescriptor,
        data: &[u8],
    ) -> Result<TextureHandle, RendererError>;

    /// Create a 1×1 solid-color texture.
    ///
    /// Same failure contract as [`GpuRenderer::create_texture`].
    fn create_texture_solid(&mut self, color: [u8; 4]) -> Result<TextureHandle, RendererError>;

    /// Update an existing texture with new pixel data.
    /// The data must match the texture's format and dimensions.
    ///
    /// Optional ([`RendererFeature::TextureInPlaceUpdate`]): the default
    /// fails with `UnsupportedFeature` before touching any state.
    fn update_texture(&mut self, handle: TextureHandle, data: &[u8]) -> Result<(), RendererError> {
        let _ = (handle, data);
        Err(RendererError::UnsupportedFeature(
            "update_texture not implemented for this backend".into(),
        ))
    }

    /// Queue a validated mip/layer/3D region update ([`RendererFeature::TextureSubresourceUpload`]).
    /// Available on Metal;
    /// other backends reject it before touching the texture.
    fn update_texture_region(
        &mut self,
        handle: TextureHandle,
        region: crate::texture::TextureUploadRegion,
        data: &[u8],
    ) -> Result<(), RendererError> {
        let _ = (handle, region, data);
        Err(RendererError::UnsupportedFeature(
            "texture subresource uploads are unavailable on this backend".into(),
        ))
    }

    /// Pending transfer producers, consumed by graph import synchronization before encoding.
    fn pending_texture_uploads(&self) -> Vec<(TextureHandle, crate::texture::TextureUploadRegion)> {
        Vec::new()
    }

    /// Current upload service gauges, when the backend owns a staged upload service.
    fn texture_upload_metrics(&self) -> Option<crate::texture::TextureUploadMetrics> {
        None
    }

    /// Get the bindless slot for a texture handle.
    fn get_bindless_slot(&self, handle: TextureHandle) -> Option<u32>;

    /// Look up which texture occupies a given bindless slot.
    fn get_texture_at_slot(&self, slot: u32) -> Option<TextureHandle>;

    /// Get the bindless index for a texture (returns 0 when unregistered).
    fn get_texture_bindless_index(&self, handle: TextureHandle) -> u32;

    /// Get the default white texture handle.
    fn default_texture(&self) -> TextureHandle;

    /// Compile a shader into a GPU pipeline and return a material handle.
    ///
    /// Takes a backend-neutral [`PipelineDescriptor`]: shader path and entry
    /// points, canonical vertex layout, portable render state, and explicit
    /// native extension points. Backends validate the descriptor before
    /// touching native APIs. See
    /// [`PipelineDescriptor::pbr`] and friends for canonical constructors.
    fn compile_material(
        &mut self,
        descriptor: &PipelineDescriptor,
    ) -> Result<MaterialHandle, RendererError>;

    /// Set texture indices on an existing material.
    fn set_material_textures(
        &mut self,
        material: MaterialHandle,
        textures: crate::renderer::registry::MaterialTextures,
    );

    /// Prepare replacements for materials depending on a shader or included file.
    ///
    /// Canonical paths distinguish equal filenames in separate directories.
    /// Each material keeps its handle and texture bindings. Its reflected
    /// interface and every live pipeline variant swap only after successful
    /// preparation; failures preserve the previous ready material. Submitted
    /// work retains replaced native pipelines until completion.
    /// Returns affected material count, including failed or queued replacements.
    fn recompile_materials_for_shader(&mut self, shader_path: &std::path::Path) -> usize;

    /// Destroy a mesh.
    fn destroy_mesh(&mut self, handle: MeshHandle);

    /// Destroy a material.
    fn destroy_material(&mut self, handle: MaterialHandle);

    /// Destroy a texture.
    fn destroy_texture(&mut self, handle: TextureHandle);

    /// Destroy a skeleton.
    fn destroy_skeleton(&mut self, handle: SkeletonHandle);

    /// Recreate the output surface at the requested physical pixel extent.
    fn resize(&mut self, width: u32, height: u32) -> Result<(), RendererError>;

    /// Create a GPU skeleton buffer for skeletal animation.
    fn create_skeleton(&mut self, joint_count: usize) -> Result<SkeletonHandle, RendererError>;

    /// Import the acquired slot's skeleton storage into a graph without native handles.
    fn skeleton_buffer_handle(
        &mut self,
        frame: &FrameToken,
        skeleton: SkeletonHandle,
    ) -> Result<BufferHandle, RendererError>;

    /// Begin a timestamp query with the given label.
    ///
    /// Debug hook gated by [`RendererFeature::TimestampQueries`]: the default
    /// no-op is the documented semantic for backends without profiling
    /// support.
    fn begin_timestamp(&mut self, _label: &str) {}

    /// End the timestamp query started with the matching label.
    ///
    /// Debug hook gated by [`RendererFeature::TimestampQueries`]: the default
    /// no-op is the documented semantic for backends without profiling
    /// support.
    fn end_timestamp(&mut self, _label: &str) {}

    /// Read all collected timestamp results from the last frame.
    ///
    /// Debug hook gated by [`RendererFeature::TimestampQueries`]: the default
    /// empty result is the documented semantic for backends without profiling
    /// support.
    fn read_timestamps(&self) -> Vec<crate::renderer::types::GpuTimestamp> {
        Vec::new()
    }
}
