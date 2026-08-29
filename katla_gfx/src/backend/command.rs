use crate::backend::traits::GpuBackend;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::ImageFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexType {
    Uint8,
    Uint16,
    Uint32,
}

impl IndexType {
    #[inline]
    pub fn size(&self) -> u32 {
        match self {
            IndexType::Uint8 => 1,
            IndexType::Uint16 => 2,
            IndexType::Uint32 => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShaderStages {
    pub vertex: bool,
    pub fragment: bool,
    pub compute: bool,
}

impl ShaderStages {
    pub const NONE: Self = Self {
        vertex: false,
        fragment: false,
        compute: false,
    };
    pub const VERTEX: Self = Self {
        vertex: true,
        fragment: false,
        compute: false,
    };
    pub const FRAGMENT: Self = Self {
        vertex: false,
        fragment: true,
        compute: false,
    };
    pub const COMPUTE: Self = Self {
        vertex: false,
        fragment: false,
        compute: true,
    };
    pub const VERTEX_FRAGMENT: Self = Self {
        vertex: true,
        fragment: true,
        compute: false,
    };
    pub const ALL: Self = Self {
        vertex: true,
        fragment: true,
        compute: true,
    };

    #[inline]
    pub fn is_empty(&self) -> bool {
        !self.vertex && !self.fragment && !self.compute
    }
}

pub struct ColorAttachmentInfo<B: GpuBackend> {
    pub view: B::ImageView,
    pub load_op: LoadOp,
    pub store_op: StoreOp,
    pub clear_value: ClearValue,
}

pub struct DepthAttachmentInfo<B: GpuBackend> {
    pub view: B::ImageView,
    pub load_op: LoadOp,
    pub store_op: StoreOp,
    pub clear_value: ClearValue,
    pub format: ImageFormat,
}

pub struct RenderPassInfo<B: GpuBackend> {
    pub color_attachments: Vec<ColorAttachmentInfo<B>>,
    pub depth_attachment: Option<DepthAttachmentInfo<B>>,
    /// Deterministic pass name applied to the underlying Metal encoder. Must be
    /// a compile-time constant so GPU captures are diffable across runs.
    pub debug_label: Option<&'static str>,
}

impl<B: GpuBackend> RenderPassInfo<B> {
    /// Unlabeled pass info with the given attachments (tests + internal call sites).
    pub(crate) fn unlabeled(
        color_attachments: Vec<ColorAttachmentInfo<B>>,
        depth_attachment: Option<DepthAttachmentInfo<B>>,
    ) -> Self {
        Self {
            color_attachments,
            depth_attachment,
            debug_label: None,
        }
    }
}

pub struct BufferImageCopy {
    pub buffer_offset: u64,
    pub image_width: u32,
    pub image_height: u32,
    pub image_depth: u32,
    pub mip_level: u32,
    pub base_array_layer: u32,
    pub layer_count: u32,
}

pub trait GpuCommandBuffer<B: GpuBackend>: Sized {
    fn begin(&mut self);
    fn end(&mut self);
    fn submit(&self, context: &B::Context);
    fn begin_render_pass(&mut self, desc: RenderPassInfo<B>) -> B::RenderEncoder;
    fn begin_compute_pass(&mut self) -> B::ComputeEncoder;
    fn begin_blit_pass(&mut self) -> B::BlitEncoder;
    /// Compute pass carrying a deterministic label for GPU captures and
    /// encoder-execution diagnostics.
    fn begin_compute_pass_with_label(&mut self, label: &'static str) -> B::ComputeEncoder;
    /// Blit pass carrying a deterministic label for GPU captures and
    /// encoder-execution diagnostics.
    fn begin_blit_pass_with_label(&mut self, label: &'static str) -> B::BlitEncoder;
    fn copy_buffer_to_texture(
        &mut self,
        src: &B::Buffer,
        dst: &B::Image,
        regions: &[BufferImageCopy],
    );
}

pub trait GpuRenderEncoder<B: GpuBackend>: Sized {
    fn end_encoding(self);
    fn bind_graphics_pipeline(&mut self, pipeline: &B::GraphicsPipeline);
    fn bind_vertex_buffer(&mut self, buffer: &B::Buffer, offset: u64, index: u32);
    fn bind_index_buffer(&mut self, buffer: &B::Buffer, offset: u64, index_type: IndexType);
    fn bind_storage_buffer(
        &mut self,
        buffer: &B::Buffer,
        offset: u64,
        index: u32,
        stages: ShaderStages,
    );
    fn bind_texture(&mut self, view: &B::ImageView, index: u32, stages: ShaderStages);
    fn bind_sampler(&mut self, sampler: &B::Sampler, index: u32, stages: ShaderStages);
    fn set_push_constants(&mut self, data: &[u8], index: u32, stages: ShaderStages);
    fn set_viewport(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        min_depth: f32,
        max_depth: f32,
    );
    fn set_scissor(&mut self, x: u32, y: u32, width: u32, height: u32);
    fn set_depth_bias(&mut self, bias: f32, slope: f32, clamp: f32);
    fn draw(
        &mut self,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    );
    fn draw_indexed(
        &mut self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    );
    fn set_stencil_reference_value(&mut self, reference: u32);
}

pub trait GpuComputeEncoder<B: GpuBackend>: Sized {
    fn end_encoding(self);
    fn bind_compute_pipeline(&mut self, pipeline: &B::ComputePipeline);
    fn bind_storage_buffer(&mut self, buffer: &B::Buffer, offset: u64, index: u32);
    fn bind_texture(&mut self, view: &B::ImageView, index: u32);
    fn bind_sampler(&mut self, sampler: &B::Sampler, index: u32);
    fn set_push_constants(&mut self, data: &[u8], index: u32);
    fn dispatch(&mut self, group_count_x: u32, group_count_y: u32, group_count_z: u32);
}

pub trait GpuBlitEncoder<B: GpuBackend>: Sized {
    fn end_encoding(self);
    fn copy_buffer_to_buffer(
        &mut self,
        src: &B::Buffer,
        src_offset: u64,
        dst: &B::Buffer,
        dst_offset: u64,
        size: u64,
    );
    fn copy_buffer_to_texture(
        &mut self,
        src: &B::Buffer,
        dst: &B::Image,
        regions: &[BufferImageCopy],
    );
    /// Copy the full base-mip surface of `src` into `dst` (same format and extent).
    fn copy_texture_to_texture(&mut self, src: &B::Image, dst: &B::Image);
}
