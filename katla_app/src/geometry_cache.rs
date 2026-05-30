use std::collections::HashMap;
use std::sync::Arc;

use katla_gfx::MeshHandle;

/// CPU-side geometry data retained after mesh loading for collider generation etc.
#[derive(Debug, Clone)]
pub struct MeshGeometryData {
    pub positions: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

/// Cache mapping GPU mesh handles to their retained CPU geometry.
#[derive(Debug, Default)]
pub struct GeometryCache {
    entries: HashMap<MeshHandle, Arc<MeshGeometryData>>,
}

impl GeometryCache {
    pub fn insert(&mut self, handle: MeshHandle, data: MeshGeometryData) {
        self.entries.insert(handle, Arc::new(data));
    }

    pub fn get(&self, handle: MeshHandle) -> Option<&Arc<MeshGeometryData>> {
        self.entries.get(&handle)
    }
}
