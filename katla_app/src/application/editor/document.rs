//! File operations, unsaved changes and user-visible scene failures.

use super::Application;
use crate::scene::{Scene, SceneManager};
use crate::ui::editor_ui::declarative::scene_dialog::SceneDialog;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) enum DocumentAction {
    New,
    Open(PathBuf),
    Quit,
}

impl Application {
    pub(crate) fn has_unsaved_scene(&self) -> bool {
        let scene = if let Some(snapshot) = &self.scene_snapshot {
            snapshot.scene()
        } else {
            SceneManager::save_scene(self)
        };
        self.scene_document.has_changes(scene)
    }

    pub(crate) fn request_document_action(&mut self, action: DocumentAction) {
        if self.play_mode != super::super::game_state::PlayMode::Editing {
            self.show_scene_error("Stop play mode before opening, replacing or closing the scene.");
            return;
        }
        if self.has_unsaved_scene() {
            self.editor.pending_document_action = Some(action);
            self.editor.editor_ui.scene_dialog = Some(SceneDialog::Unsaved);
        } else {
            self.execute_document_action(action);
        }
    }

    pub(crate) fn execute_document_action(&mut self, action: DocumentAction) {
        let result = match action {
            DocumentAction::New => SceneManager::load_scene(self, Scene::new("Untitled")),
            DocumentAction::Open(path) => SceneManager::load_from_file(self, &path),
            DocumentAction::Quit => {
                self.quit_requested = true;
                Ok(())
            }
        };
        if let Err(error) = result {
            self.show_scene_error(error);
        }
    }

    pub(crate) fn show_scene_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        log::error!("{message}");
        self.editor.editor_ui.scene_dialog = Some(SceneDialog::Error(message));
    }

    pub(crate) fn choose_scene_file(&mut self, save: bool) {
        let path = self
            .scene_document
            .path
            .clone()
            .unwrap_or_else(crate::scene::default_scene_path);
        self.editor.editor_ui.scene_dialog = Some(if save {
            SceneDialog::SaveAs(path.display().to_string())
        } else {
            SceneDialog::Open(path.display().to_string())
        });
    }

    pub(crate) fn save_editor_scene(&mut self, path: Option<PathBuf>) {
        if self.play_mode != super::super::game_state::PlayMode::Editing {
            self.show_scene_error("Stop play mode before saving the editor scene.");
            return;
        }
        let Some(path) = path.or_else(|| self.scene_document.path.clone()) else {
            self.choose_scene_file(true);
            return;
        };
        match SceneManager::save_to_file(self, &path) {
            Ok(()) => {
                self.editor.editor_ui.scene_dialog = None;
                self.editor.editor_ui.show_save_confirmation();
                if let Some(action) = self.editor.pending_document_action.take() {
                    self.execute_document_action(action);
                }
            }
            Err(error) => self.show_scene_error(error),
        }
    }

    pub(crate) fn submit_scene_path(&mut self, value: String) {
        let value = value.trim();
        if value.is_empty() {
            return;
        }
        let path = PathBuf::from(value);
        match self.editor.editor_ui.scene_dialog.take() {
            Some(SceneDialog::Open(_)) => self.request_document_action(DocumentAction::Open(path)),
            Some(SceneDialog::SaveAs(_)) if path.exists() => {
                self.editor.editor_ui.scene_dialog = Some(SceneDialog::Overwrite(path));
            }
            Some(SceneDialog::SaveAs(_)) => self.save_editor_scene(Some(path)),
            other => self.editor.editor_ui.scene_dialog = other,
        }
    }
}
