use crate::backend::traits::GpuBackend;
use crate::texture::ImageFormat;

pub trait GpuBuffer: Sized + Send + Sync {
    fn size(&self) -> u64;
    fn map(&self) -> *mut u8;
    fn unmap(&self);
    fn flush(&self, offset: u64, size: u64);
    fn gpu_address(&self) -> u64;
}

pub trait GpuImage: Sized + Send + Sync {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn format(&self) -> ImageFormat;
    fn mip_levels(&self) -> u32;
}

pub trait GpuImageView<B: GpuBackend>: Sized + Send + Sync {
    fn image(&self) -> &B::Image;
}

pub trait GpuGraphicsPipeline: Clone + Send + Sync {}

pub trait GpuComputePipeline: Clone + Send + Sync {
    fn workgroup_size(&self) -> [u32; 3];
}

pub trait GpuSampler: Clone + Send + Sync {}

pub trait GpuFence: Send + Sync {
    fn is_signaled(&self) -> bool;
}

pub trait GpuEvent: Send + Sync {}
