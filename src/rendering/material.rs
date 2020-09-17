use crate::rendering::vertextypes::*;
use crate::util::CachedGLTFModel;
use crate::vulkanstuff::Texture;
use crate::vulkanstuff::VulkanRenderer;
use crate::vulkanstuff::{ImageInfo, RenderPipeline};

use mikpe_math::Mat4;

use erupt::{utils::allocator::Allocator, vk1_0::*, DeviceLoader};

use std::rc::Rc;

pub struct Material {
    pub renderpipeline: RenderPipeline,
    pub texture: Option<Texture>,
}

impl Material {
    pub fn new(model: Rc<CachedGLTFModel>, renderer: &mut VulkanRenderer) -> Self {
        let render_pass = renderer.render_pass;
        let surface_caps = renderer.surface_caps();
        let num_images = renderer.num_images();
        let (device, mut allocator) = renderer.device_and_allocator();
        let mut renderpipeline = RenderPipeline::new::<VertexPBR>(
            &device,
            &mut allocator,
            render_pass,
            surface_caps,
            num_images,
        );
        let mut texture = None;
        if !model.images.is_empty() {
            let image = &model.images[0];
            println!("Image format: {:?}", image.format);
            //TODO: Support more image formats:
            match image.format {
                gltf::image::Format::R8G8B8 => {
                    let mut pad_vec = Vec::new();
                    pad_vec.resize((image.width * image.height) as usize, 0u8);
                    let pixels = &image.pixels;

                    let pixel_chunks = pixels.chunks(3);

                    let mut new_pixels = Vec::with_capacity(pixels.len() + pad_vec.len());
                    for (pixel, pad) in pixel_chunks.zip(pad_vec) {
                        new_pixels.push(pixel[0]);
                        new_pixels.push(pixel[1]);
                        new_pixels.push(pixel[2]);
                        new_pixels.push(pad);
                    }
                    let tex = Texture::create_image(
                        &mut renderer.context,
                        image.width,
                        image.height,
                        Format::R8G8B8A8_SRGB,
                        new_pixels.as_slice(),
                    );
                    renderpipeline.uniform.add_image_info(ImageInfo {
                        image_view: tex.image_view,
                        sampler: tex.image_sampler,
                    });
                    texture = Some(tex);
                }
                gltf::image::Format::R8G8B8A8 => {
                    let pixels = &image.pixels;

                    let tex = Texture::create_image(
                        &mut renderer.context,
                        image.width,
                        image.height,
                        Format::R8G8B8A8_SRGB,
                        pixels.as_slice(),
                    );
                    renderpipeline.uniform.add_image_info(ImageInfo {
                        image_view: tex.image_view,
                        sampler: tex.image_sampler,
                    });

                    texture = Some(tex);
                }
                _ => {}
            }
        }
        Self {
            renderpipeline,
            texture,
        }
    }

    pub fn destroy(&mut self, device: &DeviceLoader, allocator: &mut Allocator) {
        self.renderpipeline.destroy(device, allocator);
    }

    //TODO: Can we in any way fix so that these bindings happen in a better way?
    //Maybe decouple the actual data of the uniform to the drawcall-creation and
    //let the material stop caring about the image_index
    pub fn bind(&self, device: &DeviceLoader, command_buffer: CommandBuffer) {
        unsafe {
            device.cmd_bind_pipeline(
                command_buffer,
                PipelineBindPoint::GRAPHICS,
                self.renderpipeline.pipeline,
            );
            device.cmd_bind_descriptor_sets(
                command_buffer,
                PipelineBindPoint::GRAPHICS,
                self.renderpipeline.pipeline_layout,
                0,
                &[self.renderpipeline.uniform.next_descriptor().desc_set],
                &[],
            );
        }
    }

    pub fn upload_pipeline_data(
        &mut self,
        device: &DeviceLoader,
        view: Mat4,
        proj: Mat4,
        model: Mat4,
    ) {
        let mat = [model, view, proj];
        let data_slice = unsafe {
            std::slice::from_raw_parts(mat.as_ptr() as *const u8, std::mem::size_of_val(&mat))
        };
        self.renderpipeline
            .uniform
            .update_buffer(device, data_slice);
    }
}
