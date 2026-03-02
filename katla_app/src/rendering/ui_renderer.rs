//! UI Renderer implementation.
//!
//! Provides a complete UI rendering solution that owns all UI-specific GPU resources.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::ui_material::UiMaterial;
use katla_gfx::sync::{
    VkBuffer, VkDescriptorSet, VkDescriptorSetLayout, VkImageView, VkSampler,
};
use katla_gfx::{
    DescriptorSetBuilder, DescriptorSetLayoutBuilder, DescriptorType, Extent2D, FrameBuffer,
    IndexBuffer, IndexType, Offset2D, PassExecutionContext, PipelineHandle, Rect2D, ShaderStages,
    TextureHandle, TextureManager, UniformBuffer, VertexBuffer, VulkanContext, VulkanRenderer,
};

/// A single draw command for UI rendering.
#[derive(Debug, Clone)]
pub struct UiDrawCommand {
    /// Index offset in the index buffer.
    pub index_offset: u32,
    /// Number of indices to draw.
    pub index_count: u32,
    /// Clip rectangle (scissor) for this command.
    pub clip_rect: [f32; 4],
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
        VkBuffer(self.vertex_buffer.borrow().object())
    }

    fn index_buffer(&self) -> VkBuffer {
        VkBuffer(self.index_buffer.borrow().object())
    }
}

/// UI-specific textures managed via TextureManager.
struct UITextures {
    font_texture_handle: TextureHandle,
    white_texture_handle: TextureHandle,
    font_texture_view: VkImageView,
    white_texture_view: VkImageView,
    sampler: VkSampler,
    uniform_buffer: UniformBuffer<[f32; 4]>,
    descriptor_set_layout: VkDescriptorSetLayout,
    descriptor_set: katla_gfx::DescriptorSet,
    atlas_width: u32,
    atlas_height: u32,
    external_textures: HashMap<u64, VkImageView>,
    texture_manager: RefCell<TextureManager>,
}

impl UITextures {
    fn new(
        context: Rc<VulkanContext>,
        mut texture_manager: TextureManager,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Result<Self, katla_gfx::RendererError> {
        let sampler = context.create_sampler_clamp_to_edge()?;

        let white_pixels = [255u8, 255, 255, 255];
        let white_texture_handle = texture_manager.create_solid(white_pixels);
        let white_texture_view = texture_manager
            .get_view(white_texture_handle)
            .expect("White texture not found");

        let white_atlas = vec![255u8; (atlas_width * atlas_height * 4) as usize];
        let font_texture_handle =
            texture_manager.create_rgba(atlas_width, atlas_height, &white_atlas);
        let font_texture_view = texture_manager
            .get_view(font_texture_handle)
            .expect("Font texture not found");

        let descriptor_set_layout = DescriptorSetLayoutBuilder::new()
            .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
            .add_binding(1, DescriptorType::Sampler, ShaderStages::FRAGMENT)
            .add_binding(3, DescriptorType::UniformBuffer, ShaderStages::VERTEX)
            .build(&context)?;

        let descriptor_set_layout_raw: VkDescriptorSetLayout = descriptor_set_layout.into();

        let uniform_buffer = UniformBuffer::<[f32; 4]>::new(context.clone())?;
        uniform_buffer.write(&[1920.0, 1080.0, 0.0, 0.0]);

        let descriptor_set = DescriptorSetBuilder::new(&context)
            .sampled_image(0, font_texture_view)
            .sampler(1, sampler)
            .uniform_buffer(3, &uniform_buffer)
            .build(descriptor_set_layout)?;

        Ok(Self {
            font_texture_handle,
            white_texture_handle,
            font_texture_view,
            white_texture_view,
            sampler,
            uniform_buffer,
            descriptor_set_layout: descriptor_set_layout_raw,
            descriptor_set,
            atlas_width,
            atlas_height,
            external_textures: HashMap::new(),
            texture_manager: RefCell::new(texture_manager),
        })
    }

    fn descriptor_set(&self) -> VkDescriptorSet {
        self.descriptor_set.wrapped()
    }

    fn update_screen_size(&self, width: f32, height: f32) {
        self.uniform_buffer.write(&[width, height, 0.0, 0.0]);
    }

    fn update_font_atlas(&self, pixels: &[u8]) -> bool {
        if pixels.len() != (self.atlas_width * self.atlas_height * 4) as usize {
            return false;
        }
        if let Some(texture) = self
            .texture_manager
            .borrow_mut()
            .get_texture_mut(self.font_texture_handle)
        {
            texture.update_data(pixels);
            true
        } else {
            log::warn!("UITextures: font texture not found in manager");
            false
        }
    }

    fn resize_font_atlas(&mut self, width: u32, height: u32, pixels: &[u8]) -> bool {
        self.atlas_width = width;
        self.atlas_height = height;
        let mut manager = self.texture_manager.borrow_mut();
        if let Some(texture) = manager.get_texture_mut(self.font_texture_handle) {
            texture.resize(width, height, pixels);
            if let Some(view) = manager.get_view(self.font_texture_handle) {
                self.font_texture_view = view;
                // Update descriptor set with the new font atlas image view
                self.descriptor_set.update_sampled_image(0, view);
            }
            true
        } else {
            log::warn!("UITextures: font texture not found in manager");
            false
        }
    }

    fn set_external_texture(&mut self, texture_id: u64, image_view: VkImageView) {
        self.external_textures.insert(texture_id, image_view);
    }

    fn get_image_view(&self, texture_id: u64) -> VkImageView {
        if texture_id == katla_ui::TextureId::FONT_ATLAS.0 {
            self.font_texture_view.into()
        } else if texture_id == katla_ui::TextureId::NONE.0 {
            self.white_texture_view.into()
        } else if let Some(&view) = self.external_textures.get(&texture_id) {
            log::trace!("UI found external texture {}", texture_id);
            view.into()
        } else {
            log::warn!("UI texture {} not found, using white fallback", texture_id);
            self.white_texture_view.into()
        }
    }
}

/// UI Renderer that owns all UI-specific GPU resources.
pub struct UIRenderer {
    buffers: FrameBuffer<UIBuffers>,
    textures: UITextures,
    pipeline: PipelineHandle,
}

impl UIRenderer {
    pub fn new(
        renderer: &mut VulkanRenderer,
        vertex_capacity: u64,
        index_capacity: u64,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Result<Self, katla_gfx::RendererError> {
        let context = renderer.context.clone();
        let material_cache = renderer.material_cache.clone();

        let texture_manager = TextureManager::new(context.clone())?;

        let buffers =
            FrameBuffer::new(|_| UIBuffers::new(context.clone(), vertex_capacity, index_capacity));
        let textures = UITextures::new(context, texture_manager, atlas_width, atlas_height)?;

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

    pub fn update_font_atlas(&mut self, pixels: &[u8]) -> bool {
        self.textures.update_font_atlas(pixels)
    }

    pub fn resize_font_atlas(&mut self, width: u32, height: u32, pixels: &[u8]) -> bool {
        self.textures.resize_font_atlas(width, height, pixels)
    }

    pub fn update_screen_size(&self, width: f32, height: f32) {
        self.textures.update_screen_size(width, height);
    }

    pub fn register_texture(&mut self, texture_id: u64, image_view: VkImageView) {
        self.textures.set_external_texture(texture_id, image_view);
    }

    pub fn register_texture_handle(
        &mut self,
        texture_id: u64,
        handle: TextureHandle,
        texture_manager: &TextureManager,
    ) -> bool {
        if let Some(view) = texture_manager.get_view(handle) {
            self.textures.set_external_texture(texture_id, view);
            true
        } else {
            log::warn!(
                "TextureHandle {:?} not found in TextureManager for texture_id {}",
                handle,
                texture_id
            );
            false
        }
    }

    pub fn draw(&self, ctx: &PassExecutionContext, draw_data: &UiDrawData) -> bool {
        if draw_data.vertex_data.is_empty() || draw_data.index_data.is_empty() {
            return true;
        }

        let ui_buffer = self.buffers.current_mut(ctx.frame_index());

        if !ui_buffer.update_vertices(&draw_data.vertex_data) {
            return false;
        }
        if !ui_buffer.update_indices(&draw_data.index_data) {
            return false;
        }

        let Some(pipeline) = ctx.get_pipeline(self.pipeline) else {
            log::error!("UI pipeline handle {:?} not found in cache", self.pipeline);
            return false;
        };

        ctx.bind_graphics_pipeline(pipeline);
        ctx.bind_graphics_descriptor_set_at(pipeline, self.textures.descriptor_set(), 0);
        ctx.bind_vertex_buffers(0, &[ui_buffer.vertex_buffer()], &[0]);
        ctx.bind_index_buffer(ui_buffer.index_buffer(), 0, IndexType::Uint32);

        let pipeline_layout = pipeline.vk_layout();

        for cmd in &draw_data.commands {
            ctx.set_scissor(&Rect2D {
                offset: Offset2D {
                    x: cmd.clip_rect[0] as i32,
                    y: cmd.clip_rect[1] as i32,
                },
                extent: Extent2D {
                    width: cmd.clip_rect[2] as u32,
                    height: cmd.clip_rect[3] as u32,
                },
            });

            unsafe {
                let image_view = self.textures.get_image_view(cmd.texture_id);
                ctx.push_texture_descriptor(pipeline_layout, image_view.into());
            }

            ctx.draw_indexed(cmd.index_count, 1, cmd.index_offset, 0, 0);
        }

        true
    }
}
