use log::info;

#[cfg(feature = "editor")]
use crate::components::rendering::DrawableComponent;
#[cfg(feature = "editor")]
use crate::ui::{EditorUI, Panel};

pub struct UiTestRunner {
    output_dir: String,
    state: UiTestState,
    screenshots_taken: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiTestState {
    Default,
    DefaultShot,
    EntitySelected,
    EntitySelectedShot,
    HierarchyExpanded,
    HierarchyExpandedShot,
    AssetBrowserShot,
    PreferencesOpening,
    Preferences,
    Done,
}

impl UiTestState {
    fn screenshot_name(self) -> &'static str {
        match self {
            Self::Default => "",
            Self::DefaultShot => "01_default",
            Self::EntitySelected => "",
            Self::EntitySelectedShot => "02_entity_selected",
            Self::HierarchyExpanded => "",
            Self::HierarchyExpandedShot => "03_hierarchy_expanded",
            Self::AssetBrowserShot => "04_asset_browser",
            Self::PreferencesOpening => "",
            Self::Preferences => "05_preferences",
            Self::Done => "",
        }
    }
}

impl UiTestRunner {
    pub fn new(output_dir: String) -> Self {
        std::fs::create_dir_all(&output_dir).unwrap_or_else(|e| {
            log::error!(
                "Failed to create UI test output dir '{}': {}",
                output_dir,
                e
            );
        });
        info!("UI test mode: screenshots will be saved to {}", output_dir);
        Self {
            output_dir,
            state: UiTestState::Default,
            screenshots_taken: 0,
        }
    }

    pub fn screenshot_path(&self, name: &str) -> String {
        format!("{}/{}.png", self.output_dir, name)
    }

    pub fn screenshots_taken(&self) -> usize {
        self.screenshots_taken
    }

    #[cfg(feature = "editor")]
    pub fn on_frame(
        &mut self,
        frame: usize,
        editor_ui: &mut EditorUI,
        world: &katla_ecs::World,
    ) -> Option<String> {
        match self.state {
            // Frame 10: take default screenshot (no state changes needed)
            UiTestState::Default if frame == 10 => {
                let name = UiTestState::DefaultShot.screenshot_name();
                let path = self.screenshot_path(name);
                info!("UI test [{}]: taking screenshot", name);
                self.screenshots_taken += 1;
                self.state = UiTestState::DefaultShot;
                Some(path)
            }
            // Frame 15: select entity (state change, no screenshot)
            UiTestState::DefaultShot if frame == 15 => {
                editor_ui.selected_entity = world
                    .query_ref::<&DrawableComponent>()
                    .next()
                    .map(|(id, _)| id);
                if let Some(id) = editor_ui.selected_entity {
                    info!("UI test: selected entity {:?}", id);
                } else {
                    info!(
                        "UI test: no DrawableComponent entity found, proceeding without selection"
                    );
                }
                self.state = UiTestState::EntitySelected;
                None
            }
            // Frame 30: take entity_selected screenshot (entity selected for 15 frames)
            UiTestState::EntitySelected if frame == 30 => {
                let name = UiTestState::EntitySelectedShot.screenshot_name();
                let path = self.screenshot_path(name);
                info!("UI test [{}]: taking screenshot", name);
                self.screenshots_taken += 1;
                self.state = UiTestState::EntitySelectedShot;
                Some(path)
            }
            // Frame 35: expand hierarchy (state change, no screenshot)
            UiTestState::EntitySelectedShot if frame == 35 => {
                if let Some(id) = editor_ui.selected_entity {
                    editor_ui.expand_entity(id);
                    info!("UI test: expanded entity {:?} in hierarchy", id);
                }
                self.state = UiTestState::HierarchyExpanded;
                None
            }
            // Frame 50: take hierarchy_expanded screenshot (expanded for 15 frames)
            UiTestState::HierarchyExpanded if frame == 50 => {
                let name = UiTestState::HierarchyExpandedShot.screenshot_name();
                let path = self.screenshot_path(name);
                info!("UI test [{}]: taking screenshot", name);
                self.screenshots_taken += 1;
                self.state = UiTestState::HierarchyExpandedShot;
                Some(path)
            }
            // Frame 70: take asset_browser screenshot (no state change needed)
            UiTestState::HierarchyExpandedShot if frame == 70 => {
                let name = UiTestState::AssetBrowserShot.screenshot_name();
                let path = self.screenshot_path(name);
                info!("UI test [{}]: taking screenshot", name);
                self.screenshots_taken += 1;
                self.state = UiTestState::AssetBrowserShot;
                Some(path)
            }
            // Frame 75: open preferences panel (state change, no screenshot)
            UiTestState::AssetBrowserShot if frame == 75 => {
                editor_ui.open_panel(Panel::Preferences);
                info!("UI test: opened preferences panel");
                self.state = UiTestState::PreferencesOpening;
                None
            }
            // Frame 99: take preferences screenshot (panel open for 24 frames)
            UiTestState::PreferencesOpening if frame == 99 => {
                let path = self.screenshot_path(UiTestState::Preferences.screenshot_name());
                info!(
                    "UI test [{}]: taking screenshot",
                    UiTestState::Preferences.screenshot_name()
                );
                self.screenshots_taken += 1;
                self.state = UiTestState::Done;
                Some(path)
            }
            _ => None,
        }
    }
}
