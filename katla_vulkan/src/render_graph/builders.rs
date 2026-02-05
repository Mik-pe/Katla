use ash::vk;

use super::types::*;
use super::{RenderGraphBuilder, ResourceId, ResourceKind};

pub struct SwapchainImageResourceBuilder {
    image: vk::Image,
    image_view: vk::ImageView,
    format: ImageFormat,
    extent: Extent2D,
}

impl SwapchainImageResourceBuilder {
    pub fn new(
        image: vk::Image,
        image_view: vk::ImageView,
        format: vk::Format,
        extent: vk::Extent2D,
    ) -> Self {
        Self {
            image,
            image_view,
            format: Self::vk_to_image_format(format),
            extent: Extent2D {
                width: extent.width,
                height: extent.height,
            },
        }
    }

    fn vk_to_image_format(format: vk::Format) -> ImageFormat {
        match format {
            vk::Format::R8G8B8A8_SRGB => ImageFormat::R8G8B8A8Srgb,
            vk::Format::B8G8R8A8_SRGB => ImageFormat::B8G8R8A8Srgb,
            vk::Format::D32_SFLOAT => ImageFormat::D32Sfloat,
            vk::Format::D32_SFLOAT_S8_UINT => ImageFormat::D32SfloatS8Uint,
            vk::Format::D24_UNORM_S8_UINT => ImageFormat::D24UnormS8Uint,
            vk::Format::D16_UNORM => ImageFormat::D16Unorm,
            vk::Format::R32_SFLOAT => ImageFormat::R32Sfloat,
            _ => ImageFormat::B8G8R8A8Srgb,
        }
    }

    pub fn build(self) -> ResourceKind {
        ResourceKind::ExternalImage {
            vk_image: self.image,
            image_view: self.image_view,
            format: self.format.into(),
            extent: self.extent.into(),
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
        let mut usage_flags = vk::ImageUsageFlags::empty();
        for usage in self.usages {
            usage_flags |= usage.to_vk_flags();
        }

        ResourceKind::Image {
            extent: self.extent.into(),
            format: self.format.into(),
            usage: usage_flags,
            samples: self.samples.into(),
            tiling: self.tiling.into(),
            initial_layout: self.initial_layout.into(),
            final_layout: self.final_layout.into(),
        }
    }
}

pub struct BufferResourceBuilder {
    size: u64,
    usages: Vec<vk::BufferUsageFlags>,
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
        self.usages.push(vk::BufferUsageFlags::VERTEX_BUFFER);
        self
    }

    pub fn index_buffer(mut self) -> Self {
        self.usages.push(vk::BufferUsageFlags::INDEX_BUFFER);
        self
    }

    pub fn uniform_buffer(mut self) -> Self {
        self.usages.push(vk::BufferUsageFlags::UNIFORM_BUFFER);
        self
    }

    pub fn storage_buffer(mut self) -> Self {
        self.usages.push(vk::BufferUsageFlags::STORAGE_BUFFER);
        self
    }

    pub fn transfer_src(mut self) -> Self {
        self.usages.push(vk::BufferUsageFlags::TRANSFER_SRC);
        self
    }

    pub fn transfer_dst(mut self) -> Self {
        self.usages.push(vk::BufferUsageFlags::TRANSFER_DST);
        self
    }

    pub fn device_local(mut self) -> Self {
        self.device_local = true;
        self
    }

    pub fn build(self) -> ResourceKind {
        let mut usage_flags = vk::BufferUsageFlags::empty();
        for usage in self.usages {
            usage_flags |= usage;
        }

        let memory_properties = if self.device_local {
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        } else {
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT
        };

        ResourceKind::Buffer {
            size: self.size,
            usage: usage_flags,
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
                vk_image: vk::Image::null(),
                image_view: vk::ImageView::null(),
                format: format.into(),
                extent: extent.into(),
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
            ResourceKind::Image {
                usage,
                samples,
                format,
                ..
            } => {
                assert!(usage.contains(vk::ImageUsageFlags::COLOR_ATTACHMENT));
                assert!(usage.contains(vk::ImageUsageFlags::SAMPLED));
                assert_eq!(samples, vk::SampleCountFlags::TYPE_1);
                assert_eq!(format, ImageFormat::B8G8R8A8Srgb.into());
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
                assert!(usage.contains(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT));
                assert_eq!(format, ImageFormat::D32Sfloat.into());
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
                assert!(usage.contains(vk::BufferUsageFlags::VERTEX_BUFFER));
                assert!(memory_properties.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL));
            }
            _ => panic!("Expected Buffer resource"),
        }
    }
}
