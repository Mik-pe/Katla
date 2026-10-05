//! Non-owning content cache. GPU reference counts own the actual resources.

use super::MeshAsset;
use crate::application::Application;
use katla_gfx::{GpuRenderer, MeshHandle, PrimitiveTopology};
use katla_math::AABB;
use std::{collections::HashMap, path::Path, sync::Arc};

#[derive(Default)]
pub(crate) struct MeshAssetCache {
    recipes: HashMap<String, (MeshHandle, AABB)>,
    keys: HashMap<MeshHandle, String>,
}

impl MeshAssetCache {
    pub(crate) fn contains(&self, key: &str) -> bool {
        self.recipes.contains_key(key)
    }

    pub(crate) fn remove(&mut self, handle: MeshHandle) {
        if let Some(key) = self.keys.remove(&handle) {
            self.recipes.remove(&key);
        }
    }
}

pub(crate) fn upload(app: &mut Application, path: &Path) -> Result<(MeshHandle, AABB), String> {
    let asset = MeshAsset::load(path)?;
    let key = asset.geometry_key()?;
    if let Some(&(handle, bounds)) = app.mesh_assets.recipes.get(&key) {
        return Ok((handle, bounds));
    }
    let compiled = asset.compile()?;
    let handle = app
        .renderer
        .create_mesh(
            &compiled.vertices,
            &compiled.indices,
            PrimitiveTopology::TriangleList,
        )
        .map_err(|error| error.to_string())?;
    let geometry = Arc::new(compiled.geometry);
    app.geometry_cache.insert_shared(handle, geometry.clone());
    if let Some(cache) = app
        .world
        .get_resource_mut::<crate::geometry_cache::GeometryCache>()
    {
        cache.insert_shared(handle, geometry);
    }
    app.mesh_assets
        .recipes
        .insert(key.clone(), (handle, compiled.bounds));
    app.mesh_assets.keys.insert(handle, key);
    Ok((handle, compiled.bounds))
}
