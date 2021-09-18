use crate::{
    renderer::{
        vulkan::{CommandBuffer, VulkanContext},
        ImageInfo, RenderPipeline, Texture,
    },
    rendering::vertextypes::*,
    util::GLTFModel,
};

use katla_math::Mat4;

use ash::{vk, Device};

use std::{rc::Rc, sync::Arc};

pub struct Material {
    pub renderpipeline: RenderPipeline,
    pub texture: Option<Texture>,
    context: Arc<VulkanContext>,
}

impl Material {
    pub fn new(
        model: Rc<GLTFModel>,
        context: Arc<VulkanContext>,
        render_pass: vk::RenderPass,
        num_images: usize,
    ) -> Self {
        let mut renderpipeline =
            RenderPipeline::new::<VertexPBR>(context.clone(), render_pass, num_images);
        let mut texture = None;
        if !model.images.is_empty() {
            let image = &model.images[0];
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
                        &context,
                        image.width,
                        image.height,
                        vk::Format::R8G8B8A8_SRGB,
                        new_pixels.as_slice(),
                    );
                    renderpipeline
                        .uniform
                        .add_image_info(ImageInfo::new(tex.image_view, tex.image_sampler));
                    texture = Some(tex);
                }
                gltf::image::Format::R8G8B8A8 => {
                    let pixels = &image.pixels;

                    let tex = Texture::create_image(
                        &context,
                        image.width,
                        image.height,
                        vk::Format::R8G8B8A8_SRGB,
                        pixels.as_slice(),
                    );
                    renderpipeline
                        .uniform
                        .add_image_info(ImageInfo::new(tex.image_view, tex.image_sampler));

                    texture = Some(tex);
                }
                _ => {
                    println!("Unsupported texture format: {:?}", image.format);
                }
            }
        }
        Self {
            renderpipeline,
            context,
            texture,
        }
    }

    //TODO: Can we in any way fix so that these bindings happen in a better way?
    //Maybe decouple the actual data of the uniform to the drawcall-creation and
    //let the material stop caring about the image_index
    pub fn bind(&self, command_buffer: &CommandBuffer) {
        command_buffer.bind_pipeline(
            self.renderpipeline.pipeline,
            vk::PipelineBindPoint::GRAPHICS,
        );

        command_buffer.bind_descriptor_sets(
            vk::PipelineBindPoint::GRAPHICS,
            self.renderpipeline.pipeline_layout,
            &[self.renderpipeline.uniform.next_descriptor().desc_set],
        );
    }

    pub fn upload_pipeline_data(&mut self, device: &Device, view: Mat4, proj: Mat4, model: Mat4) {
        let mat = [model, view, proj];
        let data_slice = unsafe {
            std::slice::from_raw_parts(mat.as_ptr() as *const u8, std::mem::size_of_val(&mat))
        };
        self.renderpipeline.update_buffer(data_slice);
    }
}

impl Drop for Material {
    fn drop(&mut self) {
        if let Some(texture) = self.texture.take() {
            texture.destroy(&self.context);
        }
        self.renderpipeline.destroy();
    }
}
