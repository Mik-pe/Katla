use super::VulkanCtx;
use erupt::{
    utils::allocator::{Allocation, MemoryTypeFinder},
    vk1_0::*,
};
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub channels: u32,
    memory: Allocation<Image>,
}

impl Texture {
    pub fn create_image(
        context: &mut VulkanCtx,
        width: u32,
        height: u32,
        format: Format,
        pixel_data: &[u8],
    ) -> Self {
        unsafe {
            let create_info = ImageCreateInfoBuilder::new()
                .extent(Extent3D {
                    width,
                    height,
                    depth: 1,
                })
                .image_type(ImageType::_2D)
                .mip_levels(1)
                .array_layers(1)
                .format(format)
                .usage(ImageUsageFlags::TRANSFER_DST | ImageUsageFlags::SAMPLED)
                .initial_layout(ImageLayout::UNDEFINED)
                .samples(SampleCountFlagBits::_1)
                .sharing_mode(SharingMode::EXCLUSIVE);

            let image = context
                .device
                .create_image(&create_info, None, None)
                .unwrap();
            let memory = context
                .allocator
                .allocate(&context.device, image, MemoryTypeFinder::dynamic())
                .unwrap();
            dbg!(&memory);

            let total_size = pixel_data.len() as u64;
            dbg!(total_size);
            let range = ..memory.region().start + total_size;
            dbg!(&range);

            let mut map = memory.map(&context.device, range).unwrap();
            map.import(pixel_data);
            map.unmap(&context.device).unwrap();
            Self {
                width,
                height,
                channels: 4,
                memory,
            }
        }
    }

    pub fn get_size(&self) -> u32 {
        self.width * self.height * self.channels
    }
}
