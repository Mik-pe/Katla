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
use crate::handle::{MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle};
use crate::renderer::features::RendererFeature;
use crate::renderer::pipeline_descriptor::PipelineDescriptor;
use crate::renderer::pipeline_kind::PipelineKind;
use crate::renderer::registry::PrimitiveTopology;
use crate::renderer::types::{DrawList, FrameUniforms, PointLightGPU, UIDrawList};
use crate::texture::TextureDescriptor;
use crate::viewport::{Viewport, ViewportBuilder, ViewportHandle};

/// Backend-agnostic renderer interface.
///
/// Covers resource creation (meshes, textures, materials, skeletons, viewports),
/// frame lifecycle, and teardown. All method signatures use Katla-native types
/// only — no `vk::`, `ash::`, or Metal types appear in the trait.
///
/// # Canonical Frame Lifecycle
///
/// The recommended frame order (as used by `katla_app`):
///
/// 1. `wait_for_frame()` — ensure GPU is done with this frame slot
/// 2. `set_frame_uniforms()` — write camera/lighting data to storage buffer
/// 3. `execute_draw_calls()` — write per-object data to storage buffer
/// 4. Render graph `render()` — submit GPU work via `FrameGraph<B>`
/// 5. (implicit present) — swapchain present is handled by the render graph
///
/// **Vulkan** follows this order directly. `render_frame()` is a no-op because
/// all rendering goes through `VulkanRenderer::render()` with `FrameGraph`.
///
/// **Metal** currently uses `render_frame()` for its hardcoded pass sequence.
/// Once migrated to the shared frame graph, `render_frame()` will become a
/// no-op on Metal as well and can be removed from the trait.
pub trait GpuRenderer: Sized + 'static {
    // ========================================================================
    // Initialization & Queries
    // ========================================================================

    /// Get the swapchain / surface extent (primary window size).
    fn swapchain_extent(&self) -> Size2D;

    /// Get the current frame index for double-buffered resources.
    fn current_frame(&self) -> usize;

    /// Number of swapchain images.
    fn num_images(&self) -> usize;

    /// Block until the GPU is idle.
    fn wait_for_device(&self);

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

    // ========================================================================
    // Frame Lifecycle
    // ========================================================================

    /// Wait for the previous frame's GPU work to complete.
    fn wait_for_frame(&mut self) -> Result<(), RendererError>;

    /// Set per-frame uniforms (camera, lighting).
    fn set_frame_uniforms(&mut self, uniforms: FrameUniforms);

    /// Write draw call data into the GPU storage buffer.
    fn execute_draw_calls(&mut self, draw_list: &DrawList) -> Result<(), RendererError>;

    /// Convenience: set uniforms + write draw calls, return the DrawList.
    fn draw(
        &mut self,
        uniforms: &FrameUniforms,
        draw_calls: &[crate::renderer::types::DrawCall],
    ) -> Result<DrawList, RendererError>;

    /// Get the current frame uniforms.
    fn frame_uniforms(&self) -> &FrameUniforms;

    /// Begin the frame (acquire next image, etc.).
    ///
    /// **Vulkan**: Delegates to `wait_for_frame()`, returns the current frame index.
    /// The actual swapchain image acquisition happens inside `render()`.
    ///
    /// **Metal**: Acquires the next drawable from the Metal layer. Returns the
    /// frame index.
    fn begin_frame(&mut self) -> Result<u32, RendererError>;

    /// End the frame (submit, present).
    ///
    /// **Vulkan**: No-op. Presentation happens inside `render()`.
    ///
    /// **Metal**: Releases the current drawable reference and increments the
    /// frame index. The drawable is presented by the Metal command buffer.
    fn end_frame(&mut self) -> Result<(), RendererError>;

    // ========================================================================
    // Mesh Creation
    // ========================================================================

    /// Create a mesh from typed vertex and index data.
    ///
    /// The vertex type's trusted [`Vertex`] implementation declares the
    /// layout and attribute semantics; the index width comes from
    /// [`MeshIndexElement`]. Topology and static usage are explicit mesh
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

    // ========================================================================
    // Texture Creation & Queries
    // ========================================================================

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

    /// Get the bindless slot for a texture handle.
    fn get_bindless_slot(&self, handle: TextureHandle) -> Option<u32>;

    /// Look up which texture occupies a given bindless slot.
    fn get_texture_at_slot(&self, slot: u32) -> Option<TextureHandle>;

    /// Get the bindless index for a texture (returns 0 when unregistered).
    fn get_texture_bindless_index(&self, handle: TextureHandle) -> u32;

    /// Get the default white texture handle.
    fn default_texture(&self) -> TextureHandle;

    // ========================================================================
    // Material Creation
    // ========================================================================

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
    fn set_material_texture_indices(&mut self, material: MaterialHandle, indices: [u32; 4]);

    /// Set the default PBR material handle (called once during init).
    fn set_default_material(&mut self, material: MaterialHandle);

    /// Get the default PBR material handle.
    fn default_material(&self) -> MaterialHandle;

    /// Recompile all materials compiled from the given shader file.
    ///
    /// Invalidates cached shader modules, re-reads the shader from disk,
    /// and rebuilds pipelines for each matching material in-place (keeping
    /// the same handle). Returns the number of materials recompiled.
    /// Required: every backend implements this explicitly. A backend with no
    /// recompilation support returns 0 from its own implementation rather
    /// than inheriting silence.
    fn recompile_materials_for_shader(&mut self, shader_path: &std::path::Path) -> usize;

    // ========================================================================
    // Destruction
    // ========================================================================

    /// Destroy a mesh.
    fn destroy_mesh(&mut self, handle: MeshHandle);

    /// Destroy a material.
    fn destroy_material(&mut self, handle: MaterialHandle);

    /// Destroy a texture.
    fn destroy_texture(&mut self, handle: TextureHandle);

    /// Destroy a skeleton.
    fn destroy_skeleton(&mut self, handle: SkeletonHandle);

    // ========================================================================
    // Viewport
    // ========================================================================

    /// Begin building a new viewport.
    fn create_viewport(&mut self) -> ViewportBuilder;

    /// Number of active viewports.
    fn viewport_count(&self) -> usize;

    /// Look up a viewport by handle.
    fn get_viewport(&self, handle: ViewportHandle) -> Option<&Viewport>;

    /// Look up a viewport extent by handle.
    fn viewport_extent(&self, handle: ViewportHandle) -> Option<Size2D>;

    /// Destroy a viewport.
    fn destroy_viewport(&mut self, handle: ViewportHandle);

    // ========================================================================
    // Frame Graph
    // ========================================================================

    /// Recreate swapchain after resize. Returns updated texture names and slots.
    fn resize(&mut self, width: u32, height: u32) -> Result<(), RendererError>;

    /// Recreate the 3D-scene render targets (depth, HDR, picking) at the given
    /// size, independent of the swapchain. Under the editor the scene is
    /// composed for the viewport panel's aspect ratio, so its render targets
    /// must be sized to the panel — not the window — to avoid stretching the
    /// scene across the full drawable and then cropping.
    ///
    /// Required with no default. A backend whose scene targets are
    /// frame-graph transients (sized via the frame graph) implements this as
    /// an explicit documented no-op instead of inheriting silence.
    fn recreate_scene_render_targets(&mut self, width: u32, height: u32);

    // ========================================================================
    // Lighting
    // ========================================================================

    /// Upload point light data for Forward+ tile-based culling.
    ///
    /// Required: every backend implements this explicitly, even if only to
    /// record that light upload is owned elsewhere.
    fn upload_lights(&mut self, lights: &[PointLightGPU]);

    // ========================================================================
    // Shadows
    // ========================================================================

    /// Update shadow cascade view-projection matrices from light direction.
    ///
    /// Required: every backend implements this explicitly.
    fn update_shadows(&mut self, light_direction: [f32; 3]);

    /// Upload shadow cascade data to GPU for the current frame.
    ///
    /// Required: every backend implements this explicitly.
    fn upload_shadow_cascades(&mut self);

    /// Get the base bindless index for per-frame depth textures.
    /// Actual index for frame N is `base + N`. Returns `None` if not registered.
    fn depth_texture_base_index(&self) -> Option<u32> {
        None
    }

    /// Get the bindless slot index of the offscreen viewport texture.
    /// The editor UI uses this to display the 3D scene in the viewport panel.
    fn viewport_bindless_index(&self) -> Option<u32> {
        None
    }

    /// Register per-frame depth textures with the bindless system.
    /// Returns the base bindless slot index.
    ///
    /// Optional ([`RendererFeature::DepthBindlessRegistration`]): the default
    /// fails with `UnsupportedFeature` before touching any state.
    fn register_depth_textures_bindless(&mut self) -> Result<u32, RendererError> {
        Err(RendererError::UnsupportedFeature(
            "register_depth_textures_bindless not supported".into(),
        ))
    }

    /// Get the bindless slot index of the HDR geometry render target.
    /// Used by the tonemapping shader to sample the HDR scene.
    fn geometry_hdr_bindless_index(&self) -> Option<u32> {
        None
    }

    // ========================================================================
    // Animation
    // ========================================================================

    /// Initialize the GPU animation compute pipeline.
    /// `shader_path` is an absolute or relative path to the WGSL shader.
    ///
    /// Optional ([`RendererFeature::AnimationCompute`]): the default fails
    /// with `UnsupportedFeature` before touching any state.
    fn init_animation_pipeline(
        &mut self,
        _shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        Err(RendererError::UnsupportedFeature(
            "init_animation_pipeline not implemented for this backend".into(),
        ))
    }

    // ========================================================================
    // Pipeline Initialization
    // ========================================================================

    /// Initialize Forward+ light culling for the given output size.
    ///
    /// Optional ([`RendererFeature::LightCulling`]): the default fails with
    /// `UnsupportedFeature` before touching any state.
    fn init_light_culling(
        &mut self,
        _width: u32,
        _height: u32,
        _shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        Err(RendererError::UnsupportedFeature(
            "init_light_culling not implemented for this backend".into(),
        ))
    }

    /// Initialize shadow-map resources.
    ///
    /// Optional ([`RendererFeature::ShadowMaps`]): the default fails with
    /// `UnsupportedFeature` before touching any state.
    fn init_shadow_resources(&mut self) -> Result<(), RendererError> {
        Err(RendererError::UnsupportedFeature(
            "init_shadow_resources not implemented for this backend".into(),
        ))
    }

    /// Initialize a GPU pipeline by kind.
    ///
    /// Consolidates the individual `init_*_pipeline` methods into a single entry
    /// point. The `shader_paths` slice length depends on the kind:
    ///
    /// - **1 path**: Shadow, ShadowSkinned, DepthPrepass, DepthPrepassSkinned,
    ///   DepthPrepassBillboard, Picking, PickingSkinned, Sky, Tonemap
    /// - **2 paths**: StencilIndicator (base + skinned)
    /// - **4 paths**: Outline (stencil_mark + stencil_mark_skinned + outline_draw + outline_draw_skinned)
    ///
    /// Optional ([`RendererFeature::PassPipelines`]): the default fails with
    /// `UnsupportedFeature` before touching any state.
    fn init_pass_pipeline(
        &mut self,
        _kind: PipelineKind,
        _shader_paths: &[&std::path::Path],
    ) -> Result<(), RendererError> {
        Err(RendererError::UnsupportedFeature(
            "init_pass_pipeline not implemented for this backend".into(),
        ))
    }

    /// Store the bindless slot of the viewport texture for UI composition.
    ///
    /// Required with no default. A backend that resolves the viewport texture
    /// through graph bindings instead of a stored slot implements this as an
    /// explicit documented no-op.
    fn set_viewport_bindless_slot(&mut self, slot: u32);

    // ========================================================================
    // UI Rendering
    // ========================================================================

    /// Set the UI material handle for backends that render UI directly (Metal).
    ///
    /// Required with no default. A backend that renders UI through the frame
    /// graph instead of a direct pass implements this as an explicit
    /// documented no-op.
    fn set_ui_material(&mut self, material: MaterialHandle);

    /// Queue a UI draw list for rendering in the next frame.
    ///
    /// Required with no default. Gated by
    /// [`RendererFeature::DirectUiPass`]: backends that render UI through the
    /// frame graph implement this as an explicit documented no-op and report
    /// the feature as unsupported.
    fn render_ui_pass(&mut self, draw_list: UIDrawList);

    // ========================================================================
    // Skeleton
    // ========================================================================

    /// Create a GPU skeleton buffer for skeletal animation.
    fn create_skeleton(&mut self, joint_count: usize) -> Result<SkeletonHandle, RendererError>;

    /// Upload joint matrices to a skeleton.
    fn update_skeleton(&mut self, handle: SkeletonHandle, matrices: &[[f32; 16]]);

    // ========================================================================
    // Particles (optional — Metal may return errors/no-ops initially)
    // ========================================================================

    /// Initialize the global particle system.
    fn init_particle_system(&mut self) -> Result<(), RendererError>;

    // ========================================================================
    // Font Atlas
    // ========================================================================

    /// Create or replace the UI font atlas texture.
    ///
    /// Fails with a typed error instead of installing a placeholder.
    /// Failed creation changes nothing.
    fn create_ui_font_atlas(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Result<TextureHandle, RendererError>;

    /// Update the existing font atlas texture in-place.
    fn update_ui_font_atlas(&mut self, width: u32, height: u32, data: &[u8]);

    /// Get the font atlas texture handle, if one has been created.
    fn ui_font_atlas_handle(&self) -> Option<TextureHandle>;

    // ========================================================================
    // GPU Timestamp Queries (Profiling)
    // ========================================================================

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

    // ========================================================================
    // Viewport Panel Rect
    // ========================================================================

    /// Set the viewport panel bounds in physical pixel coordinates.
    /// When Some, the 3D scene is restricted to this rect. When None, full-screen.
    ///
    /// Required with no default. A backend that sizes its scene targets
    /// through [`GpuRenderer::recreate_scene_render_targets`] instead of a
    /// per-frame rect implements this as an explicit documented no-op.
    fn set_viewport_panel_rect(&mut self, rect: Option<crate::rect::Rect>);
}

// ---------------------------------------------------------------------------
// VulkanRenderer impl — delegates to existing methods.
// Feature-gated behind vulkan since VulkanRenderer is vulkan-only.
// ---------------------------------------------------------------------------

use crate::renderer::VulkanRenderer;

impl GpuRenderer for VulkanRenderer {
    fn swapchain_extent(&self) -> Size2D {
        VulkanRenderer::swapchain_extent(self)
    }

    fn current_frame(&self) -> usize {
        VulkanRenderer::current_frame(self)
    }

    fn num_images(&self) -> usize {
        VulkanRenderer::num_images(self)
    }

    fn wait_for_device(&self) {
        VulkanRenderer::wait_for_device(self);
    }

    fn destroy(&mut self) {
        VulkanRenderer::destroy(self);
    }

    fn capabilities(&self) -> &crate::renderer::types::GpuCapabilities {
        &self.capabilities
    }

    fn supports_feature(&self, feature: crate::renderer::features::RendererFeature) -> bool {
        use crate::renderer::features::RendererFeature;
        match feature {
            // Vulkan renders UI through the frame graph (`frame.submit_ui()`),
            // not through a direct queued UI pass.
            RendererFeature::DirectUiPass => false,
            RendererFeature::AnimationCompute
            | RendererFeature::LightCulling
            | RendererFeature::PassPipelines
            | RendererFeature::ShadowMaps
            | RendererFeature::ParticleSystem
            | RendererFeature::TimestampQueries
            | RendererFeature::TextureInPlaceUpdate
            | RendererFeature::DepthBindlessRegistration => true,
        }
    }

    fn wait_for_frame(&mut self) -> Result<(), RendererError> {
        VulkanRenderer::wait_for_frame(self)
    }

    fn set_frame_uniforms(&mut self, uniforms: FrameUniforms) {
        VulkanRenderer::set_frame_uniforms(self, uniforms);
    }

    fn execute_draw_calls(&mut self, draw_list: &DrawList) -> Result<(), RendererError> {
        VulkanRenderer::execute_draw_calls(self, draw_list)
    }

    fn draw(
        &mut self,
        uniforms: &FrameUniforms,
        draw_calls: &[crate::renderer::types::DrawCall],
    ) -> Result<DrawList, RendererError> {
        VulkanRenderer::draw(self, uniforms, draw_calls)
    }

    fn frame_uniforms(&self) -> &FrameUniforms {
        VulkanRenderer::frame_uniforms(self)
    }

    fn begin_frame(&mut self) -> Result<u32, RendererError> {
        VulkanRenderer::wait_for_frame(self)?;
        Ok(self.current_frame() as u32)
    }

    fn end_frame(&mut self) -> Result<(), RendererError> {
        Ok(())
    }

    fn create_mesh<T, U>(
        &mut self,
        vertices: &[T],
        indices: &[U],
        topology: PrimitiveTopology,
    ) -> Result<MeshHandle, RendererError>
    where
        T: crate::vertex::Vertex,
        U: crate::renderer::registry::MeshIndexElement,
    {
        VulkanRenderer::create_mesh(self, vertices, indices, topology)
    }

    fn mesh_index_format(&self, mesh: MeshHandle) -> Option<crate::backend::command::IndexType> {
        VulkanRenderer::mesh_index_format(self, mesh)
    }

    fn mesh_vertex_count(&self, mesh: MeshHandle) -> Option<u32> {
        VulkanRenderer::mesh_vertex_count(self, mesh)
    }

    fn mesh_index_count(&self, mesh: MeshHandle) -> Option<u32> {
        VulkanRenderer::mesh_index_count(self, mesh)
    }

    fn create_mesh_dynamic(
        &mut self,
        descriptor: &crate::renderer::registry::MeshDescriptor,
        vertex_data: &[u8],
        indices: &[u32],
    ) -> Result<MeshHandle, RendererError> {
        VulkanRenderer::create_mesh_dynamic(self, descriptor, vertex_data, indices)
    }

    fn update_mesh_dynamic(
        &mut self,
        mesh: MeshHandle,
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError> {
        VulkanRenderer::update_mesh_dynamic(self, mesh, vertex_data, vertex_count, indices)
    }

    fn create_texture(
        &mut self,
        desc: &TextureDescriptor,
        data: &[u8],
    ) -> Result<TextureHandle, RendererError> {
        VulkanRenderer::create_texture(self, desc, data)
    }

    fn create_texture_solid(&mut self, color: [u8; 4]) -> Result<TextureHandle, RendererError> {
        VulkanRenderer::create_texture_solid(self, color)
    }

    fn update_texture(&mut self, handle: TextureHandle, data: &[u8]) -> Result<(), RendererError> {
        let texture =
            self.texture_manager
                .get_texture(handle)
                .ok_or_else(|| RendererError::StaleHandle {
                    resource: "texture".to_string(),
                    detail: format!("{handle:?} in update_texture"),
                })?;
        texture.update_data(data)
    }

    fn get_bindless_slot(&self, handle: TextureHandle) -> Option<u32> {
        VulkanRenderer::get_bindless_slot(self, handle)
    }

    fn get_texture_at_slot(&self, slot: u32) -> Option<TextureHandle> {
        VulkanRenderer::get_texture_at_slot(self, slot)
    }

    fn get_texture_bindless_index(&self, handle: TextureHandle) -> u32 {
        VulkanRenderer::get_texture_bindless_index(self, handle)
    }

    fn default_texture(&self) -> TextureHandle {
        VulkanRenderer::default_texture(self)
    }

    fn recreate_scene_render_targets(&mut self, width: u32, height: u32) {
        if let Err(error) = unsafe { self.context.device.device_wait_idle() } {
            log::error!("Failed to wait before resizing scene targets: {error}");
            return;
        }
        self.frame_context
            .resize_scene_depth(ash::vk::Extent2D { width, height });
        if let Some(base) = self.depth_texture_base_index {
            for (frame, texture) in self.frame_context.depth_render_textures.iter().enumerate() {
                if let Err(error) = self
                    .bindless_manager
                    .update_texture(base + frame as u32, texture.image_view.vk())
                {
                    log::error!("Failed to refresh scene depth binding: {error}");
                }
            }
        }
        self.resize_light_culling(width, height);
    }

    fn compile_material(
        &mut self,
        descriptor: &PipelineDescriptor,
    ) -> Result<MaterialHandle, RendererError> {
        use crate::renderer::pipeline_descriptor::{BlendMode, PipelineStages};
        use crate::vertex::VertexLayout;
        use crate::vulkan::material::compiler::{MaterialOptions, VertexType};

        descriptor.validate()?;
        if !descriptor.specialization.is_empty() {
            return Err(RendererError::UnsupportedFeature(
                "specialization constants are not yet plumbed into the Vulkan material compiler"
                    .to_string(),
            ));
        }
        let PipelineStages::Graphics { .. } = &descriptor.stages else {
            return Err(RendererError::UnsupportedFeature(
                "compute pipelines are not yet supported by compile_material".to_string(),
            ));
        };

        // Derive the internal routing from canonical layout identity.
        // No strings, no silent fallback: unknown layouts fail loudly.
        let vertex_type = if descriptor.vertex == VertexLayout::pbr() {
            VertexType::Pbr
        } else if descriptor.vertex == VertexLayout::ui() {
            VertexType::Ui
        } else if descriptor.vertex == VertexLayout::position() {
            VertexType::Simple
        } else if descriptor.vertex == VertexLayout::pbr_skinned() {
            VertexType::Skinned
        } else {
            return Err(RendererError::InvalidDescriptor {
                resource: "material".to_string(),
                reason: format!(
                    "unknown vertex layout ({} attributes, stride {}): \
                     Vulkan material compilation supports the canonical \
                     PBR / UI / position / skinned layouts",
                    descriptor.vertex.len(),
                    descriptor.vertex.stride(),
                ),
            });
        };

        // Depth state maps straight through; the compiler normalises
        // test=false to (false, false, Always) for every entry path.
        let (vertex_entry, fragment_entry) = match &descriptor.stages {
            PipelineStages::Graphics {
                vertex_entry,
                fragment_entry,
            } => (vertex_entry.clone(), fragment_entry.clone()),
            PipelineStages::Compute { .. } => unreachable!("rejected above"),
        };
        let options = MaterialOptions {
            alpha_blended: matches!(descriptor.blend, BlendMode::AlphaBlend),
            double_sided: matches!(descriptor.cull, crate::pipeline::CullMode::None),
            wireframe: descriptor.wireframe,
            vertex_type,
            color_format: descriptor.color_format,
            is_compositing: descriptor.native.vulkan.compositing,
            depth_test: descriptor.depth.test,
            depth_write: descriptor.depth.write,
            depth_compare: descriptor.depth.compare,
            vertex_entry,
            fragment_entry,
        };
        VulkanRenderer::compile_material(self, &descriptor.shader_path, options)
    }

    fn set_material_texture_indices(&mut self, material: MaterialHandle, indices: [u32; 4]) {
        VulkanRenderer::set_material_texture_indices(self, material, indices);
    }

    fn set_default_material(&mut self, material: MaterialHandle) {
        self.default_material_handle = Some(material);
    }

    fn default_material(&self) -> MaterialHandle {
        VulkanRenderer::default_material(self)
    }

    fn recompile_materials_for_shader(&mut self, shader_path: &std::path::Path) -> usize {
        VulkanRenderer::recompile_materials_for_shader(self, shader_path)
    }

    fn destroy_mesh(&mut self, handle: MeshHandle) {
        VulkanRenderer::destroy_mesh(self, handle);
    }

    fn destroy_material(&mut self, handle: MaterialHandle) {
        VulkanRenderer::destroy_material(self, handle);
    }

    fn destroy_texture(&mut self, handle: TextureHandle) {
        VulkanRenderer::destroy_texture(self, handle);
    }

    fn destroy_skeleton(&mut self, handle: SkeletonHandle) {
        VulkanRenderer::destroy_skeleton(self, handle);
    }

    fn create_viewport(&mut self) -> ViewportBuilder {
        VulkanRenderer::create_viewport(self)
    }

    fn viewport_count(&self) -> usize {
        VulkanRenderer::viewport_count(self)
    }

    fn get_viewport(&self, handle: ViewportHandle) -> Option<&Viewport> {
        VulkanRenderer::get_viewport(self, handle)
    }

    fn viewport_extent(&self, handle: ViewportHandle) -> Option<Size2D> {
        VulkanRenderer::viewport_extent(self, handle)
    }

    fn destroy_viewport(&mut self, handle: ViewportHandle) {
        VulkanRenderer::destroy_viewport(self, handle);
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), RendererError> {
        VulkanRenderer::recreate_swapchain(self, Size2D::new(width, height))
    }

    fn create_skeleton(&mut self, joint_count: usize) -> Result<SkeletonHandle, RendererError> {
        VulkanRenderer::create_skeleton(self, joint_count)
    }

    fn update_skeleton(&mut self, handle: SkeletonHandle, matrices: &[[f32; 16]]) {
        VulkanRenderer::update_skeleton(self, handle, matrices);
    }

    fn init_particle_system(&mut self) -> Result<(), RendererError> {
        VulkanRenderer::init_particle_system(self)
    }

    fn create_ui_font_atlas(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Result<TextureHandle, RendererError> {
        VulkanRenderer::create_ui_font_atlas(self, width, height, data)
    }

    fn update_ui_font_atlas(&mut self, width: u32, height: u32, data: &[u8]) {
        VulkanRenderer::update_ui_font_atlas(self, width, height, data);
    }

    fn ui_font_atlas_handle(&self) -> Option<TextureHandle> {
        self.ui_renderer.font_atlas()
    }

    // -- Lighting --

    fn upload_lights(&mut self, lights: &[PointLightGPU]) {
        VulkanRenderer::upload_lights(self, lights);
    }

    // -- Shadows --

    fn update_shadows(&mut self, light_direction: [f32; 3]) {
        VulkanRenderer::update_shadows(self, light_direction);
    }

    fn upload_shadow_cascades(&mut self) {
        VulkanRenderer::upload_shadow_cascades(self);
    }

    fn depth_texture_base_index(&self) -> Option<u32> {
        VulkanRenderer::depth_texture_base_index(self)
    }

    fn register_depth_textures_bindless(&mut self) -> Result<u32, RendererError> {
        VulkanRenderer::register_depth_textures_bindless(self)
    }

    // -- Animation --

    fn init_animation_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        VulkanRenderer::init_animation_pipeline(self, shader_path)
    }

    // -- UI Rendering --

    fn set_ui_material(&mut self, _material: MaterialHandle) {
        // Vulkan renders UI through the frame graph via frame.submit_ui();
        // there is no stored direct-pass UI material.
    }

    fn render_ui_pass(&mut self, _draw_list: UIDrawList) {
        // Vulkan renders UI through the frame graph via frame.submit_ui(),
        // not through a direct render_ui_pass call.
    }

    fn set_viewport_panel_rect(&mut self, _rect: Option<crate::rect::Rect>) {
        // Vulkan sizes its 3D-scene targets through
        // recreate_scene_render_targets; no per-frame panel rect is stored.
    }

    fn set_viewport_bindless_slot(&mut self, _slot: u32) {
        // Vulkan resolves the viewport texture through frame-graph bindings,
        // not through a stored bindless slot.
    }

    // -- Pipeline Initialization --

    fn init_light_culling(
        &mut self,
        width: u32,
        height: u32,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        VulkanRenderer::init_light_culling(self, width, height, shader_path)
    }

    fn init_shadow_resources(&mut self) -> Result<(), RendererError> {
        VulkanRenderer::init_shadow_resources(self, None, crate::shadow::CascadeParams::default())
    }

    fn init_pass_pipeline(
        &mut self,
        kind: PipelineKind,
        shader_paths: &[&std::path::Path],
    ) -> Result<(), RendererError> {
        match kind {
            PipelineKind::Shadow => VulkanRenderer::init_shadow_pipeline(self, shader_paths[0]),
            PipelineKind::ShadowSkinned => {
                VulkanRenderer::init_shadow_pipeline_skinned(self, shader_paths[0])
            }
            PipelineKind::DepthPrepass => {
                VulkanRenderer::init_depth_prepass_pipeline(self, shader_paths[0])
            }
            PipelineKind::DepthPrepassSkinned => {
                VulkanRenderer::init_depth_prepass_skinned_pipeline(self, shader_paths[0])
            }
            PipelineKind::DepthPrepassBillboard => {
                VulkanRenderer::init_depth_prepass_billboard_pipeline(self, shader_paths[0])
            }
            PipelineKind::Outline => VulkanRenderer::init_outline_pipelines(
                self,
                shader_paths[0],
                shader_paths[1],
                shader_paths[2],
                shader_paths[3],
            ),
            PipelineKind::StencilIndicator => VulkanRenderer::init_stencil_indicator_pipelines(
                self,
                shader_paths[0],
                shader_paths[1],
            ),
            PipelineKind::Picking
            | PipelineKind::PickingSkinned
            | PipelineKind::Sky
            | PipelineKind::Tonemap => Ok(()),
        }
    }

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
        // Need &mut to read results; use get_mut pattern through interior mutability
        // Since this is called on &self, we return cached results
        if let Some(ref tq) = self.timestamp_queries {
            tq.cached_results()
        } else {
            Vec::new()
        }
    }
}
