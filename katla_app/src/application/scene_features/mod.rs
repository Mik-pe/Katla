//! Application-owned scene resources and built-in pass composition.

mod animation;
mod graphics;
mod lights;
mod particles;

use animation::AnimationFeatures;
pub(crate) use graphics::{GraphicsFrame, SceneGraphics};
use lights::LightFeatures;
use particles::ParticleFeatures;

use katla_gfx::{GpuRenderer, MaterialTextures, TextureDescriptor, TextureHandle};

use crate::{AppResult, Renderer};

pub(crate) struct SceneFeatures {
    pub(crate) animation: AnimationFeatures,
    pub(crate) lights: LightFeatures,
    pub(crate) particles: ParticleFeatures,
    pub(crate) graphics: SceneGraphics,
    white_texture: TextureHandle,
    normal_texture: TextureHandle,
    metallic_roughness_texture: TextureHandle,
}

impl SceneFeatures {
    pub(crate) fn new(
        renderer: &mut Renderer,
        resources: &crate::resources::ResourceManager,
        bindings: &super::frame_graph_config::FrameGraphBindings,
    ) -> AppResult<Self> {
        let normal_texture = renderer.create_texture(
            &TextureDescriptor::rgba8_unorm(1, 1).with_label("scene flat normal"),
            &[128, 128, 255, 255],
        )?;
        let metallic_roughness_texture = match renderer.create_texture(
            &TextureDescriptor::rgba8_unorm(1, 1).with_label("scene metallic roughness"),
            &[255, 128, 0, 255],
        ) {
            Ok(handle) => handle,
            Err(error) => {
                renderer.destroy_texture(normal_texture);
                return Err(error.into());
            }
        };
        Ok(Self {
            animation: AnimationFeatures::new(renderer, resources)?,
            lights: LightFeatures::new(renderer, resources)?,
            particles: ParticleFeatures::new(renderer, resources)?,
            graphics: SceneGraphics::new(renderer, resources, bindings)?,
            white_texture: renderer.default_texture(),
            normal_texture,
            metallic_roughness_texture,
        })
    }

    pub(crate) fn material_textures(&self) -> MaterialTextures {
        MaterialTextures {
            albedo: self.white_texture,
            normal: self.normal_texture,
            metallic_roughness: self.metallic_roughness_texture,
            occlusion: self.white_texture,
        }
    }
}
