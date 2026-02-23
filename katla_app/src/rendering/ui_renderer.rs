//! UI Renderer implementation.
//!
//! Provides a complete UI rendering solution that owns all UI-specific GPU resources.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ash::vk;
use katla_vulkan::{
    DescriptorSetBuilder, DescriptorSetLayoutBuilder, DescriptorType, Extent2D, FrameBuffer,
    IndexBuffer, IndexType, MaterialPipeline, MaterialPipelineCache, Offset2D, PassExecutionContext, Rect2D,
    ShaderStages, Texture, UniformBuffer, VertexBuffer, VkBuffer, VkDescriptorSet, VkImageView,
    VkSampler, VulkanContext,
};

use super::ui_material::UiMaterial;

/// A single draw command for UI rendering.
#[derive(Debug, Clone)]
pub struct UiDrawCommand {
    /// Index offset in the index buffer.
    pub index_offset: u32,
    /// Number of indices to draw.
    pub index_count: u32,
    /// Clip rectangle (scissor) for this command.
    pub clip_rect: [f32; 4], // [x, y, width, height]
    /// Texture to use (see katla_ui::TextureId: 0=NONE, 1=FONT_ATLAS, 2=VIEWPORT, 100+=custom)
    pub texture_id: u64,
}

/// UI draw data for rendering.
#[derive(Clone, Debug)]
pub struct UiDrawData {
    pub vertex_data: Vec<u8>,
    pub index_data: Vec<u8>,
    pub screen_size: [f32; 2],
    pub commands: Vec<UiDrawCommand>,
}

/// Persistent buffers for UI rendering.
/// One set per frame in flight to avoid synchronization issues.
struct UIBuffers {
    vertex_buffer: RefCell<VertexBuffer>,
    index_buffer: RefCell<IndexBuffer>,
    vertex_capacity: u64,
    index_capacity: u64,
}

impl UIBuffers {
    fn new(context: Rc<VulkanContext>, vertex_capacity: u64, index_capacity: u64) -> Self {
        let vertex_buffer = VertexBuffer::new(context.clone(), vertex_capacity, 0);
        let index_buffer = IndexBuffer::new(context, index_capacity, IndexType::Uint32, 0);

        Self {
            vertex_buffer: RefCell::new(vertex_buffer),
            index_buffer: RefCell::new(index_buffer),
            vertex_capacity,
            index_capacity,
        }
    }

    /// Update vertex data. Returns false if data exceeds capacity.
    fn update_vertices(&self, data: &[u8]) -> bool {
        if data.len() as u64 > self.vertex_capacity {
            log::warn!(
                "UIBuffers: vertex data ({}) exceeds capacity ({})",
                data.len(),
                self.vertex_capacity
            );
            return false;
        }
        self.vertex_buffer.borrow_mut().upload_data(data);
        true
    }

    /// Update index data. Returns false if data exceeds capacity.
    fn update_indices(&self, data: &[u8]) -> bool {
        if data.len() as u64 > self.index_capacity {
            log::warn!(
                "UIBuffers: index data ({}) exceeds capacity ({})",
                data.len(),
                self.index_capacity
            );
            return false;
        }
        self.index_buffer.borrow_mut().upload_data(data);
        true
    }

    fn vertex_buffer(&self) -> VkBuffer {
        VkBuffer::new(self.vertex_buffer.borrow().object())
    }

    fn index_buffer(&self) -> VkBuffer {
        VkBuffer::new(self.index_buffer.borrow().object())
    }
}

/// UI texture resources for font atlas and fallback.
struct UITextures {
    font_texture: Rc<Texture>,
    white_texture: Rc<Texture>,
    sampler: VkSampler,
    uniform_buffer: UniformBuffer<[f32; 4]>,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_set: katla_vulkan::DescriptorSet,
    atlas_width: u32,
    atlas_height: u32,
    external_textures: HashMap<u64, vk::ImageView>,
    context: Rc<VulkanContext>,
}

impl UITextures {
    fn new(
        context: Rc<VulkanContext>,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Result<Self, vk::Result> {
        let sampler = context.create_sampler_clamp_to_edge()?;

        let white_pixels = [255u8, 255, 255, 255];
        let white_texture = Rc::new(Texture::create_image_rgb(context.clone(), 1, 1, &white_pixels));

        let white_atlas = vec![255u8; (atlas_width * atlas_height * 4) as usize];
        let mut font_texture = Rc::new(Texture::create_image_rgb(
            context.clone(),
            atlas_width,
            atlas_height,
            &white_atlas,
        ));

        // Create static descriptor set layout (font texture + sampler + uniform)
        let descriptor_set_layout = DescriptorSetLayoutBuilder::new()
            .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
            .add_binding(1, DescriptorType::Sampler, ShaderStages::FRAGMENT)
            .add_binding(3, DescriptorType::UniformBuffer, ShaderStages::VERTEX)
            .build(&context)?;

        let descriptor_set_layout_raw: vk::DescriptorSetLayout = descriptor_set_layout.into();

        // Create uniform buffer for screen size
        let uniform_buffer = UniformBuffer::<[f32; 4]>::new(context.clone())?;
        uniform_buffer.write(&[1920.0, 1080.0, 0.0, 0.0]);

        // Build descriptor set with mixed bindings using unified builder
        let descriptor_set = DescriptorSetBuilder::new(&context)
            .sampled_image(0, font_texture.image_view)
            .sampler(1, sampler)
            .uniform_buffer(3, &uniform_buffer)
            .build(descriptor_set_layout)?;

        // Register the font texture with the descriptor set for auto-updates
        if let Some(texture) = Rc::get_mut(&mut font_texture) {
            texture.register_for_descriptor(descriptor_set.wrapped(), 0);
        }

        Ok(Self {
            font_texture,
            white_texture,
            sampler,
            uniform_buffer,
            descriptor_set_layout: descriptor_set_layout_raw,
            descriptor_set,
            atlas_width,
            atlas_height,
            external_textures: HashMap::new(),
            context,
        })
    }

    fn descriptor_set(&self) -> VkDescriptorSet {
        self.descriptor_set.wrapped()
    }

    fn update_screen_size(&self, width: f32, height: f32) {
        self.uniform_buffer.write(&[width, height, 0.0, 0.0]);
    }

    fn update_font_atlas(&mut self, pixels: &[u8]) -> bool {
        if pixels.len() != (self.atlas_width * self.atlas_height * 4) as usize {
            return false;
        }
        // Try to update in-place (no descriptor update needed - same image view)
        if let Some(texture) = Rc::get_mut(&mut self.font_texture) {
            texture.update_data(pixels);
        } else {
            // Multiple references exist, need to recreate and update descriptor
            log::debug!("UITextures: font texture has multiple refs, recreating");
            let mut new_texture = Texture::create_image_rgb(
                self.context.clone(),
                self.atlas_width,
                self.atlas_height,
                pixels,
            );
            new_texture.register_for_descriptor(self.descriptor_set(), 0);
            self.font_texture = Rc::new(new_texture);
        }
        true
    }

    fn resize_font_atlas(&mut self, width: u32, height: u32, pixels: &[u8]) -> bool {
        self.atlas_width = width;
        self.atlas_height = height;
        // Try to resize in-place (auto-updates registered descriptors)
        if let Some(texture) = Rc::get_mut(&mut self.font_texture) {
            texture.resize(width, height, pixels);
        } else {
            // Multiple references exist, need to recreate and update descriptor
            log::debug!("UITextures: font texture has multiple refs, recreating for resize");
            let mut new_texture = Texture::create_image_rgb(
                self.context.clone(),
                width,
                height,
                pixels,
            );
            new_texture.register_for_descriptor(self.descriptor_set(), 0);
            self.font_texture = Rc::new(new_texture);
        }
        true
    }

    fn set_external_texture(&mut self, texture_id: u64, image_view: VkImageView) {
        self.external_textures.insert(texture_id, image_view.into());
    }

    fn get_image_view(&self, texture_id: u64) -> vk::ImageView {
        // Texture ID 0 = NONE (solid color), 1 = FONT_ATLAS
        // See katla_ui::TextureId enum
        if texture_id == katla_ui::TextureId::FONT_ATLAS.0 {
            self.font_texture.image_view.into()
        } else if texture_id == katla_ui::TextureId::NONE.0 {
            // Solid color rendering - use white texture
            self.white_texture.image_view.into()
        } else if let Some(&view) = self.external_textures.get(&texture_id) {
            log::trace!("UI found external texture {}", texture_id);
            view
        } else {
            log::warn!("UI texture {} not found, using white fallback", texture_id);
            self.white_texture.image_view.into()
        }
    }
}

impl Drop for UITextures {
    fn drop(&mut self) {
        unsafe {
            self.context.destroy_sampler(self.sampler);
            // uniform_buffer and descriptor_set are dropped automatically
            self.context.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }
}

/// UI Renderer that owns all UI-specific GPU resources.
pub struct UIRenderer {
    buffers: FrameBuffer<UIBuffers>,
    textures: UITextures,
    pipeline: Rc<RefCell<MaterialPipeline>>,
}

impl UIRenderer {
    /// Create a new UI renderer.
    ///
    /// The renderer owns its pipeline internally, created from UiMaterial::default()
    /// using the provided material cache.
    pub fn new(
        context: Rc<VulkanContext>,
        material_cache: Rc<RefCell<MaterialPipelineCache>>,
        vertex_capacity: u64,
        index_capacity: u64,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Result<Self, vk::Result> {
        let buffers = FrameBuffer::new(|_| {
            UIBuffers::new(context.clone(), vertex_capacity, index_capacity)
        });

        let textures = UITextures::new(context, atlas_width, atlas_height)?;

        // Create pipeline internally from UiMaterial config
        let ui_material = UiMaterial::default();
        let pipeline = material_cache
            .borrow_mut()
            .get_or_create(&ui_material)
            .expect("Failed to create UI pipeline");

        Ok(Self {
            buffers,
            textures,
            pipeline,
        })
    }

    /// Update font atlas texture.
    pub fn update_font_atlas(&mut self, pixels: &[u8]) -> bool {
        self.textures.update_font_atlas(pixels)
    }

    /// Resize font atlas texture.
    pub fn resize_font_atlas(&mut self, width: u32, height: u32, pixels: &[u8]) -> bool {
        self.textures.resize_font_atlas(width, height, pixels)
    }

    /// Update screen size uniform.
    pub fn update_screen_size(&self, width: f32, height: f32) {
        self.textures.update_screen_size(width, height);
    }

    /// Register an external texture (viewport, thumbnail).
    pub fn register_texture(&mut self, texture_id: u64, image_view: VkImageView) {
        self.textures.set_external_texture(texture_id, image_view);
    }

    /// Draw UI using the render context.
    /// Returns false if buffer capacity is exceeded.
    pub fn draw(&self, ctx: &PassExecutionContext, draw_data: &UiDrawData) -> bool {
        if draw_data.vertex_data.is_empty() || draw_data.index_data.is_empty() {
            return true; // Empty data is valid, just nothing to draw
        }

        // Get frame index from context (passed through render graph)
        let ui_buffer = self.buffers.current_mut(ctx.frame_index());

        if !ui_buffer.update_vertices(&draw_data.vertex_data) {
            return false;
        }
        if !ui_buffer.update_indices(&draw_data.index_data) {
            return false;
        }

        let pipeline = self.pipeline.borrow();
        ctx.bind_graphics_pipeline(&pipeline);
        ctx.bind_graphics_descriptor_set_at(&pipeline, self.textures.descriptor_set(), 0);
        ctx.bind_vertex_buffers(0, &[ui_buffer.vertex_buffer()], &[0]);
        ctx.bind_index_buffer(ui_buffer.index_buffer(), 0, IndexType::Uint32);

        // Use the pipeline's layout (created with both set 0 and set 1)
        let pipeline_layout = pipeline.vk_layout();
        drop(pipeline);

        for cmd in &draw_data.commands {
            ctx.set_scissor(&Rect2D {
                offset: Offset2D { x: cmd.clip_rect[0] as i32, y: cmd.clip_rect[1] as i32 },
                extent: Extent2D { width: cmd.clip_rect[2] as u32, height: cmd.clip_rect[3] as u32 },
            });

            // SAFETY: push_texture_descriptor requires a valid pipeline_layout created
            // with push descriptor support, and a valid image_view. Both invariants
            // are maintained by UITextures.
            unsafe {
                let image_view = self.textures.get_image_view(cmd.texture_id);
                ctx.push_texture_descriptor(pipeline_layout, image_view);
            }

            ctx.draw_indexed(cmd.index_count, 1, cmd.index_offset, 0, 0);
        }

        true
    }
}
