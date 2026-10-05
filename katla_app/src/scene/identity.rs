//! Stable document-local identity, independent of ECS allocation and names.

use katla_ecs::Component;
use serde::{Deserialize, Serialize};

/// Persistent key within a scene document. Zero is never a valid entity key.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SceneEntityId(pub u64);

impl std::fmt::Display for SceneEntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Identity is retained through renaming, reload and play restoration.
#[derive(Component)]
pub(crate) struct SceneIdentity {
    #[inspect(skip)]
    pub(crate) id: SceneEntityId,
}
