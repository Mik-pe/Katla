//! Weak image uploads for the immutable decoded glTF asset cache.

use katla_gfx::{GpuRenderer, TextureHandle};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub(crate) struct GltfTextureCache {
    uploads: HashMap<PathBuf, HashMap<(usize, bool), TextureHandle>>,
}

impl GltfTextureCache {
    pub(crate) fn prune(&mut self, renderer: &impl GpuRenderer) {
        self.uploads.retain(|_, images| {
            images.retain(|_, handle| renderer.get_bindless_slot(*handle).is_some());
            !images.is_empty()
        });
    }

    pub(crate) fn get(&self, asset: &Path, image: usize, srgb: bool) -> Option<TextureHandle> {
        self.uploads.get(asset)?.get(&(image, srgb)).copied()
    }

    pub(crate) fn insert(&mut self, asset: &Path, image: usize, srgb: bool, handle: TextureHandle) {
        self.uploads
            .entry(asset.into())
            .or_default()
            .insert((image, srgb), handle);
    }
}
