use crate::backend::command::{
    GpuBlitEncoder, GpuCommandBuffer, GpuComputeEncoder, GpuRenderEncoder,
};
use crate::backend::resource::{
    GpuBuffer, GpuComputePipeline, GpuEvent, GpuFence, GpuGraphicsPipeline, GpuImage, GpuImageView,
    GpuSampler,
};

pub trait GpuBackend: Sized + 'static {
    type Context: GpuContext<Self>;
    type CommandBuffer: GpuCommandBuffer<Self>;
    type RenderEncoder: GpuRenderEncoder<Self>;
    type ComputeEncoder: GpuComputeEncoder<Self>;
    type BlitEncoder: GpuBlitEncoder<Self>;
    type Image: GpuImage;
    type ImageView: GpuImageView<Self>;
    type Buffer: GpuBuffer;
    type GraphicsPipeline: GpuGraphicsPipeline;
    type ComputePipeline: GpuComputePipeline;
    type Sampler: GpuSampler;
    type Fence: GpuFence;
    type Event: GpuEvent;

    fn name() -> &'static str;
}

pub trait GpuContext<B: GpuBackend>: Sized + Send + Sync {}
