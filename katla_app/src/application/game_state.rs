//! Editor play mode state machine and scene snapshot for play/stop cycle.

use log::info;

use crate::scene::SceneManager;

/// Editor play mode state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    Editing,
    Playing,
    Paused,
}

/// Serialized snapshot of all non-hidden entities in the world.
///
/// Captured before entering play mode and restored when play mode stops,
/// so any runtime mutations during gameplay are discarded.
pub(crate) struct SceneSnapshot {
    ron_data: String,
}

impl SceneSnapshot {
    /// Serialize all non-hidden entities into an in-memory RON snapshot.
    pub fn capture(app: &crate::application::Application) -> Self {
        let scene = SceneManager::save_scene(app);
        let ron_data = ron::ser::to_string_pretty(&scene, crate::scene::ron_pretty_config())
            .expect("Scene serialization should not fail for in-memory snapshot");
        info!(
            "Captured scene snapshot ({} bytes, {} entities)",
            ron_data.len(),
            scene.entities.len()
        );
        Self { ron_data }
    }

    /// Restore the world to the snapshotted state.
    ///
    /// Destroys all non-hidden entities, then re-spawns from the snapshot.
    pub fn restore(self, app: &mut crate::application::Application) {
        let scene: crate::scene::Scene = ron::from_str(&self.ron_data)
            .expect("Scene deserialization should not fail for in-memory snapshot");
        info!(
            "Restoring scene snapshot ({} entities)",
            scene.entities.len()
        );
        SceneManager::load_scene(app, scene).expect("Scene load from snapshot should not fail");
    }
}
