//! Scene identity and the last successfully saved editor state.

use super::Scene;
use std::path::PathBuf;

pub(crate) struct SceneDocument {
    pub(crate) path: Option<PathBuf>,
    pub(crate) saved: Scene,
}

impl Default for SceneDocument {
    fn default() -> Self {
        Self {
            path: None,
            saved: Scene::new("Untitled"),
        }
    }
}

#[cfg(any(feature = "editor", test))]
impl SceneDocument {
    /// Playback progress changes continuously without editing the document.
    pub(crate) fn has_changes(&self, current: Scene) -> bool {
        fn authored_entities(mut scene: Scene) -> Vec<super::EntityDescriptor> {
            for entity in &mut scene.entities {
                if let Some(animation) = &mut entity.animation {
                    animation.duration = 0.0;
                    animation.target_duration = 0.0;
                    animation.time = 0.0;
                    animation.target_time = 0.0;
                    animation.loop_count = 0;
                    animation.target_loop_count = 0;
                    animation.completed = false;
                    animation.target_completed = false;
                    animation.blend_time = 0.0;
                    animation.blend_weight = 0.0;
                }
            }
            scene.entities
        }
        authored_entities(current) != authored_entities(self.saved.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_playback_progress_is_not_an_unsaved_edit_but_animation_speed_is() {
        let mut scene = crate::scene::build_default_scene();
        let animation_entity = scene
            .entities
            .iter()
            .position(|entity| entity.animation.is_some())
            .unwrap();
        let document = SceneDocument {
            path: None,
            saved: scene.clone(),
        };
        scene.entities[animation_entity]
            .animation
            .as_mut()
            .unwrap()
            .time += 1.0;
        scene.entities[animation_entity]
            .animation
            .as_mut()
            .unwrap()
            .duration = 3.5;
        assert!(!document.has_changes(scene.clone()));
        scene.entities[animation_entity]
            .animation
            .as_mut()
            .unwrap()
            .speed += 0.5;
        assert!(document.has_changes(scene));
    }
}
