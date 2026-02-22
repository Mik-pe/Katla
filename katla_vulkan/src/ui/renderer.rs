//! UI Renderer implementation.
//!
//! Provides a complete UI rendering solution that owns all UI-specific GPU resources.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ash::vk;

use crate::context::VulkanContext;
use crate::material::MaterialPipeline;
use crate::render_graph::types::{Extent2D, Offset2D, Rect2D, ShaderStages};
use crate::render_graph::PassExecutionContext;
use crate::sync::{VkBuffer, VkDescriptorSet, VkImageView, VkSampler};
use crate::texture::Texture;
use crate::vulkan::descriptor::DescriptorSetLayoutBuilder;
use crate::vulkan::material::buffer_descriptor::{BufferDescriptorSource, UniformBuffer};
use crate::vulkan::pipeline_state::DescriptorType;
use crate::{IndexBuffer, IndexType, VertexBuffer};

/// A single draw command for UI rendering.
#[derive(Debug, Clone)]
pub struct UiDrawCommand {
    /// Index offset in the index buffer.
    pub index_offset: u32,
    /// Number of indices to draw.
    pub index_count: u32,
    /// Clip rectangle (scissor) for this command.
    pub clip_rect: [f32; 4], // [x, y, width, height]
    /// Texture to use (0 = font atlas, 1 = viewport, 2+ = custom thumbnails)
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
}

impl UIBuffers {
    fn new(context: Rc<VulkanContext>, vertex_capacity: u64, index_capacity: u64) -> Self {
        let vertex_buffer = VertexBuffer::new(context.clone(), vertex_capacity, 0);
        let index_buffer = IndexBuffer::new(context, index_capacity, IndexType::Uint32, 0);

        Self {
            vertex_buffer: RefCell::new(vertex_buffer),
            index_buffer: RefCell::new(index_buffer),
        }
    }

    fn update_vertices(&self, data: &[u8]) -> bool {
        self.vertex_buffer.borrow_mut().upload_data(data);
        true
    }

    fn update_indices(&self, data: &[u8]) -> bool {
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
    push_descriptor_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set: VkDescriptorSet,
    descriptor_pool: vk::DescriptorPool,
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
        unsafe {
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

            // Create push descriptor layout for dynamic textures
            let push_descriptor_layout = DescriptorSetLayoutBuilder::new()
                .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                .with_push_descriptor(true)
                .build(&context)?;

            let set_layouts: Vec<vk::DescriptorSetLayout> = vec![
                descriptor_set_layout.into(),
                push_descriptor_layout.into(),
            ];
            let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
            let pipeline_layout = context.device.create_pipeline_layout(&pipeline_layout_info, None)?;

            let pool_sizes = [
                vk::DescriptorPoolSize { ty: vk::DescriptorType::SAMPLED_IMAGE, descriptor_count: 1 },
                vk::DescriptorPoolSize { ty: vk::DescriptorType::SAMPLER, descriptor_count: 1 },
                vk::DescriptorPoolSize { ty: vk::DescriptorType::UNIFORM_BUFFER, descriptor_count: 1 },
            ];

            let pool_create_info = vk::DescriptorPoolCreateInfo::default()
                .pool_sizes(&pool_sizes)
                .max_sets(1);
            let descriptor_pool = context.device.create_descriptor_pool(&pool_create_info, None)?;

            let alloc_layouts: Vec<vk::DescriptorSetLayout> = vec![descriptor_set_layout.into()];
            let alloc_info = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&alloc_layouts);
            let descriptor_sets = context.device.allocate_descriptor_sets(&alloc_info)?;
            let descriptor_set = descriptor_sets[0];

            // Create uniform buffer for screen size
            let uniform_buffer = UniformBuffer::<[f32; 4]>::new(context.clone())?;
            uniform_buffer.write(&[1920.0, 1080.0, 0.0, 0.0]);

            let font_image_info = vk::DescriptorImageInfo {
                sampler: vk::Sampler::null(),
                image_view: font_texture.image_view.into(),
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            };
            let sampler_info = vk::DescriptorImageInfo {
                sampler: sampler.into(),
                image_view: vk::ImageView::null(),
                image_layout: vk::ImageLayout::UNDEFINED,
            };
            let uniform_buffer_info = vk::DescriptorBufferInfo {
                buffer: uniform_buffer.buffer(),
                offset: 0,
                range: uniform_buffer.size(),
            };

            let font_image_infos = [font_image_info];
            let sampler_infos = [sampler_info];
            let uniform_buffer_infos = [uniform_buffer_info];

            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&font_image_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&sampler_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&uniform_buffer_infos),
            ];
            context.device.update_descriptor_sets(&writes, &[]);

            // Register the font texture with the descriptor set for auto-updates
            if let Some(texture) = Rc::get_mut(&mut font_texture) {
                texture.register_for_descriptor(VkDescriptorSet::new(descriptor_set), 0);
            }

            // Convert wrapper layouts to raw for storage (needed for Drop cleanup)
            let descriptor_set_layout_raw: vk::DescriptorSetLayout = descriptor_set_layout.into();
            let push_descriptor_layout_raw: vk::DescriptorSetLayout = push_descriptor_layout.into();

            Ok(Self {
                font_texture,
                white_texture,
                sampler,
                uniform_buffer,
                descriptor_set_layout: descriptor_set_layout_raw,
                push_descriptor_layout: push_descriptor_layout_raw,
                pipeline_layout,
                descriptor_set: VkDescriptorSet::new(descriptor_set),
                descriptor_pool,
                atlas_width,
                atlas_height,
                external_textures: HashMap::new(),
                context,
            })
        }
    }

    fn descriptor_set(&self) -> VkDescriptorSet {
        self.descriptor_set
    }

    fn pipeline_layout(&self) -> vk::PipelineLayout {
        self.pipeline_layout
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
            new_texture.register_for_descriptor(self.descriptor_set, 0);
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
            new_texture.register_for_descriptor(self.descriptor_set, 0);
            self.font_texture = Rc::new(new_texture);
        }
        true
    }

    fn set_external_texture(&mut self, texture_id: u64, image_view: VkImageView) {
        self.external_textures.insert(texture_id, image_view.into());
    }

    fn get_image_view(&self, texture_id: u64) -> vk::ImageView {
        if texture_id == 0 {
            self.font_texture.image_view.into()
        } else if let Some(&view) = self.external_textures.get(&texture_id) {
            view
        } else {
            self.white_texture.image_view.into()
        }
    }
}

impl Drop for UITextures {
    fn drop(&mut self) {
        unsafe {
            self.context.destroy_sampler(self.sampler);
            // uniform_buffer is dropped automatically with its own Drop impl
            self.context.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.context.device.destroy_descriptor_set_layout(self.push_descriptor_layout, None);
            self.context.device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.context.device.destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

/// UI Renderer that owns all UI-specific GPU resources.
///
/// This is created by the application and used with PassExecutionContext to draw UI.
/// VulkanRenderer does NOT own this - the application owns it.
pub struct UIRenderer {
    buffers: Vec<UIBuffers>,
    textures: UITextures,
    pipeline: Rc<RefCell<MaterialPipeline>>,
    frame_index: Cell<usize>,
    frames_in_flight: usize,
}

impl UIRenderer {
    /// Create a new UI renderer.
    pub fn new(
        context: Rc<VulkanContext>,
        pipeline: Rc<RefCell<MaterialPipeline>>,
        vertex_capacity: u64,
        index_capacity: u64,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Result<Self, vk::Result> {
        let frames_in_flight = 2;

        let buffers = (0..frames_in_flight)
            .map(|_| UIBuffers::new(context.clone(), vertex_capacity, index_capacity))
            .collect();

        let textures = UITextures::new(context, atlas_width, atlas_height)?;

        Ok(Self {
            buffers,
            textures,
            pipeline,
            frame_index: Cell::new(0),
            frames_in_flight,
        })
    }

    /// Advance frame index (call after each frame).
    pub fn advance_frame(&self) {
        let next = (self.frame_index.get() + 1) % self.frames_in_flight;
        self.frame_index.set(next);
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
    pub fn draw(&self, ctx: &PassExecutionContext, draw_data: &UiDrawData) {
        if draw_data.vertex_data.is_empty() || draw_data.index_data.is_empty() {
            return;
        }

        let frame_idx = self.frame_index.get();
        let Some(ui_buffer) = self.buffers.get(frame_idx) else {
            return;
        };

        ui_buffer.update_vertices(&draw_data.vertex_data);
        ui_buffer.update_indices(&draw_data.index_data);

        let pipeline = self.pipeline.borrow();
        ctx.bind_graphics_pipeline(&pipeline);
        ctx.bind_graphics_descriptor_set_at(&pipeline, self.textures.descriptor_set(), 0);
        ctx.bind_vertex_buffers(0, &[ui_buffer.vertex_buffer()], &[0]);
        ctx.bind_index_buffer(ui_buffer.index_buffer(), 0, IndexType::Uint32);

        let pipeline_layout = self.textures.pipeline_layout();
        drop(pipeline);

        for cmd in &draw_data.commands {
            ctx.set_scissor(&Rect2D {
                offset: Offset2D { x: cmd.clip_rect[0] as i32, y: cmd.clip_rect[1] as i32 },
                extent: Extent2D { width: cmd.clip_rect[2] as u32, height: cmd.clip_rect[3] as u32 },
            });

            unsafe {
                let image_view = self.textures.get_image_view(cmd.texture_id);
                ctx.push_texture_descriptor(pipeline_layout, image_view);
            }

            ctx.draw_indexed(cmd.index_count, 1, cmd.index_offset, 0, 0);
        }
    }
}
