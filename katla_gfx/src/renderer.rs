//! Vulkan renderer implementation modules.
//!
//! This module organizes VulkanRenderer methods into logical groups:
//!
//! - `frame` - Frame rendering and swapchain management
//! - `viewport` - Viewport system management (TODO: extract from lib.rs)
//! - `ui` - UI buffer and texture management (TODO: extract from lib.rs)

pub mod mesh_manager;
pub mod registry;
pub mod types;
pub mod viewport_manager;

pub use crate::handle::{Handle, MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle};
use crate::viewport::{ViewportBuilder, ViewportHandle};
pub use registry::AssetRegistry;
pub use types::{
    DrawCall, DrawList, FrameUniforms, InstanceData, ParticleDispatch, ParticleRender, UIDrawList,
    UiDrawCommand,
};

use crate::material::Material;
use crate::texture::{TextureDescriptor, TextureManager};
use crate::vulkan::context::VulkanContext;
use crate::{
    BindlessTextureManager, IndexBuffer, MAX_BINDLESS_TEXTURES, RendererError,
    SkeletonDescriptorSet, StorageDescriptorSet, StorageUniformManager, SwapData, VertexBuffer,
    VulkanFrameCtx, viewport::Viewport,
};
use ash::vk;
use log::{error, info};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{cell::RefCell, ffi::CString, rc::Rc};

use crate::MaterialBuilder;
use crate::barrier::ImageBarrier;
use crate::sync::{COLOR_SUBRESOURCE_RANGE, DEPTH_SUBRESOURCE_RANGE};
use crate::vulkan::material::compiler::MaterialCompiler;

/// Transpose a 4x4 matrix from row-major to column-major format.
///
pub struct VulkanRenderer {
    pub(crate) context: Rc<VulkanContext>,
    pub(crate) frame_context: VulkanFrameCtx,
    pub(crate) swap_data: SwapData,
    /// Mesh manager for mesh creation and storage.
    pub(crate) mesh_manager: mesh_manager::MeshManager,
    /// Asset registry for managing GPU resources (materials).
    /// This stores the actual pipelines, while the application
    /// only holds opaque handles (MaterialHandle).
    pub asset_registry: AssetRegistry,
    /// Bindless texture manager for efficient texture binding.
    /// All textures are stored in a single array accessed by index.
    /// Texture indices are passed via ObjectUniforms.texture_indices.
    pub(crate) bindless_manager: BindlessTextureManager,
    /// Centralized texture manager for handle-based texture creation.
    /// Provides a clean API for creating and looking up textures by handle.
    pub(crate) texture_manager: TextureManager,
    /// Storage uniform manager for storage buffer-based uniforms.
    /// Materials use storage buffers with instance indexing.
    pub(crate) storage_manager: StorageUniformManager,
    /// Storage descriptor set for binding frame and object uniforms.
    /// Contains the storage buffer bound at two offsets (frame_data at 0, objects at 256).
    pub(crate) storage_descriptor_set: StorageDescriptorSet,
    /// Draw list cell for geometry pass (shared with render graph).
    pub(crate) draw_list_cell: Rc<RefCell<Option<DrawList>>>,
    /// Skeleton descriptor sets for GPU skeletal animation.
    /// Indexed by SkeletonHandle.
    pub(crate) skeleton_descriptors: Vec<Option<SkeletonDescriptorSet>>,
    /// Frame-level uniforms set once per frame via set_frame_uniforms().
    pub(crate) frame_uniforms: FrameUniforms,
    /// Cached default white PBR material handle.
    default_material_handle: Option<MaterialHandle>,
    /// Offscreen render targets as (texture_id, target) pairs.
    /// Simple Vec since we only have a few targets (viewport + preview).
    /// - TextureId 2 = viewport
    /// - TextureId 101 = preview
    render_targets: Vec<(u64, ViewportRenderTarget)>,
    /// Output render target for final composition (UI renders here, then present_pass copies to swapchain).
    output_target: Option<OutputRenderTarget>,
    /// Viewport manager for viewport and render target management.
    pub(crate) viewport_manager: viewport_manager::ViewportManager,
    /// Cached UI render state (lazy initialized).
    ui_state: Option<UiRenderState>,
    /// Pending UI data for next frame (set by render_ui, consumed by render_frame).
    pending_ui: Option<UiFrameData>,
    /// Material compiler for compiling materials from shaders.
    pub(crate) material_compiler: MaterialCompiler,
}

/// Cached UI mesh and material handles.
struct UiRenderState {
    mesh: MeshHandle,
    material: MaterialHandle,
}

/// Pending UI frame data passed from render_ui() to render_frame().
struct UiFrameData {
    vertex_bytes: Vec<u8>,
    vertex_count: u32,
    indices: Vec<u32>,
    commands: Vec<UiDrawCommand>,
    screen_size: [f32; 2],
}

/// Number of frames that can be processed concurrently.
/// This is an implementation detail for double-buffering.
pub(crate) const FRAMES_IN_FLIGHT: usize = 2;

impl VulkanRenderer {
    pub fn init(
        display: &dyn HasDisplayHandle,
        window: &dyn HasWindowHandle,
        with_validation_layers: bool,
        app_name: CString,
        engine_name: CString,
    ) -> Result<Self, RendererError> {
        let context = Rc::new(VulkanContext::init(
            display,
            window,
            with_validation_layers,
            app_name,
            engine_name,
        ));

        // Set up validation logging at appropriate log levels
        if with_validation_layers {
            context.setup_validation_logging();
        }

        let frame_context = VulkanFrameCtx::init(&context);

        let swapchain_images_raw: Vec<vk::Image> = frame_context
            .swapchain_images
            .iter()
            .map(|img| img.vk())
            .collect();
        let swap_data = SwapData::new(&context.device, &swapchain_images_raw, FRAMES_IN_FLIGHT);

        // Initialize bindless texture manager
        let bindless_manager = BindlessTextureManager::new(context.clone()).map_err(|e| {
            error!("Failed to create bindless texture manager: {:?}", e);
            RendererError::InitializationFailed(
                "Failed to create bindless texture manager".to_string(),
            )
        })?;
        info!(
            "Bindless texture system initialized (max {} textures)",
            MAX_BINDLESS_TEXTURES
        );

        // Initialize texture manager
        let texture_manager = TextureManager::new(context.clone()).map_err(|e| {
            error!("Failed to create texture manager: {:?}", e);
            RendererError::InitializationFailed("Failed to create texture manager".to_string())
        })?;
        info!("Texture manager initialized");

        // Initialize storage uniform system with standard layout
        let storage_manager = StorageUniformManager::new(context.clone())?;

        // Create storage descriptor set for binding frame and object uniforms
        let storage_descriptor_set = StorageDescriptorSet::new(
            &context,
            storage_manager.buffer(),
            storage_manager.buffer_size(),
        )
        .map_err(|e| {
            error!("Failed to create storage descriptor set: {:?}", e);
            RendererError::InitializationFailed(
                "Failed to create storage descriptor set".to_string(),
            )
        })?;

        // Initialize mesh manager
        let mesh_manager = mesh_manager::MeshManager::new(context.clone());

        // Initialize viewport manager
        let viewport_manager = viewport_manager::ViewportManager::new(context.clone());

        // Initialize material compiler
        let material_compiler =
            MaterialCompiler::new(context.clone(), &bindless_manager, &storage_descriptor_set)
                .map_err(|e| {
                    error!("Failed to create material compiler: {:?}", e);
                    RendererError::InitializationFailed(
                        "Failed to create material compiler".to_string(),
                    )
                })?;
        info!("Material compiler initialized");

        Ok(Self {
            context: context.clone(),
            frame_context,
            swap_data,
            mesh_manager,
            asset_registry: AssetRegistry::new(),
            bindless_manager,
            texture_manager,
            storage_manager,
            storage_descriptor_set,
            draw_list_cell: Rc::new(RefCell::new(None)),
            skeleton_descriptors: Vec::new(),
            frame_uniforms: FrameUniforms::default(),
            default_material_handle: None,
            render_targets: Vec::new(),
            output_target: None,
            viewport_manager,
            ui_state: None,
            pending_ui: None,
            material_compiler,
        })
    }

    /// Get the Vulkan context.
    ///
    /// This provides access to low-level Vulkan resources. Most operations should use
    /// higher-level VulkanRenderer methods instead.
    ///
    /// # Safety
    ///
    /// The caller must ensure proper synchronization when using the context.
    pub fn context(&self) -> &Rc<VulkanContext> {
        &self.context
    }

    // ========================================================================
    // Texture Creation API
    // ========================================================================

    /// Create a texture from a descriptor and pixel data.
    ///
    /// This is the primary method for texture creation. The texture is
    /// automatically registered with the bindless system.
    ///
    /// # Arguments
    /// * `desc` - Texture descriptor specifying dimensions, format, and usage
    /// * `data` - Pixel data (must match descriptor dimensions and format)
    ///
    /// # Returns
    /// A TextureHandle for the created texture.
    ///
    /// # Example
    /// ```ignore
    /// use katla_gfx::{TextureDescriptor, VulkanRenderer};
    ///
    /// let desc = TextureDescriptor::rgba8_srgb(512, 512);
    /// let texture = renderer.create_texture(&desc, &pixel_data);
    /// ```
    pub fn create_texture(&mut self, desc: &TextureDescriptor, data: &[u8]) -> TextureHandle {
        let handle = self.texture_manager.create(desc, data);

        if let Some(texture) = self.texture_manager.get_texture_rc(handle) {
            let slot = self
                .bindless_manager
                .register_texture(texture.image_view().vk())
                .expect("Failed to register texture with bindless system");
            self.texture_manager.register_bindless_slot(handle, slot);
        }

        handle
    }

    /// Create an RGBA8 SRGB texture from pixel data.
    ///
    /// Convenience method for the most common texture type.
    pub fn create_texture_rgba(&mut self, width: u32, height: u32, data: &[u8]) -> TextureHandle {
        let desc = TextureDescriptor::rgba8_srgb(width, height);
        self.create_texture(&desc, data)
    }

    /// Create an RGBA8 UNORM texture (for linear data like normal maps).
    pub fn create_texture_unorm(&mut self, width: u32, height: u32, data: &[u8]) -> TextureHandle {
        let desc = TextureDescriptor::rgba8_unorm(width, height);
        self.create_texture(&desc, data)
    }

    /// Create a 1x1 solid color texture.
    ///
    /// Useful for placeholder or fallback textures.
    pub fn create_texture_solid(&mut self, color: [u8; 4]) -> TextureHandle {
        self.texture_manager.create_solid(color)
    }

    /// Create a texture from RGB data (converts to RGBA internally).
    pub fn create_texture_from_rgb(
        &mut self,
        width: u32,
        height: u32,
        rgb_data: &[u8],
    ) -> TextureHandle {
        self.texture_manager
            .create_from_rgb(width, height, rgb_data)
    }

    /// Create an empty texture (no initial data).
    ///
    /// Useful for render targets or textures that will be filled later.
    pub fn create_texture_empty(&mut self, desc: &TextureDescriptor) -> TextureHandle {
        self.texture_manager.create_empty(desc)
    }

    /// Get the default white texture.
    pub fn default_texture(&self) -> TextureHandle {
        self.texture_manager.default_white()
    }

    /// Set frame-level uniforms for the current frame.
    ///
    /// This should be called once per frame before `render_frame()`.
    /// The uniforms are used by all draw calls in the frame.
    ///
    /// # Arguments
    /// * `uniforms` - Frame uniforms containing view/proj matrices, camera position, and lighting
    pub fn set_frame_uniforms(&mut self, uniforms: FrameUniforms) {
        // Write frame uniforms to storage buffer so shaders can read them
        self.storage_manager.update_from_frame_uniforms(&uniforms);

        // Store for reference
        self.frame_uniforms = uniforms;
    }

    /// Execute draw calls from FrameContext and prepare them for rendering.
    ///
    /// This method writes all per-object data from draw calls to the storage buffer.
    /// Frame uniforms should be set separately via `set_frame_uniforms()`.
    ///
    /// This method writes all per-object data from draw calls to the storage buffer.
    /// Frame uniforms should be set separately via `set_frame_uniforms()`.
    ///
    /// # Arguments
    /// * `draw_list` - The DrawList from FrameContext containing draw calls with instance_index
    ///
    /// # Example
    /// ```ignore
    /// // In application render loop
    /// let mut frame = FrameContext::new();
    /// frame.set_camera(&view, &proj);
    /// frame.draw(mesh, material)
    ///     .with_transform(transform)
    ///     .submit();
    ///
    /// // Set frame uniforms
    /// renderer.set_frame_uniforms(&frame.frame_uniforms().unwrap());
    ///
    /// // Execute draw calls (writes to storage buffer)
    /// renderer.execute_draw_calls(&frame.draw_list());
    ///
    /// // Render with frame graph
    /// renderer.render(&mut frame_graph, |frame| {
    ///     frame.submit("geometry", &frame.draw_list());
    /// });
    /// ```
    pub fn execute_draw_calls(&mut self, draw_list: &DrawList) {
        // Write all per-object data to storage buffer
        for draw_call in &draw_list.draws {
            let index = draw_call.instance_index as usize;

            // Extract material parameters
            let color = draw_call.color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let metallic = draw_call.metallic;
            let roughness = draw_call.roughness;
            let ao = draw_call.ao;
            let emission_idx = draw_call.material_params[3]; // emission index stored in w component

            // Note: Texture indices will come from MaterialAsset in future
            let texture_indices = [0u32, 0, 0, 0]; // Default textures for now

            // Write to storage buffer at instance_index
            self.storage_manager.update_object_bindless(
                index,
                &draw_call.model_matrix,
                &color,
                metallic,
                roughness,
                ao,
                emission_idx,
                texture_indices,
            );
        }
    }

    /// Initialize or resize the output render target.
    ///
    /// This creates a texture that the UI renders to, which is then
    /// copied to the swapchain by the present pass.
    pub fn init_output_target(&mut self, width: u32, height: u32) -> Result<(), vk::Result> {
        let needs_resize = self
            .output_target
            .as_ref()
            .map(|t| t.extent.width != width || t.extent.height != height)
            .unwrap_or(true);

        if needs_resize {
            // Old target is dropped automatically with Drop
            self.output_target = None;
            let target = OutputRenderTarget::new(self.context.clone(), width, height)?;
            self.output_target = Some(target);
            info!(
                "Output render target created/resized to {}x{}",
                width, height
            );
        }
        Ok(())
    }

    /// Get the swapchain extent (primary window size).
    pub fn swapchain_extent(&self) -> crate::Size2D {
        let ext = self.frame_context.swapchain.get_extent();
        crate::Size2D::new(ext.width, ext.height)
    }

    /// Get output dimensions.
    pub fn output_extent(&self) -> Option<crate::Size2D> {
        self.output_target
            .as_ref()
            .map(|t| crate::Size2D::from(t.extent))
    }

    // ========================================================================
    // Material Creation API
    // ========================================================================

    /// Create a PBR material with default settings.
    ///
    /// This is a convenience method for creating standard PBR materials with
    /// sensible defaults: depth testing enabled, backface culling enabled,
    /// opaque rendering.
    ///
    /// # Arguments
    /// * `shader_path` - Path to WGSL shader file
    ///
    /// # Returns
    /// A MaterialHandle for the created material.
    ///
    /// # Example
    /// ```ignore
    /// let material = renderer.create_pbr_material("shaders/model.wgsl")?;
    /// ```
    pub fn create_pbr_material(
        &mut self,
        shader_path: impl AsRef<std::path::Path>,
    ) -> Result<MaterialHandle, RendererError> {
        use crate::vulkan::material::compiler::{MaterialOptions, VertexType};

        self.material_compiler
            .compile(
                &mut self.asset_registry,
                shader_path.as_ref(),
                crate::vulkan::material::compiler::MaterialType::Pbr,
                MaterialOptions {
                    vertex_type: VertexType::Pbr,
                    ..Default::default()
                },
            )
            .map_err(|e| {
                RendererError::InitializationFailed(format!("Material compilation failed: {}", e))
            })
    }

    /// Create a UI material with default settings.
    ///
    /// UI materials use premultiplied alpha blending and disable depth writing.
    ///
    /// # Arguments
    /// * `shader_path` - Path to WGSL shader file
    ///
    /// # Returns
    /// A MaterialHandle for the created material.
    ///
    /// # Example
    /// ```ignore
    /// let material = renderer.create_ui_material("shaders/ui.wgsl")?;
    /// ```
    pub fn create_ui_material(
        &mut self,
        shader_path: impl AsRef<std::path::Path>,
    ) -> Result<MaterialHandle, RendererError> {
        use crate::vulkan::material::compiler::{MaterialOptions, VertexType};

        self.material_compiler
            .compile(
                &mut self.asset_registry,
                shader_path.as_ref(),
                crate::vulkan::material::compiler::MaterialType::Ui,
                MaterialOptions {
                    alpha_blended: true,
                    vertex_type: VertexType::Ui,
                    ..Default::default()
                },
            )
            .map_err(|e| {
                RendererError::InitializationFailed(format!("Material compilation failed: {}", e))
            })
    }

    /// Create a material with custom options using the builder pattern.
    ///
    /// This is the advanced API for materials requiring custom configuration
    /// (alpha blending, double-sided rendering, wireframe mode, etc.).
    ///
    /// # Arguments
    /// * `shader_path` - Path to WGSL shader file
    ///
    /// # Returns
    /// A MaterialBuilder for configuring the material.
    ///
    /// # Example
    /// ```ignore
    /// let material = renderer
    ///     .material_builder("shaders/grass.wgsl")
    ///     .alpha_blended()
    ///     .double_sided()
    ///     .build()?;
    /// ```
    pub fn material_builder(
        &mut self,
        shader_path: impl AsRef<std::path::Path>,
    ) -> MaterialBuilder<'_> {
        MaterialBuilder::new(self, shader_path.as_ref().to_path_buf())
    }

    // ========================================================================
    // Render Target Management (Unified)
    // ========================================================================

    /// Texture IDs for built-in render targets.
    pub(crate) const VIEWPORT_TEXTURE_ID: u64 = 2;
    pub(crate) const PREVIEW_TEXTURE_ID: u64 = 101;

    /// Create or resize a render target for the given texture ID.
    ///
    /// # Arguments
    /// * `texture_id` - Unique ID for this render target (e.g., 2 for viewport, 101 for preview)
    /// * `width` - Width in pixels
    /// * `height` - Height in pixels
    /// * `count` - Number of targets to create (1 for single-buffered, FRAMES_IN_FLIGHT for double-buffered)
    pub(crate) fn init_render_target(
        &mut self,
        texture_id: u64,
        width: u32,
        height: u32,
        _count: usize, // Ignored - we only need one target per texture_id
    ) -> Result<(), vk::Result> {
        // Check if we need to resize
        let needs_resize = self
            .render_targets
            .iter()
            .find(|(id, _)| *id == texture_id)
            .map(|(_, t)| t.extent.width != width || t.extent.height != height)
            .unwrap_or(true);

        if needs_resize {
            // Remove old target if exists
            self.render_targets.retain(|(id, _)| *id != texture_id);

            // Create new target
            let target = ViewportRenderTarget::new(self.context.clone(), width, height)?;
            self.render_targets.push((texture_id, target));

            info!(
                "Render target {} created/resized to {}x{}",
                texture_id, width, height
            );
        }
        Ok(())
    }

    /// Get a render target by texture ID.
    pub(crate) fn get_render_target(&self, texture_id: u64) -> Option<&ViewportRenderTarget> {
        self.render_targets
            .iter()
            .find(|(id, _)| *id == texture_id)
            .map(|(_, t)| t)
    }

    /// Get the first render target for a texture ID (alias for get_render_target).
    pub(crate) fn get_render_target_first(&self, texture_id: u64) -> Option<&ViewportRenderTarget> {
        self.get_render_target(texture_id)
    }

    /// Check if a render target exists for the given texture ID.
    pub(crate) fn has_render_target(&self, texture_id: u64) -> bool {
        self.render_targets.iter().any(|(id, _)| *id == texture_id)
    }

    /// Remove a render target.
    pub(crate) fn remove_render_target(&mut self, texture_id: u64) {
        self.render_targets.retain(|(id, _)| *id != texture_id);
    }

    // ========================================================================
    // Viewport System
    // ========================================================================

    /// Create a viewport builder for configuring a new viewport.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let viewport = renderer.create_viewport()
    ///     .size(512, 512)
    ///     .with_depth(DepthFormat::D32SfloatS8Uint)
    ///     .output_mode(OutputMode::Offscreen)
    ///     .label("preview")
    ///     .build(&mut renderer)?;
    /// ```
    pub fn create_viewport(&mut self) -> ViewportBuilder {
        self.viewport_manager.create()
    }

    /// Get the number of viewports.
    pub fn viewport_count(&self) -> usize {
        self.viewport_manager.count()
    }

    /// Get viewport by handle.
    pub fn get_viewport(&self, handle: ViewportHandle) -> Option<&Viewport> {
        self.viewport_manager.get(handle)
    }

    /// Get mutable viewport by handle.
    pub fn get_viewport_mut(&mut self, handle: ViewportHandle) -> Option<&mut Viewport> {
        self.viewport_manager.get_mut(handle)
    }

    /// Get the texture ID for a viewport (for UI sampling).
    /// Returns a u64 that can be used with katla_ui::TextureId::custom(id).
    pub fn viewport_texture_id(&self, handle: ViewportHandle) -> Option<u64> {
        self.viewport_manager.texture_id(handle)
    }

    /// Get the viewport extent by handle.
    pub fn viewport_extent(&self, handle: ViewportHandle) -> Option<crate::Size2D> {
        self.viewport_manager.extent(handle)
    }

    /// Set frame uniforms for a viewport.
    pub fn set_viewport_uniforms(&mut self, handle: ViewportHandle, uniforms: FrameUniforms) {
        if let Some(viewport) = self.viewport_manager.get_mut(handle) {
            viewport.set_frame_uniforms(uniforms);
        }
    }

    /// Set the draw list for a viewport.
    pub fn set_viewport_draw_list(&mut self, handle: ViewportHandle, draw_list: DrawList) {
        if let Some(viewport) = self.viewport_manager.get_mut(handle) {
            viewport.set_draw_list(draw_list);
        }
    }

    /// Clear the draw list for a viewport.
    pub fn clear_viewport_draw_list(&mut self, handle: ViewportHandle) {
        if let Some(viewport) = self.viewport_manager.get_mut(handle) {
            viewport.clear_draw_list();
        }
    }

    /// Destroy a viewport by handle.
    pub fn destroy_viewport(&mut self, handle: ViewportHandle) {
        if self.viewport_manager.destroy(handle) {
            info!("Viewport {} destroyed", handle.0);
        }
    }

    /// Check if a viewport is ready for rendering.
    pub fn is_viewport_ready(&self, handle: ViewportHandle) -> bool {
        self.viewport_manager
            .get(handle)
            .is_some_and(|v| v.storage_manager.is_some() && v.storage_descriptor.is_some())
    }

    /// Update viewport camera and lighting.
    ///
    /// Call this each frame before rendering to update the viewport's view/projection
    /// matrices and lighting parameters.
    pub fn update_viewport_camera(
        &mut self,
        handle: ViewportHandle,
        view_matrix: &[f32; 16],
        proj_matrix: &[f32; 16],
        inv_view_proj: &[f32; 16],
        camera_position: &[f32; 4],
        light_direction: &[f32; 4],
        light_color: &[f32; 4],
        light_intensity: f32,
    ) {
        if let Some(viewport) = self.viewport_manager.get_mut(handle)
            && let Some(ref mut manager) = viewport.storage_manager
        {
            manager.update_frame_with_lighting(
                view_matrix,
                proj_matrix,
                inv_view_proj,
                camera_position,
                light_direction,
                light_color,
                light_intensity,
            );
        }
    }

    pub fn destroy(&mut self) {
        // Destroy output render target (Drop handles cleanup)
        self.output_target = None;

        // Destroy all render targets (Drop handles cleanup)
        self.render_targets.clear();

        // Destroy all viewports (Drop handles cleanup for ViewportRenderTarget)
        self.viewport_manager.clear();

        // Destroy all registered assets first (materials, meshes)
        self.asset_registry.destroy();

        // Destroy material compiler (cleans up descriptor layouts)
        self.material_compiler.destroy();

        // Storage uniform resources will be dropped automatically
        self.context.pre_destroy();
        self.swap_data.destroy(&self.context.device);
        self.frame_context.destroy();
        info!("Clean shutdown!");
    }

    pub fn wait_for_device(&self) {
        unsafe {
            self.context.device.device_wait_idle().unwrap();
        }
    }

    pub fn recreate_swapchain(&mut self) {
        self.wait_for_device();

        let old_extent = self.frame_context.swapchain.get_extent();
        info!("=== Recreating swapchain ===");
        info!("  Old extent: {}x{}", old_extent.width, old_extent.height);

        self.frame_context.recreate_swapchain();

        let new_extent = self.frame_context.swapchain.get_extent();
        info!("  New extent: {}x{}", new_extent.width, new_extent.height);
    }

    pub fn num_images(&self) -> usize {
        self.frame_context.swapchain_image_views.len()
    }

    /// Create a mesh from vertex and index data.
    ///
    /// Returns a handle that can be used in DrawCall objects.
    /// The actual GPU buffers are managed internally by the AssetRegistry.
    ///
    /// # Arguments
    /// * `vertices` - Slice of vertex data (must match the vertex binding of the material)
    /// * `indices` - Index data for indexed drawing
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_mesh<T, U>(&mut self, vertices: &[T], indices: &[U]) -> MeshHandle
    where
        T: bytemuck::Pod,
        U: bytemuck::Pod,
    {
        self.mesh_manager
            .create_mesh(&mut self.asset_registry, vertices, indices)
    }

    /// Register a mesh with pre-existing buffers.
    ///
    /// This is useful when you've already created buffers and want to register them
    /// with the renderer for use in the draw list system.
    ///
    /// # Arguments
    /// * `vertex_buffer` - The vertex buffer (or None if no vertices)
    /// * `index_buffer` - The index buffer (or None if no indices)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn register_mesh(
        &mut self,
        vertex_buffer: Option<VertexBuffer>,
        index_buffer: Option<IndexBuffer>,
    ) -> MeshHandle {
        self.mesh_manager
            .register_mesh(&mut self.asset_registry, vertex_buffer, index_buffer)
    }

    /// Create a cube mesh with the given size.
    ///
    /// # Arguments
    /// * `size` - The size of the cube as [width, height, depth]
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_cube_mesh(&mut self, size: [f32; 3]) -> MeshHandle {
        self.mesh_manager
            .create_cube(&mut self.asset_registry, size)
    }

    /// Create a UV sphere mesh.
    ///
    /// # Arguments
    /// * `radius` - The radius of the sphere
    /// * `segments` - Number of horizontal segments (longitude)
    /// * `rings` - Number of vertical rings (latitude)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_sphere_mesh(&mut self, radius: f32, segments: u32, rings: u32) -> MeshHandle {
        self.mesh_manager
            .create_sphere(&mut self.asset_registry, radius, segments, rings)
    }

    /// Create a plane mesh on the XZ plane.
    ///
    /// # Arguments
    /// * `width` - The width of the plane (X axis)
    /// * `height` - The height of the plane (Z axis)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_plane_mesh(&mut self, width: f32, height: f32) -> MeshHandle {
        self.mesh_manager
            .create_plane(&mut self.asset_registry, width, height)
    }

    /// Create a cylinder mesh standing on Y axis.
    ///
    /// # Arguments
    /// * `height` - The height of the cylinder (Y axis)
    /// * `radius` - The radius of the cylinder
    /// * `segments` - Number of segments around the circumference
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_cylinder_mesh(&mut self, height: f32, radius: f32, segments: u32) -> MeshHandle {
        self.mesh_manager
            .create_cylinder(&mut self.asset_registry, height, radius, segments)
    }

    /// Create a torus (donut) mesh on the XZ plane.
    ///
    /// # Arguments
    /// * `major_radius` - Distance from center of torus to center of tube
    /// * `minor_radius` - Radius of the tube
    /// * `segments` - Number of segments around the major circumference
    /// * `rings` - Number of segments around the minor circumference (tube)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_torus_mesh(
        &mut self,
        major_radius: f32,
        minor_radius: f32,
        segments: u32,
        rings: u32,
    ) -> MeshHandle {
        self.mesh_manager.create_torus(
            &mut self.asset_registry,
            major_radius,
            minor_radius,
            segments,
            rings,
        )
    }

    /// Create a plane on the XY axis (vertical, facing +Z).
    ///
    /// # Arguments
    /// * `width` - The width of the plane (X axis)
    /// * `height` - The height of the plane (Y axis)
    /// * `segments` - Number of subdivisions in both directions
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_plane_xy_mesh(&mut self, width: f32, height: f32, segments: u32) -> MeshHandle {
        self.mesh_manager
            .create_plane_xy(&mut self.asset_registry, width, height, segments)
    }

    /// Create a dynamic mesh from raw vertex and index data.
    ///
    /// This method creates a mesh that can be updated every frame.
    /// The buffers are created with CPU-accessible memory for fast updates.
    ///
    /// # Arguments
    /// * `vertex_data` - Raw vertex data in bytes
    /// * `vertex_count` - Number of vertices
    /// * `indices` - Index data (u32)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_mesh_dynamic(
        &mut self,
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> MeshHandle {
        self.mesh_manager.create_mesh_dynamic(
            &mut self.asset_registry,
            vertex_data,
            vertex_count,
            indices,
        )
    }

    /// Update a dynamic mesh with new vertex and index data.
    ///
    /// The mesh must have been created with `create_mesh_dynamic`.
    /// This method updates the existing buffers with new data.
    ///
    /// # Arguments
    /// * `mesh` - Handle to the mesh to update
    /// * `vertex_data` - New vertex data in bytes
    /// * `vertex_count` - Number of vertices
    /// * `indices` - New index data (u32)
    ///
    /// # Returns
    /// `Ok(())` on success, `Err` if mesh not found or buffers too small.
    pub fn update_mesh_dynamic(
        &mut self,
        mesh: MeshHandle,
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError> {
        self.mesh_manager.update_mesh_dynamic(
            &mut self.asset_registry,
            mesh,
            vertex_data,
            vertex_count,
            indices,
        )
    }

    /// Update object transform data in the storage buffer.
    ///
    /// This updates the object uniforms at the given index in the storage buffer.
    /// The shader will use this data when rendering with the corresponding instance_index.
    ///
    /// # Arguments
    /// * `index` - Object index (0-255)
    /// * `model_matrix` - Model matrix (object-to-world) - column-major [f32; 16]
    /// * `color` - Base color tint (RGBA)
    /// * `metallic` - Metallic factor (0.0 = dielectric, 1.0 = metal)
    /// * `roughness` - Roughness factor (0.0 = smooth, 1.0 = rough)
    /// * `ao` - Ambient occlusion factor (0.0 = full occlusion, 1.0 = none)
    /// * `normal_scale` - Normal map scale factor
    /// * `texture_indices` - Texture indices for bindless [albedo, normal, mr, ao]
    pub fn update_object_storage(
        &mut self,
        index: usize,
        model_matrix: &[f32; 16],
        color: &[f32; 4],
        metallic: f32,
        roughness: f32,
        ao: f32,
        normal_scale: f32,
        texture_indices: [u32; 4],
    ) {
        self.storage_manager.update_object_bindless(
            index,
            model_matrix,
            color,
            metallic,
            roughness,
            ao,
            normal_scale,
            texture_indices,
        );
    }

    /// Register a Material with the renderer.
    ///
    /// This method registers a material instance for use in rendering.
    /// Materials can be created from templates or directly from pipelines.
    ///
    /// # Arguments
    /// * `material` - The material to register
    ///
    /// # Returns
    /// A `MaterialHandle` that references the registered material.
    ///
    /// # Example
    /// ```ignore
    /// let material = Material::new(template_handle)
    ///     .with_texture(0, albedo_texture);
    ///
    /// let handle = renderer.register_material(&material);
    /// ```
    pub fn register_material(&mut self, material: &Material) -> MaterialHandle {
        use crate::renderer::registry::MaterialAsset;

        let pipeline = material.pipeline();
        let vertex_binding = material
            .vertex_binding()
            .expect("Material must have vertex binding")
            .clone();
        let is_bindless = material.is_bindless();

        // Extract texture indices from material (convert handles to u32 indices)
        let texture_handles = material.texture_slots();
        let texture_indices = [
            texture_handles[0].index(),
            texture_handles[1].index(),
            texture_handles[2].index(),
            texture_handles[3].index(),
        ];

        let material_asset = MaterialAsset {
            pipeline,
            vertex_binding,
            material_data: crate::renderer::registry::MaterialData {
                color: [1.0, 1.0, 1.0, 1.0],
                metallic: 0.0,
                roughness: 0.5,
                ao: 1.0,
                texture_indices,
                emission_index: 0,
            },
            material_descriptor_set: None,
            material_descriptor_layout: None,
        };

        self.asset_registry.register_material(material_asset)
    }

    /// Returns the default white PBR material handle.
    ///
    /// The default material is a simple bindless PBR material that renders
    /// geometry with white albedo and default PBR parameters.
    ///
    /// # Panics
    /// Panics if `init_default_material()` has not been called.
    ///
    /// # Example
    /// ```ignore
    /// // Initialize first (typically during application startup)
    /// renderer.init_default_material(binding, PathBuf::from("shaders/pbr.wgsl"));
    ///
    /// // Then use the default material
    /// let material = renderer.default_material();
    /// let draw = DrawCall::new(mesh, material);
    /// ```
    pub fn default_material(&self) -> MaterialHandle {
        self.default_material_handle
            .expect("default_material() called before init_default_material()")
    }

    /// Submits a slice of DrawCalls for immediate rendering.
    ///
    /// This is a convenience method for simple cases with few draw calls.
    /// It creates a temporary DrawList, populates it with the provided draw calls,
    /// and submits it through the existing render infrastructure.
    ///
    /// # Performance
    /// For >100 draws/frame, use `DrawList` directly to avoid repeated
    /// submission overhead. This method is optimized for convenience in simple cases.
    ///
    /// # Arguments
    /// * `draw_calls` - Slice of DrawCall objects to render
    ///
    /// # Returns
    /// `Ok(())` on success, or an error if submission fails.
    ///
    /// # Example
    /// ```ignore
    /// let mesh = renderer.create_cube_mesh([1.0, 1.0, 1.0]);
    /// let material = renderer.default_material();
    ///
    /// let draw = DrawCall::new(mesh, material)
    ///     .with_transform(model_matrix)
    ///     .with_color([1.0, 0.0, 0.0, 1.0]);
    ///
    /// renderer.draw_immediate(&[draw])?;
    /// ```
    pub fn draw_immediate(&mut self, draw_calls: &[DrawCall]) -> Result<(), RendererError> {
        if draw_calls.is_empty() {
            return Ok(());
        }

        // Create a temporary DrawList and populate it
        let mut draw_list = DrawList::new();
        for draw in draw_calls {
            draw_list.push(draw.clone());
        }

        // Submit through the draw_list_cell
        *self.draw_list_cell.borrow_mut() = Some(draw_list);

        Ok(())
    }

    /// Convenience: draw a single mesh with default material.
    ///
    /// # Performance
    /// Zero heap allocation for single mesh drawing.
    /// For >100 draws/frame, use `DrawList` directly.
    #[inline]
    pub fn draw_mesh(
        &mut self,
        mesh: MeshHandle,
        transform: [f32; 16],
    ) -> Result<(), RendererError> {
        let material = self.default_material();
        self.draw_mesh_with_material(mesh, transform, material)
    }

    /// Convenience: draw a single mesh with custom material.
    ///
    /// # Performance
    /// Zero heap allocation using `std::slice::from_ref`.
    /// For >100 draws/frame, use `DrawList` directly.
    #[inline]
    pub fn draw_mesh_with_material(
        &mut self,
        mesh: MeshHandle,
        transform: [f32; 16],
        material: MaterialHandle,
    ) -> Result<(), RendererError> {
        let draw_call = DrawCall::new(mesh, material).with_transform(transform);
        self.draw_immediate(std::slice::from_ref(&draw_call))
    }

    /// Get the skeleton descriptor set for a handle.
    pub fn get_skeleton_descriptor(
        &self,
        handle: SkeletonHandle,
    ) -> Option<&SkeletonDescriptorSet> {
        self.skeleton_descriptors
            .get(handle.index() as usize)?
            .as_ref()
    }

    /// Queue UI for rendering in the next frame.
    ///
    /// Call this before `render_frame()` each frame. The data is consumed
    /// during `render_frame()` and rendered as an overlay.
    ///
    /// # Arguments
    /// * `vertex_bytes` - Raw vertex data (VertexUI as bytes)
    /// * `vertex_count` - Number of vertices
    /// * `indices` - Index data (u32)
    /// * `commands` - Draw commands with clip rects and texture indices
    /// * `screen_size` - Screen dimensions [width, height] in pixels
    pub fn render_ui(
        &mut self,
        vertex_bytes: &[u8],
        vertex_count: u32,
        indices: &[u32],
        commands: &[UiDrawCommand],
        screen_size: [f32; 2],
    ) {
        if vertex_bytes.is_empty() || indices.is_empty() || commands.is_empty() {
            return;
        }

        self.pending_ui = Some(UiFrameData {
            vertex_bytes: vertex_bytes.to_vec(),
            vertex_count,
            indices: indices.to_vec(),
            commands: commands.to_vec(),
            screen_size,
        });
    }

    // ========================================================================
    // Render Graph System
    // ========================================================================

    /// Create a frame graph builder for configuring a render pipeline.
    ///
    /// Frame graphs are built once at startup and executed every frame.
    /// They define the structure of your rendering pipeline (passes,
    /// resources, dependencies) and handle automatic barrier generation.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let frame_graph = renderer.create_frame_graph()
    ///     .add_pass(GeometryPass::new("geometry")
    ///         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
    ///         .write_depth("depth", ImageFormat::D32Sfloat))
    ///     .add_pass(FullscreenPass::new("tonemap")
    ///         .read("color")
    ///         .write_backbuffer())
    ///     .build()?;
    /// ```
    pub fn create_frame_graph(&self) -> crate::render_graph::FrameGraphBuilder {
        crate::render_graph::FrameGraphBuilder::new()
    }

    /// Execute a frame graph with the given submission callback.
    ///
    /// This is the main rendering entry point when using frame graphs.
    /// The callback receives a [`Frame`] for submitting draw lists to passes.
    ///
    /// # Arguments
    /// * `frame_graph` - The compiled frame graph to execute
    /// * `f` - Callback for submitting work to passes
    ///
    /// # Example
    ///
    /// ```ignore
    /// renderer.render(&frame_graph, |frame| {
    ///     frame.submit("geometry", &opaque_draw_list);
    ///     frame.submit("geometry", &transparent_draw_list);
    ///     // Passes without draw lists (like tonemap) run automatically
    /// });
    /// ```
    pub fn render<F>(&mut self, frame_graph: &mut crate::render_graph::FrameGraph, f: F)
    where
        F: FnOnce(&mut crate::render_graph::Frame),
    {
        // 1. Wait for previous frame to complete (also resets the fence)
        self.swap_data.wait_for_fence(&self.context.device);

        // 2. Acquire next swapchain image
        let (image_index, _is_suboptimal) = unsafe {
            self.frame_context
                .swapchain
                .swapchain_loader
                .acquire_next_image(
                    self.frame_context.swapchain.swapchain,
                    u64::MAX,
                    self.swap_data.image_available_semaphore(),
                    vk::Fence::null(),
                )
                .expect("Failed to acquire swapchain image")
        };

        // 3. Get command buffer for this frame
        let frame_idx = self.swap_data.current_frame();
        let cmd = self.frame_context.command_buffers[frame_idx].vk_command_buffer();

        // 4. Begin command buffer
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.context
                .device
                .begin_command_buffer(cmd, &begin_info)
                .expect("Failed to begin command buffer");
        }

        // 5. Transition swapchain image to COLOR_ATTACHMENT_OPTIMAL for rendering
        let swapchain_image = self.frame_context.swapchain_images[image_index as usize].vk();
        ImageBarrier::transition_from_undefined(
            &cmd,
            &self.context.device,
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );

        // 6. Execute frame graph (records commands into the command buffer)
        frame_graph
            .execute(self, image_index, f)
            .expect("Frame graph execution failed");

        // 7. Transition swapchain image from COLOR_ATTACHMENT_OPTIMAL to PRESENT_SRC_KHR
        let swapchain_image = self.frame_context.swapchain_images[image_index as usize].vk();
        ImageBarrier::transition(
            &cmd,
            &self.context.device,
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );

        // 8. End command buffer
        unsafe {
            self.context
                .device
                .end_command_buffer(cmd)
                .expect("Failed to end command buffer");
        }

        // 8. Submit command buffer with synchronization
        let render_finished_semaphore = self.swap_data.render_finished_semaphore(image_index);
        let wait_semaphores = [self.swap_data.image_available_semaphore()];
        let signal_semaphores = [render_finished_semaphore];
        let swapchains = [self.frame_context.swapchain.swapchain];
        let image_indices = [image_index];

        self.context.gfx_queue.submit(
            &[&self.frame_context.command_buffers[frame_idx]],
            &wait_semaphores,
            &signal_semaphores,
            self.swap_data.in_flight_fence(),
        );

        // 9. Present to swapchain
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        unsafe {
            self.frame_context
                .swapchain
                .swapchain_loader
                .queue_present(self.context.gfx_queue.vk_queue(), &present_info)
                .expect("Failed to present");
        }

        // 10. Advance to next frame
        self.swap_data.step_frame();
    }
}

/// Offscreen render target for viewport rendering.
///
/// This holds the color and depth attachments for rendering the 3D scene
/// to a texture that can be sampled by the UI viewport panel.
pub struct ViewportRenderTarget {
    /// Color attachment image.
    color_image: vk::Image,
    color_memory: Option<gpu_allocator::vulkan::Allocation>,
    /// Color attachment image view (exposed for render graph).
    pub(crate) color_image_view: vk::ImageView,
    /// Depth attachment image.
    depth_image: vk::Image,
    depth_memory: Option<gpu_allocator::vulkan::Allocation>,
    depth_image_view: vk::ImageView,
    /// Render extent.
    pub extent: vk::Extent2D,
    /// Sampler for sampling the color texture.
    sampler: vk::Sampler,
    /// Context for cleanup.
    context: Rc<VulkanContext>,
}

impl ViewportRenderTarget {
    /// Create a new viewport render target with the given dimensions.
    pub fn new(context: Rc<VulkanContext>, width: u32, height: u32) -> Result<Self, vk::Result> {
        unsafe {
            let extent = vk::Extent2D { width, height };
            let extent3d = vk::Extent3D {
                width,
                height,
                depth: 1,
            };

            // Create color image (HDR format for PBR rendering with tonemapping)
            let color_create_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .extent(extent3d)
                .mip_levels(1)
                .array_layers(1)
                .format(vk::Format::R16G16B16A16_SFLOAT)
                .tiling(vk::ImageTiling::OPTIMAL)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .usage(
                    vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::TRANSFER_SRC,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .samples(vk::SampleCountFlags::TYPE_1);

            let (color_image, color_memory) =
                context.create_image(color_create_info, gpu_allocator::MemoryLocation::GpuOnly);

            // Create color image view
            let color_view_create_info = vk::ImageViewCreateInfo::default()
                .image(color_image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R16G16B16A16_SFLOAT)
                .components(vk::ComponentMapping::default())
                .subresource_range(COLOR_SUBRESOURCE_RANGE);

            let color_image_view = context
                .device
                .create_image_view(&color_view_create_info, None)?;

            // Create depth image (D32_SFLOAT_S8_UINT to match pipeline formats)
            let depth_create_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .extent(extent3d)
                .mip_levels(1)
                .array_layers(1)
                .format(vk::Format::D32_SFLOAT_S8_UINT)
                .tiling(vk::ImageTiling::OPTIMAL)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .samples(vk::SampleCountFlags::TYPE_1);

            let (depth_image, depth_memory) =
                context.create_image(depth_create_info, gpu_allocator::MemoryLocation::GpuOnly);

            // Create depth image view
            let depth_view_create_info = vk::ImageViewCreateInfo::default()
                .image(depth_image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::D32_SFLOAT_S8_UINT)
                .components(vk::ComponentMapping::default())
                .subresource_range(DEPTH_SUBRESOURCE_RANGE);

            let depth_image_view = context
                .device
                .create_image_view(&depth_view_create_info, None)?;

            // Create sampler
            let sampler = context.create_sampler_clamp_to_edge()?;

            // Transition images to their initial layouts
            let cmd_buffer = context.begin_single_time_commands();
            let cmd = cmd_buffer.vk_command_buffer();

            // Transition color to shader read only (since we blit to it, not render to it)
            // This matches the expected old_layout in the blit barrier
            ImageBarrier::transition_from_undefined(
                &cmd,
                &context.device,
                color_image,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );

            // Transition depth to depth stencil attachment optimal
            ImageBarrier::transition_from_undefined_with_range(
                &cmd,
                &context.device,
                depth_image,
                vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                DEPTH_SUBRESOURCE_RANGE,
            );

            context.end_single_time_commands(cmd_buffer);

            Ok(Self {
                color_image,
                color_memory: Some(color_memory),
                color_image_view,
                depth_image,
                depth_memory: Some(depth_memory),
                depth_image_view,
                extent,
                sampler: sampler.into(),
                context,
            })
        }
    }
}
impl Drop for ViewportRenderTarget {
    fn drop(&mut self) {
        unsafe {
            self.context.device.destroy_sampler(self.sampler, None);
            self.context
                .device
                .destroy_image_view(self.color_image_view, None);
            self.context.device.destroy_image(self.color_image, None);
            if let Some(memory) = self.color_memory.take() {
                self.context.allocator.borrow_mut().free(memory).ok();
            }
            self.context
                .device
                .destroy_image_view(self.depth_image_view, None);
            self.context.device.destroy_image(self.depth_image, None);
            if let Some(memory) = self.depth_memory.take() {
                self.context.allocator.borrow_mut().free(memory).ok();
            }
        }
    }
}

/// Output render target for final UI composition.
/// The UI renders to this texture, then present_pass blits it to the swapchain.
/// This decouples rendering from presentation for a cleaner architecture.
pub struct OutputRenderTarget {
    /// Color attachment image.
    pub(crate) color_image: vk::Image,
    color_memory: Option<gpu_allocator::vulkan::Allocation>,
    pub(crate) color_image_view: vk::ImageView,
    /// Render extent (matches swapchain size).
    pub extent: vk::Extent2D,
    /// Context for cleanup.
    context: Rc<VulkanContext>,
}

impl OutputRenderTarget {
    /// Create a new output render target with the given dimensions.
    pub fn new(context: Rc<VulkanContext>, width: u32, height: u32) -> Result<Self, vk::Result> {
        unsafe {
            let extent = vk::Extent2D { width, height };
            let extent3d = vk::Extent3D {
                width,
                height,
                depth: 1,
            };

            // Create color image (RGBA8, can be used as color attachment and transfer source/dest)
            let color_create_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .extent(extent3d)
                .mip_levels(1)
                .array_layers(1)
                .format(vk::Format::B8G8R8A8_SRGB) // Match swapchain format
                .tiling(vk::ImageTiling::OPTIMAL)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .usage(
                    vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::TRANSFER_SRC
                        | vk::ImageUsageFlags::TRANSFER_DST,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .samples(vk::SampleCountFlags::TYPE_1);

            let (color_image, color_memory) =
                context.create_image(color_create_info, gpu_allocator::MemoryLocation::GpuOnly);

            // Create color image view
            let color_view_create_info = vk::ImageViewCreateInfo::default()
                .image(color_image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::B8G8R8A8_SRGB)
                .components(vk::ComponentMapping::default())
                .subresource_range(COLOR_SUBRESOURCE_RANGE);

            let color_image_view = context
                .device
                .create_image_view(&color_view_create_info, None)?;

            // Transition image to COLOR_ATTACHMENT_OPTIMAL (ready for UI rendering)
            let cmd_buffer = context.begin_single_time_commands();
            let cmd = cmd_buffer.vk_command_buffer();

            ImageBarrier::transition_from_undefined(
                &cmd,
                &context.device,
                color_image,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            );

            context.end_single_time_commands(cmd_buffer);

            Ok(Self {
                color_image,
                color_memory: Some(color_memory),
                color_image_view,
                extent,
                context,
            })
        }
    }
}

impl Drop for OutputRenderTarget {
    fn drop(&mut self) {
        unsafe {
            self.context
                .device
                .destroy_image_view(self.color_image_view, None);
            self.context.device.destroy_image(self.color_image, None);
            if let Some(memory) = self.color_memory.take() {
                self.context.allocator.borrow_mut().free(memory).ok();
            }
        }
    }
}
