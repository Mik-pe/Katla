use katla_gfx::{MaterialHandle, MeshHandle, TextureHandle};
use std::collections::HashMap;

use crate::components::BillboardIcon;

/// GPU resources for billboard rendering.
pub struct BillboardResources {
    /// Unit quad mesh.
    pub mesh: MeshHandle,
    /// Alpha-blended billboard material.
    pub material: MaterialHandle,
    /// Icon textures indexed by BillboardIcon.
    pub icon_textures: HashMap<BillboardIcon, TextureHandle>,
    /// Whether resources have been initialized.
    pub initialized: bool,
}

impl Default for BillboardResources {
    fn default() -> Self {
        Self {
            mesh: MeshHandle::NONE,
            material: MaterialHandle::NONE,
            icon_textures: HashMap::new(),
            initialized: false,
        }
    }
}
