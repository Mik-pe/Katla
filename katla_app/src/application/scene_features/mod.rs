//! Application-owned scene resources and built-in pass composition.

mod animation;
mod graphics;
mod lights;
pub(crate) mod material_pipelines;
mod particles;

pub(crate) use animation::AnimationFeatures;
pub(crate) use graphics::{GraphicsFrame, SceneGraphics};
use lights::LightFeatures;
use particles::ParticleFeatures;

use katla_gfx::{GpuRenderer, MaterialTextures, TextureDescriptor};

use crate::{AppResult, Renderer};

pub(crate) struct SceneFeatures {
    pub(crate) animation: AnimationFeatures,
    pub(crate) lights: LightFeatures,
    pub(crate) particles: ParticleFeatures,
    pub(crate) graphics: SceneGraphics,
    textures: MaterialTextures,
}

impl SceneFeatures {
    pub(crate) fn new(
        renderer: &mut Renderer,
        resources: &crate::resources::ResourceManager,
        bindings: &super::frame_graph_config::FrameGraphBindings,
    ) -> AppResult<Self> {
        let textures = create_material_textures(renderer)?;
        Ok(Self {
            animation: AnimationFeatures::new(renderer, resources)?,
            lights: LightFeatures::new(renderer, resources)?,
            particles: ParticleFeatures::new(renderer, resources)?,
            graphics: SceneGraphics::new(renderer, resources, bindings)?,
            textures,
        })
    }

    pub(crate) fn material_textures(&self) -> MaterialTextures {
        self.textures
    }
}

/// Neutral textures preserve all per-object material multipliers.
pub(crate) fn create_material_textures(renderer: &mut Renderer) -> AppResult<MaterialTextures> {
    let flat_normal = [0.5f32, 0.5, 1.0, 1.0].map(|value| half::f16::from_f32(value).to_bits());
    let normal_texture = renderer.create_texture(
        &TextureDescriptor::rgba16_float(1, 1).with_label("scene flat normal"),
        bytemuck::cast_slice(&flat_normal),
    )?;
    let metallic_roughness_texture = match renderer.create_texture(
        &TextureDescriptor::rgba8_unorm(1, 1).with_label("scene metallic roughness"),
        &[255, 255, 255, 255],
    ) {
        Ok(handle) => handle,
        Err(error) => {
            renderer.destroy_texture(normal_texture);
            return Err(error.into());
        }
    };
    Ok(MaterialTextures {
        albedo: renderer.default_texture(),
        normal: normal_texture,
        metallic_roughness: metallic_roughness_texture,
        occlusion: renderer.default_texture(),
    })
}
