//! Editor play modes and restoration through the scene format.

use crate::scene::{Scene, SceneError, SceneManager};

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
    pub(crate) fn capture(app: &mut crate::application::Application) -> Result<Self, SceneError> {
        Ok(Self {
            scene: SceneManager::save_scene(app)?,
            document: app.scene_document.clone(),
        })
    }

    #[cfg(feature = "editor")]
    pub(crate) fn scene(&self) -> Scene {
        self.scene.clone()
    }

    pub(crate) fn restore(
        &self,
        app: &mut crate::application::Application,
    ) -> Result<(), SceneError> {
        if let Some(context) = &self.document.assets {
            SceneManager::load_with_context(
                app,
                self.scene.clone(),
                context.clone(),
                self.document.path.clone(),
            )?;
        } else {
            SceneManager::load_with_origin(app, self.scene.clone(), self.document.path.as_deref())?;
        }
        app.scene_document = self.document.clone();
        Ok(())
    }
}
