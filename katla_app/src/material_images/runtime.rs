//! Decode and weakly share immutable image generations, retiring their last owner.

use super::*;
use crate::scene::SceneAssetContext;
use katla_agent::material_sampling::TextureRole;
use katla_gfx::{GpuRenderer, ImageFormat};
use std::{collections::HashMap, sync::Weak};

#[derive(Hash, PartialEq, Eq)]
struct ImageKey {
    digest: [u8; 32],
    width: u32,
    height: u32,
    format: ImageFormat,
}

pub(crate) struct MaterialImages {
    images: HashMap<ImageKey, Weak<ImageLease>>,
    retire: mpsc::Sender<TextureHandle>,
    retired: mpsc::Receiver<TextureHandle>,
}
impl Default for MaterialImages {
    fn default() -> Self {
        let (retire, retired) = mpsc::channel();
        Self {
            images: HashMap::new(),
            retire,
            retired,
        }
    }
}
impl MaterialImages {
    pub(crate) fn drain(&mut self, renderer: &mut crate::Renderer) {
        for handle in self.retired.try_iter() {
            renderer.destroy_texture(handle);
        }
        self.images.retain(|_, image| image.strong_count() > 0);
    }
}

struct DecodedImage {
    source: TextureSource,
    descriptor: katla_gfx::TextureDescriptor,
    pixels: Vec<u8>,
    #[cfg(feature = "editor")]
    metadata: ImageMetadata,
}

impl crate::application::Application {
    pub(crate) fn prepare_material_image(
        &mut self,
        source: TextureSource,
        role: TextureRole,
        context: &SceneAssetContext,
    ) -> Result<Option<TextureBinding>, String> {
        use sha2::{Digest, Sha256};
        if source.asset().is_none() {
            return Ok(match source {
                TextureSource::Inherit => None,
                _ => Some(TextureBinding {
                    source,
                    image: None,
                    #[cfg(feature = "editor")]
                    metadata: None,
                }),
            });
        }
        let DecodedImage {
            source,
            descriptor,
            pixels,
            #[cfg(feature = "editor")]
            metadata,
        } = self.decode_material_image(source, role, context)?;
        let key = ImageKey {
            digest: Sha256::digest(&pixels).into(),
            width: descriptor.width,
            height: descriptor.height,
            format: descriptor.format,
        };
        let lease = if let Some(lease) = self
            .material_images
            .images
            .get(&key)
            .and_then(Weak::upgrade)
            .filter(|lease| self.renderer.get_bindless_slot(lease.handle).is_some())
        {
            lease
        } else {
            let handle = self
                .renderer
                .create_texture(&descriptor, &pixels)
                .map_err(|error| error.to_string())?;
            let lease = Arc::new(ImageLease {
                handle,
                retire: self.material_images.retire.clone(),
            });
            self.material_images
                .images
                .insert(key, Arc::downgrade(&lease));
            lease
        };
        Ok(Some(TextureBinding {
            source,
            image: Some(lease),
            #[cfg(feature = "editor")]
            metadata: Some(metadata),
        }))
    }
    #[cfg(feature = "editor")]
    pub(crate) fn validate_material_image(
        &mut self,
        source: &TextureSource,
        role: TextureRole,
        context: &SceneAssetContext,
    ) -> Result<(), String> {
        if source.asset().is_some() {
            self.decode_material_image(source.clone(), role, context)?;
        }
        Ok(())
    }

    fn decode_material_image(
        &mut self,
        source: TextureSource,
        role: TextureRole,
        context: &SceneAssetContext,
    ) -> Result<DecodedImage, String> {
        let path = std::fs::canonicalize(
            context.resolve(source.asset().ok_or("Image source requires an asset")?)?,
        )
        .map_err(|error| error.to_string())?;
        let model = if matches!(source, TextureSource::GltfImage { .. }) {
            Some(
                self.gltf_cache
                    .read(path.clone())
                    .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
        let image = match (&source, &model) {
            (TextureSource::File { .. }, _) => std::borrow::Cow::Owned(decode_file(&path)?),
            (TextureSource::GltfImage { image_index, .. }, Some(model)) => {
                std::borrow::Cow::Borrowed(
                    model
                        .images
                        .get(*image_index)
                        .ok_or_else(|| format!("glTF asset has no image {image_index}"))?,
                )
            }
            _ => return Err("Image source is unavailable".into()),
        };
        if image.width.max(image.height) > self.renderer.capabilities().max_texture_size {
            return Err("Image dimensions exceed this device's texture limit".into());
        }
        let srgb = matches!(role, TextureRole::Albedo | TextureRole::Emission);
        #[cfg(feature = "editor")]
        let floating = matches!(
            image.format,
            gltf::image::Format::R32G32B32FLOAT | gltf::image::Format::R32G32B32A32FLOAT
        );
        let (descriptor, pixels) = crate::util::gltf_image::texture_upload(&image, srgb)?;
        let mut source = source;
        if let Some(asset) = source.asset_mut() {
            *asset = AssetRef::File(path);
        }
        Ok(DecodedImage {
            source,
            #[cfg(feature = "editor")]
            metadata: ImageMetadata {
                width: image.width,
                height: image.height,
                mip_levels: descriptor.mip_levels,
                gpu_format: format!("{:?}", descriptor.format),
                decoded_format: format!("{:?}", image.format),
                source_color_space: if srgb && !floating { "srgb" } else { "linear" },
            },
            descriptor,
            pixels,
        })
    }

    pub(crate) fn prepare_material_assignments(
        &mut self,
        target: katla_ecs::EntityId,
        sources: &TextureAssignments,
        context: &SceneAssetContext,
        sampling: crate::rendering::MaterialSampling,
    ) -> Result<TextureBindings, String> {
        sources.validate()?;
        sampling.validate().map_err(str::to_owned)?;
        let drawable = self
            .world
            .get_component::<crate::components::DrawableComponent>(target)
            .ok_or("Material target has no drawable")?;
        for ((role, source), value) in TextureRole::ALL
            .into_iter()
            .zip(sources.roles())
            .zip(sampling.roles())
        {
            let needs_uv = match source {
                TextureSource::Inherit => drawable.texture_roles[role.index()],
                TextureSource::Neutral => false,
                _ => true,
            };
            if needs_uv && !drawable.uv_sets[value.uv.tex_coord as usize] {
                return Err(format!(
                    "{} image requires missing TEXCOORD_{}",
                    role.name(),
                    value.uv.tex_coord
                ));
            }
        }
        let mut bindings = TextureBindings::default();
        for (role, source) in TextureRole::ALL.into_iter().zip(sources.roles()) {
            bindings.0[role.index()] =
                self.prepare_material_image(source.clone(), role, context)?;
        }
        Ok(bindings)
    }
    pub(crate) fn drain_material_images(&mut self) {
        self.material_images.drain(&mut self.renderer);
    }
}

fn decode_file(path: &std::path::Path) -> Result<gltf::image::Data, String> {
    let size = std::fs::metadata(path)
        .map_err(|error| error.to_string())?
        .len();
    if size > 64 * 1024 * 1024 {
        return Err("Encoded material image exceeds 64 MiB".into());
    }
    let image = image::ImageReader::open(path)
        .map_err(|error| error.to_string())?
        .with_guessed_format()
        .map_err(|error| error.to_string())?
        .decode()
        .map_err(|error| error.to_string())?;
    let format = match image.color() {
        image::ColorType::L8 => gltf::image::Format::R8,
        image::ColorType::La8 => gltf::image::Format::R8G8,
        image::ColorType::Rgb8 => gltf::image::Format::R8G8B8,
        image::ColorType::Rgba8 => gltf::image::Format::R8G8B8A8,
        image::ColorType::L16 => gltf::image::Format::R16,
        image::ColorType::La16 => gltf::image::Format::R16G16,
        image::ColorType::Rgb16 => gltf::image::Format::R16G16B16,
        image::ColorType::Rgba16 => gltf::image::Format::R16G16B16A16,
        image::ColorType::Rgb32F => gltf::image::Format::R32G32B32FLOAT,
        image::ColorType::Rgba32F => gltf::image::Format::R32G32B32A32FLOAT,
        _ => return Err("Unsupported decoded material image format".into()),
    };
    Ok(gltf::image::Data {
        width: image.width(),
        height: image.height(),
        format,
        pixels: image.into_bytes(),
    })
}
