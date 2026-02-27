use super::types::*;
use super::{RenderGraphBuilder, ResourceId, ResourceKind};
use crate::handle::ImageHandle;

pub struct SwapchainImageResourceBuilder {
    handle: ImageHandle,
    format: ImageFormat,
    extent: Extent2D,
}

impl SwapchainImageResourceBuilder {
    pub fn new(handle: ImageHandle, format: ImageFormat, extent: Extent2D) -> Self {
        Self {
            handle,
            format,
            extent,
        }
    }

    pub fn build(self) -> ResourceKind {
        ResourceKind::ExternalImage {
            handle: self.handle,
            format: self.format,
            extent: self.extent,
        }
    }
}

pub struct OffscreenImageResourceBuilder {
    extent: Extent3D,
    format: ImageFormat,
    usages: Vec<ImageUsage>,
    samples: SampleCount,
    tiling: ImageTiling,
    initial_layout: ImageLayout,
    final_layout: ImageLayout,
}

impl OffscreenImageResourceBuilder {
    pub fn new(extent: Extent3D, format: ImageFormat) -> Self {
        Self {
            extent,
            format,
            usages: Vec::new(),
            samples: SampleCount::Sample1,
            tiling: ImageTiling::Optimal,
            initial_layout: ImageLayout::Undefined,
            final_layout: ImageLayout::ShaderReadOnlyOptimal,
        }
    }

    pub fn color_attachment(mut self) -> Self {
        self.usages.push(ImageUsage::ColorAttachment);
        self.final_layout = ImageLayout::ColorAttachmentOptimal;
        self
    }

    pub fn depth_stencil_attachment(mut self) -> Self {
        self.usages.push(ImageUsage::DepthStencilAttachment);
        self.final_layout = ImageLayout::DepthStencilAttachmentOptimal;
        self
    }

    pub fn sampled(mut self) -> Self {
        self.usages.push(ImageUsage::Sampled);
        self
    }

    pub fn storage(mut self) -> Self {
        self.usages.push(ImageUsage::Storage);
        self
    }

    pub fn transfer_src(mut self) -> Self {
        self.usages.push(ImageUsage::TransferSrc);
        self
    }

    pub fn transfer_dst(mut self) -> Self {
        self.usages.push(ImageUsage::TransferDst);
        self
    }

    pub fn samples(mut self, samples: SampleCount) -> Self {
        self.samples = samples;
        self
    }

    pub fn tiling(mut self, tiling: ImageTiling) -> Self {
        self.tiling = tiling;
        self
    }

    pub fn initial_layout(mut self, layout: ImageLayout) -> Self {
        self.initial_layout = layout;
        self
    }

    pub fn final_layout(mut self, layout: ImageLayout) -> Self {
        self.final_layout = layout;
        self
    }

    pub fn build(self) -> ResourceKind {
        ResourceKind::Image {
            extent: self.extent,
            format: self.format,
            usage: self.usages,
            samples: self.samples,
            tiling: self.tiling,
            initial_layout: self.initial_layout,
            final_layout: self.final_layout,
        }
    }
}

pub struct BufferResourceBuilder {
    size: u64,
    usages: Vec<BufferUsage>,
    device_local: bool,
}

impl BufferResourceBuilder {
    pub fn new(size: u64) -> Self {
        Self {
            size,
            usages: Vec::new(),
            device_local: false,
        }
    }

    pub fn vertex_buffer(mut self) -> Self {
        self.usages.push(BufferUsage::VertexBuffer);
        self
    }

    pub fn index_buffer(mut self) -> Self {
        self.usages.push(BufferUsage::IndexBuffer);
        self
    }

    pub fn uniform_buffer(mut self) -> Self {
        self.usages.push(BufferUsage::UniformBuffer);
        self
    }

    pub fn storage_buffer(mut self) -> Self {
        self.usages.push(BufferUsage::StorageBuffer);
        self
    }

    pub fn transfer_src(mut self) -> Self {
        self.usages.push(BufferUsage::TransferSrc);
        self
    }

    pub fn transfer_dst(mut self) -> Self {
        self.usages.push(BufferUsage::TransferDst);
        self
    }

    pub fn device_local(mut self) -> Self {
        self.device_local = true;
        self
    }

    pub fn build(self) -> ResourceKind {
        let memory_properties = if self.device_local {
            vec![MemoryProperty::DeviceLocal]
        } else {
            vec![MemoryProperty::HostVisible, MemoryProperty::HostCoherent]
        };

        ResourceKind::Buffer {
            size: self.size,
            usage: self.usages,
            memory_properties,
        }
    }
}

pub trait RenderGraphHelper {
    fn add_swapchain_resource(
        &mut self,
        name: &str,
        format: ImageFormat,
        extent: Extent2D,
    ) -> ResourceId;
}

impl RenderGraphHelper for RenderGraphBuilder {
    fn add_swapchain_resource(
        &mut self,
        name: &str,
        format: ImageFormat,
        extent: Extent2D,
    ) -> ResourceId {
        self.add_resource(
            name,
            ResourceKind::ExternalImage {
                handle: ImageHandle::NONE,
                format,
                extent,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_attachment_builder() {
        let builder = OffscreenImageResourceBuilder::new(
            Extent3D::new(1920, 1080, 1),
            ImageFormat::B8G8R8A8Srgb,
        )
        .color_attachment()
        .sampled();

        let resource = builder.build();

        match resource {
            ResourceKind::Image { usage, format, .. } => {
                assert!(usage.contains(&ImageUsage::ColorAttachment));
                assert!(usage.contains(&ImageUsage::Sampled));
                assert_eq!(format, ImageFormat::B8G8R8A8Srgb);
            }
            _ => panic!("Expected Image resource"),
        }
    }

    #[test]
    fn test_depth_attachment_builder() {
        let builder = OffscreenImageResourceBuilder::new(
            Extent3D::new(1920, 1080, 1),
            ImageFormat::D32Sfloat,
        )
        .depth_stencil_attachment();

        let resource = builder.build();

        match resource {
            ResourceKind::Image { usage, format, .. } => {
                assert!(usage.contains(&ImageUsage::DepthStencilAttachment));
                assert_eq!(format, ImageFormat::D32Sfloat);
            }
            _ => panic!("Expected Image resource"),
        }
    }

    #[test]
    fn test_buffer_builder() {
        let builder = BufferResourceBuilder::new(1024)
            .vertex_buffer()
            .device_local();

        let resource = builder.build();

        match resource {
            ResourceKind::Buffer {
                size,
                usage,
                memory_properties,
            } => {
                assert_eq!(size, 1024);
                assert!(usage.contains(&BufferUsage::VertexBuffer));
                assert!(memory_properties.contains(&MemoryProperty::DeviceLocal));
            }
            _ => panic!("Expected Buffer resource"),
        }
    }
}
