//! Portable material image choices and shared submission-safe runtime ownership.

use crate::scene::AssetRef;
use katla_gfx::{MaterialTextures, TextureHandle};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, mpsc};

/// Image selection independent of GPU handles and sampler settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextureSource {
    /// Use the mesh source's original role binding.
    #[default]
    Inherit,
    /// Use the scene's neutral texture for this role.
    Neutral,
    /// Decode a standalone image with the role's color-space interpretation.
    File { asset: AssetRef },
    /// Select a decoded image from a glTF asset, including embedded images.
    GltfImage { asset: AssetRef, image_index: usize },
}
impl TextureSource {
    pub(crate) fn asset(&self) -> Option<&AssetRef> {
        match self {
            Self::File { asset } | Self::GltfImage { asset, .. } => Some(asset),
            _ => None,
        }
    }
    pub(crate) fn asset_mut(&mut self) -> Option<&mut AssetRef> {
        match self {
            Self::File { asset } | Self::GltfImage { asset, .. } => Some(asset),
            _ => None,
        }
    }
}

/// Named role choices persisted with a drawable or reusable material.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextureAssignments {
    /// Base-color image; integer RGB decodes sRGB and alpha remains linear.
    pub albedo: TextureSource,
    /// Linear tangent-space normal image.
    pub normal: TextureSource,
    /// Linear data image using blue for metallicity and green for roughness.
    pub metallic_roughness: TextureSource,
    /// Linear red-channel occlusion image.
    pub occlusion: TextureSource,
    /// Emissive image; integer RGB decodes sRGB, decoded floats remain linear HDR.
    pub emission: TextureSource,
}
impl TextureAssignments {
    pub(crate) fn roles(&self) -> [&TextureSource; 5] {
        [
            &self.albedo,
            &self.normal,
            &self.metallic_roughness,
            &self.occlusion,
            &self.emission,
        ]
    }
    pub(crate) fn roles_mut(&mut self) -> [&mut TextureSource; 5] {
        [
            &mut self.albedo,
            &mut self.normal,
            &mut self.metallic_roughness,
            &mut self.occlusion,
            &mut self.emission,
        ]
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        for source in self.roles() {
            if let Some(asset) = source.asset() {
                asset.validate()?;
            }
        }
        Ok(())
    }
}

struct ImageLease {
    handle: TextureHandle,
    retire: mpsc::Sender<TextureHandle>,
}
impl Drop for ImageLease {
    fn drop(&mut self) {
        // A closed receiver means the application and its renderer have shut down.
        let _ = self.retire.send(self.handle);
    }
}

#[cfg(feature = "editor")]
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ImageMetadata {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) mip_levels: u32,
    pub(crate) gpu_format: String,
    pub(crate) decoded_format: String,
    pub(crate) source_color_space: &'static str,
}

#[derive(Clone)]
pub(crate) struct TextureBinding {
    pub(crate) source: TextureSource,
    image: Option<Arc<ImageLease>>,
    #[cfg(feature = "editor")]
    pub(crate) metadata: Option<ImageMetadata>,
}
impl TextureBinding {
    pub(crate) fn has_image(&self) -> bool {
        self.image.is_some()
    }
    pub(crate) fn handle(&self, neutral: TextureHandle) -> TextureHandle {
        self.image.as_ref().map_or(neutral, |image| image.handle)
    }
}

/// Runtime image references also held by undo history; the cache owns only weak links.
#[derive(Clone, Default)]
pub(crate) struct TextureBindings(pub(crate) [Option<TextureBinding>; 5]);
impl TextureBindings {
    pub(crate) fn assignments(&self) -> Option<TextureAssignments> {
        if self.0.iter().all(Option::is_none) {
            return None;
        }
        let mut sources = TextureAssignments::default();
        for (source, binding) in sources.roles_mut().into_iter().zip(&self.0) {
            if let Some(binding) = binding {
                *source = binding.source.clone();
            }
        }
        Some(sources)
    }
    pub(crate) fn textures(
        &self,
        original: MaterialTextures,
        neutral: MaterialTextures,
    ) -> Option<MaterialTextures> {
        if self.0[..4].iter().all(Option::is_none) {
            return None;
        }
        let old = [
            original.albedo,
            original.normal,
            original.metallic_roughness,
            original.occlusion,
        ];
        let fallback = [
            neutral.albedo,
            neutral.normal,
            neutral.metallic_roughness,
            neutral.occlusion,
        ];
        let handles: [TextureHandle; 4] = std::array::from_fn(|i| {
            self.0[i]
                .as_ref()
                .map_or(old[i], |binding| binding.handle(fallback[i]))
        });
        Some(MaterialTextures {
            albedo: handles[0],
            normal: handles[1],
            metallic_roughness: handles[2],
            occlusion: handles[3],
        })
    }
    pub(crate) fn emission(
        &self,
        original: TextureHandle,
        neutral: TextureHandle,
    ) -> TextureHandle {
        self.0[4]
            .as_ref()
            .map_or(original, |binding| binding.handle(neutral))
    }
}

mod runtime;
pub(crate) use runtime::MaterialImages;
