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
pub mod ui_renderer;
pub mod viewport_manager;

pub use crate::handle::{Handle, MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle};
use crate::viewport::{ViewportBuilder, ViewportHandle};
pub use crate::vulkan::context::ValidationMode;
pub use registry::AssetRegistry;
pub use types::{DrawCall, DrawList, FrameUniforms, InstanceData, UIDrawList, UiDrawCommand};

use crate::handle::ResourceStorage;
use crate::material::Material;
use crate::texture::{TextureDescriptor, TextureManager};
use crate::vulkan::context::VulkanContext;
use crate::{
    BindlessTextureManager, IndexBuffer, MAX_BINDLESS_TEXTURES, RendererError, SkeletonBuffer,
    SkeletonDescriptorSet, StorageDescriptorSet, StorageUniformManager, SwapData, VertexBuffer,
    VulkanFrameCtx, viewport::Viewport,
};
use ash::vk;
use log::{error, info, warn};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{ffi::CString, rc::Rc};

use crate::barrier::ImageBarrier;
use crate::sync::COLOR_SUBRESOURCE_RANGE;
use crate::vulkan::IndexType;
use crate::vulkan::material::compiler::{MaterialBuilder, MaterialCompiler};

/// Per-frame UI rendering resources.
pub(crate) struct UiFrameResources {
    /// Per-frame UI vertex buffers.
    pub vertex_buffers: Vec<VertexBuffer>,
    /// Per-frame UI index buffers.
    pub index_buffers: Vec<IndexBuffer>,
    /// Per-frame UI descriptor sets (owns both set and pool, automatic cleanup).
    pub descriptor_sets: Vec<Option<crate::vulkan::descriptor_set::DescriptorSet>>,
    /// UI uniform buffer (reused across frames).
    pub uniform_buffer: Option<(vk::Buffer, gpu_allocator::vulkan::Allocation)>,
}

impl UiFrameResources {
    /// Create new UI frame resources with pre-allocated buffers.
    fn new(context: &Rc<VulkanContext>) -> Self {
        let mut vertex_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut index_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut descriptor_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);

        for _ in 0..FRAMES_IN_FLIGHT {
            vertex_buffers.push(VertexBuffer::new(
                context.clone(),
                1024 * 1024, // 1MB initial size
                65536,       // Vertex count
            ));
            index_buffers.push(IndexBuffer::new(
                context.clone(),
                1024 * 1024,       // 1MB initial size
                IndexType::Uint32, // 32-bit indices
                65536,             // Index count
            ));
            descriptor_sets.push(None);
        }

        Self {
            vertex_buffers,
            index_buffers,
            descriptor_sets,
            uniform_buffer: None,
        }
    }
}

/// Transpose a 4x4 matrix from row-major to column-major format.
///
/// Pending readback operation for async frame checking
pub struct PendingReadback {
    frame: usize,
    fence: vk::Fence,
    command_buffer: crate::vulkan::commandbuffer::CommandBuffer,
    staging_buffer: vk::Buffer,
    staging_allocation: gpu_allocator::vulkan::Allocation,
    buffer_size: vk::DeviceSize,
}

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
    pub texture_manager: TextureManager,
    /// Storage uniform manager for storage buffer-based uniforms.
    /// Materials use storage buffers with instance indexing.
    pub(crate) storage_manager: StorageUniformManager,
    /// Per-frame storage descriptor sets for binding frame and object uniforms.
    /// Each set contains the storage buffer bound at two offsets (frame_data at 0, objects at 256).
    pub(crate) storage_descriptor_sets: Vec<StorageDescriptorSet>,
    /// Skeleton descriptor sets for GPU skeletal animation.
    /// Indexed by SkeletonHandle via ResourceStorage.
    pub(crate) skeleton_descriptors: ResourceStorage<SkeletonDescriptorSet>,
    /// Skeleton buffers for GPU skeletal animation.
    /// Indexed by SkeletonHandle via ResourceStorage.
    pub(crate) skeleton_buffers: ResourceStorage<SkeletonBuffer>,
    /// Compositing descriptor set layout for multi-viewport compositing.
    /// Created during initialization and used when compiling compositing materials.
    pub(crate) compositing_descriptor_set_layout: vk::DescriptorSetLayout,
    /// Frame-level uniforms set once per frame via set_frame_uniforms().
    pub(crate) frame_uniforms: FrameUniforms,
    /// Last presented swapchain image index (for debugging readback).
    last_presented_image_index: Option<u32>,
    /// Cached default white PBR material handle.
    default_material_handle: Option<MaterialHandle>,
    /// Pending async readback operation
    pending_readback: Option<PendingReadback>,
    /// Output render target for final composition (UI renders here, then present_pass copies to swapchain).
    output_target: Option<OutputRenderTarget>,
    /// Viewport manager for viewport and render target management.
    pub(crate) viewport_manager: viewport_manager::ViewportManager,
    /// Material compiler for compiling materials from shaders.
    pub(crate) material_compiler: MaterialCompiler,
    /// UI rendering subsystem - owns UI resources and font atlas.
    pub ui_renderer: ui_renderer::UIRenderer,
    /// Global particle system for GPU-driven particle effects.
    pub particle_system: Option<crate::particles::GlobalParticleSystem>,
}

/// Number of frames that can be processed concurrently.
/// This is an implementation detail for double-buffering.
pub(crate) const FRAMES_IN_FLIGHT: usize = 2;

/// Maximum number of objects that can be drawn per frame.
///
/// This is the limit of the storage buffer array that holds per-object data.
/// Each draw call uses one slot indexed by `instance_index`. If you exceed
/// this limit, `execute_draw_calls` will return a `RendererError::ObjectLimitExceeded`.
pub const MAX_OBJECTS_PER_FRAME: u32 = 256;

impl VulkanRenderer {
    pub fn init(
        display: &dyn HasDisplayHandle,
        window: &dyn HasWindowHandle,
        validation_mode: ValidationMode,
        app_name: CString,
        engine_name: CString,
    ) -> Result<Self, RendererError> {
        let context = Rc::new(VulkanContext::init(
            display,
            window,
            validation_mode,
            app_name,
            engine_name,
        ));

        // Set up validation logging at appropriate log levels
        if validation_mode.is_enabled() {
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
        let storage_manager = StorageUniformManager::new(context.clone(), FRAMES_IN_FLIGHT)?;

        // Create per-frame storage descriptor sets for binding frame and object uniforms
        let mut storage_descriptor_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for frame_idx in 0..FRAMES_IN_FLIGHT {
            let descriptor_set = StorageDescriptorSet::new(
                &context,
                storage_manager.buffer(frame_idx),
                storage_manager.buffer_size(),
            )
            .map_err(|e| {
                error!("Failed to create storage descriptor set: {:?}", e);
                RendererError::InitializationFailed(format!(
                    "Failed to create storage descriptor set for frame {}: {:?}",
                    frame_idx, e
                ))
            })?;
            storage_descriptor_sets.push(descriptor_set);
        }

        // Initialize mesh manager
        let mesh_manager = mesh_manager::MeshManager::new(context.clone());

        // Initialize viewport manager
        let viewport_manager = viewport_manager::ViewportManager::new();

        // Initialize material compiler (use first descriptor set for compilation)
        let material_compiler = MaterialCompiler::new(
            context.clone(),
            &bindless_manager,
            &storage_descriptor_sets[0],
        )
        .map_err(|e| {
            error!("Failed to create material compiler: {:?}", e);
            RendererError::InitializationFailed("Failed to create material compiler".to_string())
        })?;
        info!("Material compiler initialized");

        // Create compositing descriptor set layout for multi-viewport rendering
        let compositing_descriptor_set_layout = {
            use crate::render_graph::descriptor_sets::CompositingDescriptorSet;
            CompositingDescriptorSet::create_layout(&context.device).map_err(|e| {
                error!(
                    "Failed to create compositing descriptor set layout: {:?}",
                    e
                );
                RendererError::InitializationFailed(
                    "Failed to create compositing descriptor set layout".to_string(),
                )
            })?
        };
        info!("Compositing descriptor set layout created");

        // Initialize global particle system
        let particle_system = match crate::particles::GlobalParticleSystem::new(
            &context,
            crate::particles::DEFAULT_MAX_PARTICLES,
        ) {
            Ok(system) => {
                info!("✨ Modern particle system initialized successfully");
                info!("   - Global particle pool: 1,048,576 particles");
                info!("   - Memory footprint: ~60 MB GPU");
                info!("   - Architecture: Single buffer + atomic counters");
                Some(system)
            }
            Err(e) => {
                warn!("Failed to initialize particle system: {}", e);
                warn!("Particle effects will be disabled");
                None
            }
        };

        Ok(Self {
            context: context.clone(),
            frame_context,
            swap_data,
            mesh_manager,
            asset_registry: AssetRegistry::new(),
            bindless_manager,
            texture_manager,
            storage_manager,
            storage_descriptor_sets,
            skeleton_descriptors: ResourceStorage::new(),
            skeleton_buffers: ResourceStorage::new(),
            compositing_descriptor_set_layout,
            frame_uniforms: FrameUniforms::default(),
            last_presented_image_index: None,
            default_material_handle: None,
            pending_readback: None,
            output_target: None,
            viewport_manager,
            material_compiler,
            ui_renderer: ui_renderer::UIRenderer::new(&context),
            particle_system,
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

    /// Initialize particle emit pipeline (must be called after renderer is fully initialized).
    pub fn init_particle_emit_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        if let Some(ref mut ps) = self.particle_system {
            // Load particle emit shader
            match self
                .material_compiler
                .shader_cache
                .borrow_mut()
                .load_shader(shader_path, vk::ShaderStageFlags::COMPUTE)
            {
                Ok(shader_module) => {
                    let shader_module_wrapper = crate::sync::VkShaderModule(shader_module);
                    ps.create_emit_pipeline(&mut self.asset_registry, shader_module_wrapper)
                        .map_err(|e| {
                            RendererError::InitializationFailed(format!(
                                "Failed to create particle emit pipeline: {}",
                                e
                            ))
                        })?;
                    info!("✅ Particle emit pipeline created successfully");
                    Ok(())
                }
                Err(e) => {
                    warn!("Failed to load particle emit shader: {}", e);
                    Err(RendererError::InitializationFailed(format!(
                        "Failed to load particle emit shader: {}",
                        e
                    )))
                }
            }
        } else {
            warn!("Particle system not initialized, skipping emit pipeline creation");
            Ok(())
        }
    }

    /// Initialize particle simulate pipeline (must be called after renderer is fully initialized).
    pub fn init_particle_simulate_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        if let Some(ref mut ps) = self.particle_system {
            // Load particle simulate shader
            match self
                .material_compiler
                .shader_cache
                .borrow_mut()
                .load_shader(shader_path, vk::ShaderStageFlags::COMPUTE)
            {
                Ok(shader_module) => {
                    let shader_module_wrapper = crate::sync::VkShaderModule(shader_module);
                    ps.create_simulate_pipeline(&mut self.asset_registry, shader_module_wrapper)
                        .map_err(|e| {
                            RendererError::InitializationFailed(format!(
                                "Failed to create particle simulate pipeline: {}",
                                e
                            ))
                        })?;
                    info!("✅ Particle simulate pipeline created successfully");
                    Ok(())
                }
                Err(e) => {
                    warn!("Failed to load particle simulate shader: {}", e);
                    Err(RendererError::InitializationFailed(format!(
                        "Failed to load particle simulate shader: {}",
                        e
                    )))
                }
            }
        } else {
            warn!("Particle system not initialized, skipping simulate pipeline creation");
            Ok(())
        }
    }

    /// Initialize particle render pipeline.
    ///
    /// Loads particle vertex and fragment shaders and creates the render pipeline.
    pub fn init_particle_render_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        if let Some(ref mut ps) = self.particle_system {
            // Load particle vertex shader
            let vert_shader = self
                .material_compiler
                .shader_cache
                .borrow_mut()
                .load_shader(shader_path, vk::ShaderStageFlags::VERTEX)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load particle vertex shader: {}",
                        e
                    ))
                })?;

            // Load particle fragment shader
            let frag_shader = self
                .material_compiler
                .shader_cache
                .borrow_mut()
                .load_shader(shader_path, vk::ShaderStageFlags::FRAGMENT)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load particle fragment shader: {}",
                        e
                    ))
                })?;

            let vert_shader_wrapper = crate::sync::VkShaderModule(vert_shader);
            let frag_shader_wrapper = crate::sync::VkShaderModule(frag_shader);

            ps.create_render_pipeline(
                &mut self.asset_registry,
                vert_shader_wrapper,
                frag_shader_wrapper,
            )
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to create particle render pipeline: {}",
                    e
                ))
            })?;

            info!("✅ Particle render pipeline created successfully");
            Ok(())
        } else {
            warn!("Particle system not initialized, skipping render pipeline creation");
            Ok(())
        }
    }

    /// Get the compositing descriptor set layout for compiling compositing materials.
    ///
    /// This layout is used when creating the pipeline layout for compositing materials.
    /// It must be set in the material compiler before compiling the compositing shader.
    pub fn compositing_descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        self.compositing_descriptor_set_layout
    }

    /// Set the compositing descriptor set layout in the material compiler.
    ///
    /// This must be called before compiling a compositing material to ensure
    /// the pipeline layout includes descriptor set 2.
    pub fn set_compositing_descriptor_set_layout(&mut self) {
        self.material_compiler
            .set_compositing_descriptor_set_layout(self.compositing_descriptor_set_layout);
    }

    /// Clear the compositing descriptor set layout from the material compiler.
    ///
    /// This should be called after compiling the compositing material.
    pub fn clear_compositing_descriptor_set_layout(&mut self) {
        self.material_compiler
            .clear_compositing_descriptor_set_layout();
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

    /// Create a 1x1 solid color texture.
    ///
    /// Useful for placeholder or fallback textures.
    pub fn create_texture_solid(&mut self, color: [u8; 4]) -> TextureHandle {
        self.texture_manager.create_solid(color)
    }

    /// Get the default white texture.
    pub fn default_texture(&self) -> TextureHandle {
        self.texture_manager.default_white()
    }

    /// Get the shared sampler used by the bindless texture system.
    ///
    /// This sampler can be used for transient textures that need to be sampled
    /// (e.g., viewport render targets displayed in the UI).
    pub fn shared_sampler(&self) -> crate::sync::VkSampler {
        self.bindless_manager.shared_sampler()
    }

    // ========================================================================
    // UI Font Atlas Management
    // ========================================================================

    /// Create or update the UI font atlas texture from pixel data.
    ///
    /// Creates a texture with the given dimensions and uploads the pixel data.
    /// The texture is automatically registered with the bindless system for shader access.
    ///
    /// # Arguments
    /// * `width` - Atlas width in pixels
    /// * `height` - Atlas height in pixels
    /// * `data` - RGBA pixel data
    ///
    /// # Returns
    /// The texture handle for the font atlas.
    pub fn create_ui_font_atlas(&mut self, width: u32, height: u32, data: &[u8]) -> TextureHandle {
        // Use SRGB format for font atlas to ensure correct color sampling
        // Text glyphs rendered as SRGB for proper color reproduction
        let desc = TextureDescriptor::rgba8_srgb(width, height);
        let handle = self.create_texture(&desc, data);

        // Register the font atlas with the bindless texture system
        if let Some(texture) = self.texture_manager.get_texture_rc(handle) {
            let bindless_slot = self
                .register_bindless_texture(texture.image_view.vk())
                .unwrap_or_else(|e| {
                    log::error!("Failed to register font atlas with bindless system: {}", e);
                    // Fall back to slot 0 (should not happen in normal operation)
                    0
                });
            self.ui_renderer.set_font_atlas_bindless_slot(bindless_slot);
            log::debug!(
                "Font atlas registered with bindless system at slot {}",
                bindless_slot
            );
        }

        self.ui_renderer.set_font_atlas(handle);
        handle
    }

    /// Update the UI font atlas texture with new pixel data.
    ///
    /// Use this when the atlas has been resized or new glyphs have been added.
    ///
    /// # Arguments
    /// * `width` - Atlas width in pixels
    /// * `height` - Atlas height in pixels
    /// * `data` - RGBA pixel data
    pub fn update_ui_font_atlas(&mut self, width: u32, height: u32, data: &[u8]) {
        let current_handle = self.ui_renderer.font_atlas();

        if let Some(handle) = current_handle {
            if let Some(texture) = self.texture_manager.get_texture_rc(handle) {
                if texture.width == width && texture.height == height {
                    texture.update_data(data);
                } else {
                    let new_handle = self.create_ui_font_atlas(width, height, data);
                    self.ui_renderer.set_font_atlas(new_handle);
                }
            }
        } else {
            self.create_ui_font_atlas(width, height, data);
        }
    }

    /// Get the font atlas texture handle.
    pub fn ui_font_atlas(&self) -> Option<TextureHandle> {
        self.ui_renderer.font_atlas()
    }

    /// Set frame-level uniforms for the current frame.
    ///
    /// This should be called once per frame before `render_frame()` or `execute_draw_calls()`.
    /// The uniforms are used by all draw calls in the frame.
    ///
    /// **Important:** This must be called before `execute_draw_calls()` as it selects
    /// the appropriate per-frame storage buffer. The recommended order is:
    /// 1. `set_frame_uniforms()` - selects frame buffer and writes frame data
    /// 2. `execute_draw_calls()` - writes per-object data to the same buffer
    /// 3. `render()` - renders using the prepared data
    ///
    /// # Arguments
    /// * `uniforms` - Frame uniforms containing view/proj matrices, camera position, and lighting
    pub fn set_frame_uniforms(&mut self, uniforms: FrameUniforms) {
        // Get frame index from swap_data (the source of truth for frame advancement)
        let frame_idx = self.swap_data.current_frame();

        // Write frame uniforms to storage buffer for current frame
        self.storage_manager
            .update_from_frame_uniforms(frame_idx, &uniforms);

        // Store for reference
        self.frame_uniforms = uniforms;
    }

    /// Execute draw calls from FrameContext and prepare them for rendering.
    ///
    /// This method writes all per-object data from draw calls to the storage buffer.
    /// Frame uniforms should be set separately via `set_frame_uniforms()`.
    ///
    /// # Arguments
    /// * `draw_list` - The DrawList from FrameContext containing draw calls with instance_index
    ///
    /// # Errors
    ///
    /// Returns `RendererError::ObjectLimitExceeded` if any draw call's `instance_index`
    /// exceeds `MAX_OBJECTS_PER_FRAME`.
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
    /// renderer.execute_draw_calls(&frame.draw_list())?;
    ///
    /// // Render with frame graph
    /// renderer.render(&mut frame_graph, |frame| {
    ///     frame.submit("geometry", &frame.draw_list());
    /// })?;
    /// ```
    pub fn execute_draw_calls(&mut self, draw_list: &DrawList) -> Result<(), RendererError> {
        // Get current frame index from swap_data (source of truth)
        let frame_idx = self.current_frame();

        // Write all per-object data to storage buffer
        for draw_call in &draw_list.draws {
            let index = draw_call.instance_index as usize;

            // Bounds check with clear error message
            if index >= MAX_OBJECTS_PER_FRAME as usize {
                return Err(RendererError::ObjectLimitExceeded {
                    index,
                    limit: MAX_OBJECTS_PER_FRAME as usize,
                });
            }

            // Extract material parameters
            let color = draw_call.color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let metallic = draw_call.metallic;
            let roughness = draw_call.roughness;
            let ao = draw_call.ao;
            let emission_idx = draw_call.material_params[3]; // emission index stored in w component

            // Get texture indices from material
            let texture_indices = self
                .asset_registry
                .get_material(draw_call.material)
                .map(|m| m.material_data.texture_indices)
                .unwrap_or([0, 0, 0, 0]);

            // Write to storage buffer at instance_index
            self.storage_manager.update_object_bindless(
                frame_idx,
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
        Ok(())
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

    /// Create a PBR material with configurable color format.
    ///
    /// This is a convenience method for creating standard PBR materials with
    /// sensible defaults: depth testing enabled, backface culling enabled,
    /// opaque rendering.
    ///
    /// Uses swapchain color format (B8G8R8A8Srgb) by default. Specify HDR format
    /// for rendering to intermediate textures (e.g., for tonemapping passes).
    ///
    /// # Arguments
    /// * `shader_path` - Path to WGSL shader file
    /// * `color_format` - Optional color attachment format. None = swapchain format (LDR),
    ///   Some(ImageFormat::R16G16B16A16Sfloat) = HDR rendering
    ///
    /// # Returns
    /// A MaterialHandle for the created material.
    ///
    /// # Example
    /// ```ignore
    /// use katla_gfx::vulkan::material::compiler::{MaterialOptions, VertexType};
    ///
    /// // PBR material (default settings)
    /// let pbr = renderer.compile_material("shaders/pbr.wgsl", MaterialOptions {
    ///     vertex_type: VertexType::Pbr,
    ///     ..Default::default()
    /// })?;
    ///
    /// // UI material with alpha blending
    /// let ui = renderer.compile_material("shaders/ui.wgsl", MaterialOptions {
    ///     vertex_type: VertexType::Ui,
    ///     alpha_blended: true,
    ///     ..Default::default()
    /// })?;
    ///
    /// // Skinned mesh material for GLTF models
    /// let skinned = renderer.compile_material("shaders/skinned.wgsl", MaterialOptions {
    ///     vertex_type: VertexType::Skinned,
    ///     ..Default::default()
    /// })?;
    ///
    /// // HDR material for intermediate render targets
    /// let hdr = renderer.compile_material("shaders/pbr.wgsl", MaterialOptions {
    ///     vertex_type: VertexType::Pbr,
    ///     color_format: ImageFormat::R16G16B16A16Sfloat,
    ///     ..Default::default()
    /// })?;
    /// ```
    pub fn compile_material(
        &mut self,
        shader_path: impl AsRef<std::path::Path>,
        options: crate::vulkan::material::compiler::MaterialOptions,
    ) -> Result<MaterialHandle, RendererError> {
        use crate::vulkan::material::compiler::MaterialType;

        let material_type = match options.vertex_type {
            crate::vulkan::material::compiler::VertexType::Pbr => MaterialType::Pbr,
            crate::vulkan::material::compiler::VertexType::Ui => MaterialType::Ui,
            _ => MaterialType::Auto,
        };

        self.material_compiler
            .compile(
                &mut self.asset_registry,
                shader_path.as_ref(),
                material_type,
                options,
            )
            .map_err(|e| {
                RendererError::InitializationFailed(format!("Material compilation failed: {}", e))
            })
    }

    /// Ensure a material is compiled for a specific format.
    ///
    /// If the material was created with `ImageFormat::Auto`, this will compile
    /// it for the specified format. If already compiled, this does nothing.
    ///
    /// This is called automatically by the frame graph before execution.
    pub(crate) fn ensure_material_compiled(
        &mut self,
        material: MaterialHandle,
        format: crate::texture::ImageFormat,
    ) -> Result<(), RendererError> {
        self.material_compiler
            .compile_deferred_material(&mut self.asset_registry, material, format)
            .map_err(|e| {
                RendererError::InitializationFailed(format!("Material compilation failed: {}", e))
            })
    }

    /// Register a texture image view with the bindless texture system.
    ///
    /// Returns the bindless slot index that can be used to sample this texture
    /// from shaders using the bindless texture array.
    ///
    /// # Arguments
    /// * `image_view` - Vulkan image view handle
    ///
    /// # Returns
    /// The bindless texture slot index (u32)
    ///
    /// # Example
    /// ```ignore
    /// let slot = renderer.register_bindless_texture(image_view)?;
    /// // Pass slot to shader via object_uniforms.texture_indices.x
    /// ```
    pub fn register_bindless_texture(
        &mut self,
        image_view: vk::ImageView,
    ) -> Result<u32, RendererError> {
        self.bindless_manager
            .register_texture(image_view)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to register bindless texture: {}",
                    e
                ))
            })
    }

    /// Update an existing bindless texture slot with a new image view.
    ///
    /// This is used when a texture is recreated (e.g., after window resize) and the
    /// bindless descriptor needs to be updated with the new image view.
    ///
    /// # Arguments
    /// * `slot` - The bindless slot to update
    /// * `image_view` - The new image view
    ///
    /// # Returns
    /// Ok(()) if successful, or an error if the slot is invalid.
    ///
    /// # Example
    /// ```ignore
    /// // After recreating a texture, update the bindless descriptor:
    /// renderer.update_bindless_texture(slot, new_image_view)?;
    /// ```
    pub fn update_bindless_texture(
        &mut self,
        slot: u32,
        image_view: vk::ImageView,
    ) -> Result<(), RendererError> {
        self.bindless_manager
            .update_texture(slot, image_view)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to update bindless texture slot {}: {}",
                    slot, e
                ))
            })
    }

    /// Set the HDR texture index for tonemapping.
    ///
    /// Sets object[0].texture_indices.x to the HDR texture bindless index.
    /// The tonemap shader reads from objects[0] to get the HDR texture index.
    ///
    /// # Arguments
    /// * `hdr_texture_index` - Bindless texture index for HDR color attachment
    ///
    /// Set the HDR texture index for tonemapping.
    ///
    /// This method sets up object[0] in the storage buffer to pass the HDR texture index
    /// to fullscreen shaders (like the tonemap pass). The tonemap shader reads from
    /// `objects[0].texture_indices.x` to get the bindless texture slot.
    ///
    /// # Contract
    /// - Object index 0 is reserved for fullscreen/post-processing shader parameters
    /// - The HDR texture must already be registered with the bindless system
    /// - Tonemap shaders must read from `objects[0].texture_indices.x`
    ///
    /// # Arguments
    /// * `hdr_texture_index` - Bindless texture slot index for the HDR color attachment
    ///
    /// # Example
    /// ```ignore
    /// // Register HDR texture with bindless
    /// let hdr_slot = frame_graph.register_transient_texture_bindless(&mut renderer, "hdr_color")?;
    ///
    /// // Set up tonemap shader to sample from HDR texture
    /// renderer.set_hdr_texture_index(hdr_slot);
    /// ```
    pub fn set_hdr_texture_index(&mut self, hdr_texture_index: u32) {
        // Set object[0] texture indices (HDR texture in x, others unused)
        //
        // Note: Object index 0 is reserved for fullscreen/post-processing shader parameters.
        // This is a documented contract between the renderer and fullscreen shaders.
        let frame_idx = self.current_frame();
        self.storage_manager.update_object_bindless(
            frame_idx,
            0, // object index 0 is reserved for tonemap params
            &[
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ], // identity matrix (not used)
            &[1.0, 1.0, 1.0, 1.0], // white color (not used)
            0.0, // metallic (not used)
            0.0, // roughness (not used)
            1.0, // ao (not used)
            0.0, // emission index (not used)
            [hdr_texture_index, 0, 0, 0], // HDR texture index in x
        );
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
    /// # When to use this
    ///
    /// Most applications should use `compile_material()` with `MaterialOptions`.
    /// This method is intended for:
    /// - GLTF model loaders that need custom vertex types (Skinned)
    /// - Advanced material configuration beyond PBR defaults
    /// - Custom render targets with specific color formats
    ///
    /// # Example (GLTF loading with skinned meshes)
    /// ```ignore
    /// let material = renderer
    ///     .material_builder(&shader_path)
    ///     .with_vertex_type(VertexType::Skinned)
    ///     .with_color_format(ImageFormat::R16G16B16A16Sfloat)
    ///     .build()?;
    /// ```
    pub fn material_builder(
        &mut self,
        shader_path: impl AsRef<std::path::Path>,
    ) -> MaterialBuilder<'_> {
        MaterialBuilder::new(self, shader_path.as_ref().to_path_buf())
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
    ///
    /// Note: This is a legacy method for UI compatibility. The actual texture
    /// is managed by the frame graph system, not the viewport itself.
    /// Returns a u64 that can be used with katla_ui::TextureId::custom(id).
    pub fn viewport_texture_id(&self, handle: ViewportHandle) -> Option<u64> {
        self.viewport_manager.texture_id(handle)
    }

    /// Get the viewport extent by handle.
    pub fn viewport_extent(&self, handle: ViewportHandle) -> Option<crate::Size2D> {
        self.viewport_manager.extent(handle)
    }

    /// Destroy a viewport by handle.
    pub fn destroy_viewport(&mut self, handle: ViewportHandle) {
        if self.viewport_manager.destroy(handle) {
            info!("Viewport {} destroyed", handle.0);
        }
    }

    pub fn destroy(&mut self) {
        // Note: Pending readback should have been cleaned up by wait_for_pending_readback()
        // in cleanup_on_exit(). This is a safety check in case destroy() is called directly.
        if let Some(readback) = self.pending_readback.take() {
            log::warn!(
                "Pending readback found during destroy() - cleanup should have happened earlier"
            );
            unsafe {
                let _ = self
                    .context
                    .device
                    .wait_for_fences(&[readback.fence], true, u64::MAX);
                readback.command_buffer.return_to_pool();
                self.context.device.destroy_fence(readback.fence, None);
                self.context
                    .free_buffer(readback.staging_buffer, readback.staging_allocation);
            }
        }

        // Wait for device idle to ensure all GPU operations have completed
        self.wait_for_device();

        // Destroy output render target (Drop handles cleanup)
        self.output_target = None;

        // Destroy all viewports
        self.viewport_manager.clear();

        // Destroy particle system FIRST (before destroying other resources)
        // This ensures proper cleanup order and avoids heap corruption
        if let Some(mut particle_system) = self.particle_system.take() {
            info!("Destroying particle system");
            particle_system.destroy();
        }

        // Destroy all registered assets first (materials, meshes)
        self.asset_registry.destroy();

        // Destroy material compiler (cleans up descriptor layouts)
        self.material_compiler.destroy();

        // Destroy compositing descriptor set layout
        unsafe {
            self.context
                .device
                .destroy_descriptor_set_layout(self.compositing_descriptor_set_layout, None);
        }

        // Clean up UI resources
        {
            let ui_resources = self.ui_renderer.ui_resources_mut();
            // Vertex and index buffers have Drop impls that clean up themselves
            ui_resources.vertex_buffers.clear();
            ui_resources.index_buffers.clear();

            // Descriptor sets own their pools and clean up automatically via Drop
            ui_resources.descriptor_sets.clear();

            // Destroy uniform buffer
            if let Some((buffer, allocation)) = ui_resources.uniform_buffer.take() {
                self.context.free_buffer(buffer, allocation);
            }
        }

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

    pub fn recreate_swapchain(
        &mut self,
        frame_graph: &mut crate::render_graph::FrameGraph,
    ) -> Vec<(String, u32)> {
        self.wait_for_device();

        let old_extent = self.frame_context.swapchain.get_extent();
        info!("=== Recreating swapchain ===");
        info!("  Old extent: {}x{}", old_extent.width, old_extent.height);

        self.frame_context.recreate_swapchain();

        let new_extent = self.frame_context.swapchain.get_extent();
        info!("  New extent: {}x{}", new_extent.width, new_extent.height);

        // Recreate transient textures with new dimensions
        match frame_graph.recreate_transient_textures(self, new_extent.width, new_extent.height) {
            Ok(mut recreated_textures) => {
                // Update internal references (tonemap HDR texture)
                recreated_textures.retain(|(name, slot)| {
                    if name == "hdr_color" {
                        // Update tonemap pass with new HDR texture slot
                        if let Err(e) = frame_graph.set_tonemap_texture_index("tonemap", *slot) {
                            log::error!("Failed to update tonemap texture index: {}", e);
                        }
                    }
                    true // Keep all entries for app layer
                });
                info!(
                    "Recreated {} transient textures for resize",
                    recreated_textures.len()
                );
                recreated_textures
            }
            Err(e) => {
                log::error!("Failed to recreate transient textures: {}", e);
                Vec::new()
            }
        }
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
        let _is_bindless = material.is_bindless();

        // Convert texture handles to bindless indices
        let texture_handles = material.texture_slots();
        let texture_indices = [
            self.texture_manager
                .get_bindless_index(texture_handles[0])
                .unwrap_or(0),
            self.texture_manager
                .get_bindless_index(texture_handles[1])
                .unwrap_or(0),
            self.texture_manager
                .get_bindless_index(texture_handles[2])
                .unwrap_or(0),
            self.texture_manager
                .get_bindless_index(texture_handles[3])
                .unwrap_or(0),
        ];

        let material_asset = MaterialAsset {
            pipeline: Some(pipeline),
            fully_compiled: true,
            shader_path: None,
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

    /// Set texture indices for a material.
    ///
    /// Updates the material's texture indices for bindless sampling.
    /// Texture indices are obtained from `create_texture_*` methods.
    ///
    /// # Arguments
    /// * `material` - Material handle to update
    /// * `indices` - [albedo, normal, metallic_roughness, ao] texture indices
    pub fn set_material_texture_indices(&mut self, material: MaterialHandle, indices: [u32; 4]) {
        if let Some(mat) = self.asset_registry.get_material_mut(material) {
            mat.material_data.texture_indices = indices;
        }
    }

    /// Get the bindless texture index for a texture handle.
    ///
    /// Returns 0 if the texture isn't registered with bindless.
    pub fn get_texture_bindless_index(&self, handle: TextureHandle) -> u32 {
        self.texture_manager.get_bindless_index(handle).unwrap_or(0)
    }

    /// Get the bindless slot index for a texture handle.
    ///
    /// This is an alias for `get_texture_bindless_index()` that returns an Option
    /// instead of defaulting to 0 for unregistered textures.
    ///
    /// # Arguments
    /// * `handle` - The texture handle to query
    ///
    /// # Returns
    /// The bindless slot index if the texture is registered, None otherwise.
    ///
    /// # Example
    /// ```ignore
    /// if let Some(slot) = renderer.get_bindless_slot(texture_handle) {
    ///     println!("Texture is at bindless slot {}", slot);
    /// } else {
    ///     println!("Texture is not registered with bindless system");
    /// }
    /// ```
    pub fn get_bindless_slot(&self, handle: TextureHandle) -> Option<u32> {
        self.texture_manager.get_bindless_index(handle)
    }

    /// Get the texture handle at a specific bindless slot.
    ///
    /// This is useful for debugging and texture inspection tools to determine
    /// which texture occupies a given slot.
    ///
    /// # Arguments
    /// * `slot` - The bindless slot index
    ///
    /// # Returns
    /// The TextureHandle at that slot, or None if the slot is not registered
    /// or doesn't exist.
    ///
    /// # Example
    /// ```ignore
    /// // Query which texture is in slot 10
    /// if let Some(handle) = renderer.get_texture_at_slot(10) {
    ///     println!("Texture at slot 10: {:?}", handle);
    /// }
    /// ```
    pub fn get_texture_at_slot(&self, slot: u32) -> Option<TextureHandle> {
        self.texture_manager.get_texture_at_slot(slot)
    }

    /// Get all registered texture handles with their bindless slots.
    ///
    /// This returns an iterator over (TextureHandle, slot) pairs for all
    /// textures that have been registered with the bindless system.
    ///
    /// # Example
    /// ```ignore
    /// for (handle, slot) in renderer.iter_bindless_textures() {
    ///     println!("Texture {:?} is at slot {}", handle, slot);
    /// }
    /// ```
    pub fn iter_bindless_textures(&self) -> impl Iterator<Item = (TextureHandle, u32)> + '_ {
        self.texture_manager.iter_bindless_textures()
    }

    /// Get the font atlas bindless texture slot.
    ///
    /// Returns None if the font atlas has not been registered with the
    /// bindless system yet.
    ///
    /// # Example
    /// ```ignore
    /// if let Some(slot) = renderer.get_font_atlas_bindless_slot() {
    ///     println!("Font atlas is at bindless slot {}", slot);
    /// }
    /// ```
    pub fn get_font_atlas_bindless_slot(&self) -> Option<u32> {
        self.ui_renderer.font_atlas_bindless_slot()
    }

    /// Get information about bindless texture slot utilization.
    ///
    /// Returns (occupied_count, available_count, total_count).
    ///
    /// # Example
    /// ```ignore
    /// let (occupied, available, total) = renderer.get_bindless_stats();
    /// println!("Bindless slots: {}/{} used", occupied, total);
    /// ```
    pub fn get_bindless_stats(&self) -> (usize, usize, usize) {
        (
            self.bindless_manager.occupied_slot_count(),
            self.bindless_manager.available_slot_count(),
            self.bindless_manager.total_slot_count(),
        )
    }

    /// Get a debug representation of bindless slot allocation.
    ///
    /// Returns a string showing which slots are occupied and which are free.
    /// Useful for debugging texture allocation issues.
    ///
    /// # Example
    /// ```ignore
    /// let debug_info = renderer.debug_bindless_slot_allocation();
    /// println!("{}", debug_info);
    /// ```
    pub fn debug_bindless_slot_allocation(&self) -> String {
        self.bindless_manager.debug_slot_allocation()
    }

    /// Get a list of all occupied bindless slots.
    ///
    /// Returns a vector of (slot, image_view) pairs for all occupied slots.
    /// Useful for debugging which textures are currently bound.
    ///
    /// # Example
    /// ```ignore
    /// for (slot, image_view) in renderer.list_occupied_bindless_slots() {
    ///     println!("Slot {}: ImageView({:?})", slot, image_view);
    /// }
    /// ```
    pub fn list_occupied_bindless_slots(&self) -> Vec<(u32, ash::vk::ImageView)> {
        self.bindless_manager.list_occupied_slots()
    }

    /// Get debug information about a specific bindless slot.
    ///
    /// Returns a string describing the slot contents.
    ///
    /// # Arguments
    /// * `slot` - The bindless slot index
    ///
    /// # Example
    /// ```ignore
    /// println!("{}", renderer.debug_bindless_slot_info(5));
    /// ```
    pub fn debug_bindless_slot_info(&self, slot: u32) -> String {
        self.bindless_manager.debug_slot_info(slot)
    }

    /// Get a debug representation of all registered bindless textures.
    ///
    /// Returns a string listing all texture handles with their bindless slots.
    /// Useful for debugging texture allocation and slot assignments.
    ///
    /// # Example
    /// ```ignore
    /// let debug_info = renderer.debug_bindless_textures();
    /// println!("{}", debug_info);
    /// ```
    pub fn debug_bindless_textures(&self) -> String {
        self.texture_manager.debug_bindless_textures()
    }

    /// Get a list of all texture handles that are not registered with bindless.
    ///
    /// Returns texture handles that exist but don't have a bindless slot.
    /// Useful for finding textures that should be registered but aren't.
    ///
    /// # Example
    /// ```ignore
    /// for handle in renderer.list_unregistered_textures() {
    ///     println!("Texture {:?} is not registered with bindless", handle);
    /// }
    /// ```
    pub fn list_unregistered_textures(&self) -> Vec<crate::TextureHandle> {
        self.texture_manager.list_unregistered_textures()
    }

    /// Check if a texture is registered with the bindless system.
    ///
    /// # Arguments
    /// * `handle` - The texture handle to check
    ///
    /// # Returns
    /// true if the texture has a bindless slot assigned.
    ///
    /// # Example
    /// ```ignore
    /// if !renderer.is_bindless_registered(texture_handle) {
    ///     println!("Texture is not registered with bindless");
    /// }
    /// ```
    pub fn is_bindless_registered(&self, handle: crate::TextureHandle) -> bool {
        self.texture_manager.is_bindless_registered(handle)
    }

    /// Get bindless texture registration statistics.
    ///
    /// Returns (registered_count, unregistered_count, total_count).
    ///
    /// # Example
    /// ```ignore
    /// let (registered, unregistered, total) = renderer.get_bindless_registration_stats();
    /// println!("Bindless: {}/{} registered", registered, total);
    /// ```
    pub fn get_bindless_registration_stats(&self) -> (usize, usize, usize) {
        self.texture_manager.bindless_stats()
    }

    /// Compile a fullscreen/post-processing shader and return its pipeline handle.
    ///
    /// This is intended for post-processing effects like tonemapping, bloom, etc.
    /// The shader should generate a fullscreen triangle using `@builtin(vertex_index)`
    /// and sample from input textures.
    ///
    /// # Arguments
    ///
    /// * `shader_path` - Path to the WGSL shader file (contains both vertex and fragment)
    ///
    /// # Returns
    ///
    /// A `PipelineHandle` that can be passed to `FullscreenPass::pipeline()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let tonemap_pipeline = renderer.compile_fullscreen_shader(PathBuf::from("shaders/tonemapping.wgsl"))?;
    ///
    /// let graph = renderer.create_frame_graph()
    ///     .add_pass(FullscreenPass::new("tonemap")
    ///         .read("hdr_color")
    ///         .write_backbuffer()
    ///         .pipeline(tonemap_pipeline))
    ///     .build()?;
    /// ```
    pub fn compile_fullscreen_shader(
        &mut self,
        shader_path: std::path::PathBuf,
    ) -> Result<crate::handle::PipelineHandle, RendererError> {
        self.compile_fullscreen_shader_with_format(
            shader_path,
            crate::texture::ImageFormat::B8G8R8A8Srgb,
        )
    }

    /// Compile a fullscreen/post-processing shader with custom color format.
    ///
    /// Unlike `compile_fullscreen_shader()` which uses swapchain format,
    /// this allows specifying a custom color format for rendering to
    /// intermediate textures (e.g., HDR render targets).
    ///
    /// # Arguments
    /// * `shader_path` - Path to the WGSL shader file (contains both vertex and fragment)
    /// * `color_format` - Color attachment format for rendering
    ///
    /// # Returns
    ///
    /// A `PipelineHandle` that can be passed to `FullscreenPass::pipeline()`.
    pub fn compile_fullscreen_shader_with_format(
        &mut self,
        shader_path: std::path::PathBuf,
        color_format: crate::texture::ImageFormat,
    ) -> Result<crate::handle::PipelineHandle, RendererError> {
        use crate::pipeline::{CullMode, FrontFace};
        use crate::vulkan::material::builder::PipelineBuilder;

        use ash::vk;

        // Create storage descriptor layout for fullscreen pass
        let storage_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        ];

        let storage_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&storage_bindings);
        let storage_layout = unsafe {
            self.context
                .device
                .create_descriptor_set_layout(&storage_layout_info, None)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!("Descriptor layout: {:?}", e))
                })?
        };

        let bindless_layout = self.bindless_manager.descriptor_set_layout();

        // Load shaders (fullscreen shaders use same module for both stages)
        let mut cache = self.material_compiler.shader_cache.borrow_mut();

        let vert_module = cache
            .load_shader(&shader_path, vk::ShaderStageFlags::VERTEX)
            .map_err(|e| RendererError::InitializationFailed(format!("Vertex shader: {:?}", e)))?;
        let frag_module = cache
            .load_shader(&shader_path, vk::ShaderStageFlags::FRAGMENT)
            .map_err(|e| {
                RendererError::InitializationFailed(format!("Fragment shader: {:?}", e))
            })?;
        drop(cache);

        // Build pipeline with fullscreen-specific settings
        let builder = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert_module, frag_module)
            .with_descriptor_layouts(vec![storage_layout, bindless_layout])
            // No vertex binding - fullscreen triangle generated in shader
            .with_depth_test(false, false, crate::pipeline::CompareOp::Always)
            .with_cull_mode(CullMode::None, FrontFace::CounterClockwise)
            // Use specified color format
            .with_rendering_formats(
                Some(color_format),
                Some(crate::texture::ImageFormat::D32SfloatS8Uint),
            );

        let pipeline = builder.build_dynamic().map_err(|e| {
            RendererError::InitializationFailed(format!("Pipeline creation: {:?}", e))
        })?;

        // The pipeline now holds the descriptor layouts, so we can destroy our temporary copy
        unsafe {
            self.context
                .device
                .destroy_descriptor_set_layout(storage_layout, None);
        }

        Ok(self.asset_registry.register_pipeline(pipeline))
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

    /// Simple immediate mode draw - the happy path for basic rendering.
    ///
    /// This method combines three steps into one:
    /// 1. Sets frame uniforms (camera, lighting)
    /// 2. Writes draw call data to GPU storage buffer
    /// 3. Returns a DrawList for submission to render passes
    ///
    /// # Arguments
    /// * `uniforms` - Frame-level data (view/proj matrices, lighting)
    /// * `draw_calls` - Slice of DrawCall objects to render
    ///
    /// # Returns
    /// A DrawList that can be passed to `frame.submit()` in the render callback.
    ///
    /// # Example
    /// ```ignore
    /// // Setup
    /// let mesh = renderer.create_cube_mesh([1.0, 1.0, 1.0]);
    /// let material = renderer.default_material();
    ///
    /// // Render loop
    /// let draw_list = renderer.draw(
    ///     &frame_uniforms,
    ///     &[DrawCall::new(mesh, material)
    ///         .with_transform(model_matrix)
    ///         .with_color([1.0, 0.0, 0.0, 1.0])]
    /// )?;
    ///
    /// renderer.render(&mut frame_graph, |frame| {
    ///     frame.submit("geometry", &draw_list);
    /// })?;
    /// ```
    ///
    /// # Performance Note
    /// For complex scenes with >100 draw calls, use `DrawList` directly with
    /// `set_frame_uniforms()` + `execute_draw_calls()` for better control.
    pub fn draw(
        &mut self,
        uniforms: &FrameUniforms,
        draw_calls: &[DrawCall],
    ) -> Result<DrawList, RendererError> {
        // Set frame uniforms
        self.set_frame_uniforms(uniforms.clone());

        // Build draw list
        let mut draw_list = DrawList::new();
        for draw in draw_calls {
            draw_list.push(draw.clone());
        }

        // Write to storage buffer
        self.execute_draw_calls(&draw_list)?;

        Ok(draw_list)
    }

    /// Convenience: draw a single mesh with custom material.
    ///
    /// # Performance
    /// Zero heap allocation using `std::slice::from_ref`.
    /// Get the skeleton descriptor set for a handle.
    pub fn get_skeleton_descriptor(
        &self,
        handle: SkeletonHandle,
    ) -> Option<&SkeletonDescriptorSet> {
        self.skeleton_descriptors.get(handle.index())
    }

    /// Create a new skeleton for GPU skeletal animation.
    ///
    /// Allocates a storage buffer for joint matrices and creates a descriptor set
    /// for binding to shaders (Set 2).
    ///
    /// # Arguments
    /// * `joint_count` - Number of joints in the skeleton
    ///
    /// # Returns
    /// A SkeletonHandle for the created skeleton, or an error if creation fails.
    pub fn create_skeleton(&mut self, joint_count: usize) -> Result<SkeletonHandle, RendererError> {
        use crate::vulkan::skeleton_buffer::SkeletonBuffer;

        let buffer = SkeletonBuffer::new(self.context.clone(), joint_count);

        let pool = self.material_compiler.skeleton_descriptor_pool();
        let layout = self.material_compiler.skeleton_descriptor_layout();

        let descriptor_set =
            SkeletonDescriptorSet::new(self.context.clone(), &buffer, pool, layout).map_err(
                |e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create skeleton descriptor: {:?}",
                        e
                    ))
                },
            )?;

        // Store both the descriptor and the buffer with matching IDs
        let id = self.skeleton_descriptors.insert(descriptor_set);
        let _ = self.skeleton_buffers.insert(buffer);
        let handle = SkeletonHandle::new(id);

        Ok(handle)
    }

    /// Update skeleton joint matrices on the GPU.
    ///
    /// Uploads the current pose to the skeleton's storage buffer.
    /// Call this each frame after computing animation but before rendering.
    ///
    /// # Arguments
    /// * `handle` - Skeleton handle from `create_skeleton()`
    /// * `matrices` - Joint matrices as column-major [f32; 16] arrays (one per joint)
    pub fn update_skeleton(&mut self, handle: SkeletonHandle, matrices: &[[f32; 16]]) {
        if let Some(buffer) = self.skeleton_buffers.get_mut(handle.index()) {
            buffer.update(matrices);
        }
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
    ///
    /// Returns the frame index from swap_data, which is the authoritative source.
    /// This ensures consistency across all frame-indexed resource access.
    pub fn current_frame(&self) -> usize {
        self.swap_data.current_frame()
    }

    /// Initialize the particle system.
    ///
    /// This must be called after renderer initialization but before frame graph creation.
    /// Sets up the global particle buffer and prepares for particle rendering.
    pub fn init_particle_system(&mut self) -> Result<(), RendererError> {
        use crate::particles::GlobalParticleSystem;

        info!("Initializing particle system...");

        let particle_system = GlobalParticleSystem::new(&self.context, 1_048_576).map_err(|e| {
            RendererError::InitializationFailed(format!("Failed to create particle system: {}", e))
        })?;

        self.particle_system = Some(particle_system);

        info!("Particle system initialized successfully");
        Ok(())
    }

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
        // NOTE: This wait is for the in-flight frames, NOT for readback operations
        self.swap_data.wait_for_fence(&self.context.device);

        // 2. Get frame index (start_frame() was already called in set_frame_uniforms())
        let frame_idx = self.current_frame();

        // 3. Acquire next swapchain image
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

        // Store image index for readback debugging
        self.last_presented_image_index = Some(image_index);

        // 4. Get command buffer for this frame
        let cmd = self.frame_context.command_buffers[frame_idx].vk_command_buffer();

        // 5. Begin command buffer
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.context
                .device
                .begin_command_buffer(cmd, &begin_info)
                .expect("Failed to begin command buffer");
        }

        // 5. Transition swapchain image to COLOR_ATTACHMENT_OPTIMAL for rendering
        // Use transition_from_undefined for swapchain images because:
        // - After acquire_next_image, the actual layout is platform-specific
        // - We use load_op=CLEAR so we don't care about preserving contents
        // - Works correctly after swapchain recreation (images start as UNDEFINED)
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

        // 9. Submit command buffer with synchronization
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

        // 10. Present to swapchain
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

        // 11. Read back particle timing data (after GPU work completes)
        if let Some(ref mut ps) = self.particle_system
            && let Some(compute_time) = ps.get_compute_time_ms()
        {
            log::debug!("Particle compute time: {:.3} ms", compute_time);
        }
    }

    /// Queue an asynchronous readback of the last presented swapchain image.
    ///
    /// This is useful for detecting black frames and synchronization issues.
    /// The readback is asynchronous - use `check_pending_readback()` on the next frame
    /// to retrieve the results.
    ///
    /// # Arguments
    /// * `frame` - Current frame number for tracking
    ///
    /// # Returns
    /// * `Ok(())` - Readback was queued successfully
    /// * `Err(RendererError)` - Failed to queue readback
    ///
    /// # Async Behavior
    /// This function queues a GPU copy operation and returns immediately.
    /// The results will be available on the next frame via `check_pending_readback()`.
    /// This allows catching cross-frame synchronization issues that synchronous readback would mask.
    pub fn queue_async_readback(&mut self, frame: usize) -> Result<(), RendererError> {
        use ash::vk;

        // Get the last presented image index
        let image_index = if let Some(idx) = self.last_presented_image_index {
            idx
        } else {
            return Ok(()); // No frame presented yet
        };

        let swapchain_image = self.frame_context.swapchain_images[image_index as usize].vk();
        let extent = self.frame_context.swapchain.get_extent();
        let width = extent.width;
        let height = extent.height;

        // Create a staging buffer for readback
        let buffer_size = (width * height * 4) as vk::DeviceSize;
        let buffer_info = vk::BufferCreateInfo::default()
            .size(buffer_size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let (staging_buffer, staging_allocation) = self
            .context
            .allocate_buffer(&buffer_info, gpu_allocator::MemoryLocation::CpuToGpu);

        // Create a fence for this readback operation
        let fence_info = vk::FenceCreateInfo::default();
        let fence = unsafe {
            self.context
                .device
                .create_fence(&fence_info, None)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!("Failed to create fence: {}", e))
                })?
        };

        // Create a command buffer for the copy operation
        let command_buffer = crate::vulkan::commandbuffer::CommandBuffer::new(
            &self.context.device,
            &crate::vulkan::commandpool::CommandPool {
                device: self.context.device.clone(),
                command_pool: self.context.transfer_command_pool,
            },
        );

        // Begin command buffer
        command_buffer.begin_single_time_command();

        // Transition swapchain image to TRANSFER_SRC optimal layout
        let barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        unsafe {
            self.context.device.cmd_pipeline_barrier(
                command_buffer.vk_command_buffer(),
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
        }

        // Copy image to staging buffer
        let buffer_image_copy = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });

        unsafe {
            self.context.device.cmd_copy_image_to_buffer(
                command_buffer.vk_command_buffer(),
                swapchain_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                staging_buffer,
                &[buffer_image_copy],
            );
        }

        // End and submit command buffer with fence (async!)
        command_buffer.end_single_time_command();

        unsafe {
            // Submit with fence for async completion
            let command_buffers = [command_buffer.vk_command_buffer()];
            let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);

            self.context
                .device
                .queue_submit(self.context.gfx_queue.vk_queue(), &[submit_info], fence)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!("Failed to submit queue: {}", e))
                })?;
        }

        // Store pending readback for later retrieval
        self.pending_readback = Some(PendingReadback {
            frame,
            fence,
            command_buffer,
            staging_buffer,
            staging_allocation,
            buffer_size,
        });

        Ok(())
    }

    /// Check if the pending async readback is complete and return the data.
    ///
    /// # Returns
    /// * `Ok(Some((frame, data)))` - Readback complete, returns frame number and image data
    /// * `Ok(None)` - Readback not complete yet or no readback pending
    /// * `Err(RendererError)` - Readback failed
    pub fn check_pending_readback(&mut self) -> Result<Option<(usize, Vec<u8>)>, RendererError> {
        // Take ownership to avoid borrow issues
        if let Some(readback) = self.pending_readback.take() {
            unsafe {
                // Check if fence is signaled (readback complete)
                match self.context.device.get_fence_status(readback.fence) {
                    Ok(true) => {
                        // Fence signaled - readback is complete!
                        let mapped_ptr = self.context.map_buffer(&readback.staging_allocation);
                        let data =
                            std::slice::from_raw_parts(mapped_ptr, readback.buffer_size as usize);
                        let result = data.to_vec();
                        let frame = readback.frame;

                        // Cleanup - use CommandBuffer's return_to_pool method
                        readback.command_buffer.return_to_pool();
                        self.context.device.destroy_fence(readback.fence, None);
                        self.context
                            .free_buffer(readback.staging_buffer, readback.staging_allocation);

                        log::debug!("Frame {} readback complete", frame);
                        Ok(Some((frame, result)))
                    }
                    Ok(false) => {
                        // Still processing - put it back
                        log::debug!("Frame {} readback not ready yet", readback.frame);
                        self.pending_readback = Some(readback);
                        Ok(None)
                    }
                    Err(e) => {
                        // Error checking fence status - put it back
                        log::warn!("Failed to check fence for frame {}: {}", readback.frame, e);
                        self.pending_readback = Some(readback);
                        Err(RendererError::InitializationFailed(format!(
                            "Failed to check fence status: {}",
                            e
                        )))
                    }
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Wait for any pending async readback to complete and return the data.
    ///
    /// This is useful during shutdown to ensure all readbacks complete before
    /// destroying resources like the swapchain.
    ///
    /// # Returns
    /// * `Ok(Some((frame, data)))` - Readback was pending and is now complete
    /// * `Ok(None)` - No readback was pending
    /// * `Err(RendererError)` - Failed to wait for or complete readback
    pub fn wait_for_pending_readback(&mut self) -> Result<Option<(usize, Vec<u8>)>, RendererError> {
        if let Some(readback) = self.pending_readback.take() {
            unsafe {
                // Wait for the fence to signal
                log::debug!(
                    "Waiting for pending readback (frame {}) to complete",
                    readback.frame
                );
                let _ = self
                    .context
                    .device
                    .wait_for_fences(&[readback.fence], true, u64::MAX);

                // Fence signaled - readback is complete!
                let mapped_ptr = self.context.map_buffer(&readback.staging_allocation);
                let data = std::slice::from_raw_parts(mapped_ptr, readback.buffer_size as usize);
                let result = data.to_vec();
                let frame = readback.frame;

                // Cleanup - use CommandBuffer's return_to_pool method
                readback.command_buffer.return_to_pool();
                self.context.device.destroy_fence(readback.fence, None);
                self.context
                    .free_buffer(readback.staging_buffer, readback.staging_allocation);

                log::debug!("Pending readback (frame {}) complete", frame);
                Ok(Some((frame, result)))
            }
        } else {
            Ok(None)
        }
    }

    /// Synchronous readback (kept for backwards compatibility, but not recommended)
    ///
    /// This is the old synchronous version that stalls the GPU.
    /// Use `queue_async_readback()` + `check_pending_readback()` instead
    /// to avoid masking synchronization issues.
    pub fn readback_swapchain_image(&self) -> Result<Option<Vec<u8>>, RendererError> {
        // This method is now deprecated - the async version should be used instead
        log::warn!(
            "readback_swapchain_image() is synchronous and may mask race conditions. Use queue_async_readback() + check_pending_readback() instead."
        );
        Ok(None)
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
            if let Some(memory) = self.color_memory.take()
                && let Ok(mut allocator) = self.context.allocator.try_borrow_mut()
            {
                allocator.free(memory).ok();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::{ImageFormat, TextureDescriptor};

    #[test]
    fn test_ui_font_atlas_format() {
        // Test that the font atlas texture descriptor uses SRGB format
        // Font atlas should use SRGB format for proper color reproduction
        let desc = TextureDescriptor::rgba8_srgb(512, 512);

        // Font atlas should use SRGB format
        assert_eq!(
            desc.format,
            ImageFormat::R8G8B8A8Srgb,
            "Font atlas must use SRGB format for correct color rendering"
        );
    }

    #[test]
    fn test_texture_descriptor_format_difference() {
        // Demonstrate the difference between SRGB and UNORM formats
        let srgb_desc = TextureDescriptor::rgba8_srgb(256, 256);
        let unorm_desc = TextureDescriptor::rgba8_unorm(256, 256);

        assert_eq!(srgb_desc.format, ImageFormat::R8G8B8A8Srgb);
        assert_eq!(unorm_desc.format, ImageFormat::R8G8B8A8Unorm);

        // Both have same dimensions
        assert_eq!(srgb_desc.width, unorm_desc.width);
        assert_eq!(srgb_desc.height, unorm_desc.height);
    }
}
