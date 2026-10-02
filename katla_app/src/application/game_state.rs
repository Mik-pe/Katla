//! Editor play modes and restoration through the scene format.

use crate::scene::{Scene, SceneManager};

/// Editor play mode state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    Editing,
    Playing,
    Paused,
}

/// Editor document captured before gameplay starts.
pub(crate) struct SceneSnapshot {
    scene: Scene,
    document: crate::scene::document::SceneDocument,
}

impl SceneSnapshot {
    pub(crate) fn capture(app: &crate::application::Application) -> Self {
        Self {
            scene: SceneManager::save_scene(app),
            document: crate::scene::document::SceneDocument {
                path: app.scene_document.path.clone(),
                saved: app.scene_document.saved.clone(),
            },
        }
    }

    #[cfg(feature = "editor")]
    pub(crate) fn scene(&self) -> Scene {
        self.scene.clone()
    }

    pub(crate) fn restore(&self, app: &mut crate::application::Application) -> Result<(), String> {
        SceneManager::load_scene(app, self.scene.clone())?;
        app.scene_document.path = self.document.path.clone();
        app.scene_document.saved = self.document.saved.clone();
        Ok(())
    }
}
