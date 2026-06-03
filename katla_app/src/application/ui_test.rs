use katla_ecs::EntityId;
use log::info;

#[cfg(feature = "editor")]
use crate::components::rendering::DrawableComponent;

pub struct UiTestRunner {
    output_dir: String,
    state: UiTestState,
    screenshots_taken: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiTestState {
    Default,
    EntitySelected,
    HierarchyExpanded,
    AssetBrowser,
    Preferences,
    Done,
}

impl UiTestState {
    fn screenshot_name(self) -> &'static str {
        match self {
            Self::Default => "01_default",
            Self::EntitySelected => "02_entity_selected",
            Self::HierarchyExpanded => "03_hierarchy_expanded",
            Self::AssetBrowser => "04_asset_browser",
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
        selected_entity: &mut Option<EntityId>,
        world: &katla_ecs::World,
    ) -> Option<String> {
        match self.state {
            UiTestState::Default if frame == 30 => {
                let path = self.screenshot_path(self.state.screenshot_name());
                info!(
                    "UI test [{}]: taking screenshot",
                    self.state.screenshot_name()
                );
                self.screenshots_taken += 1;
                self.state = UiTestState::EntitySelected;
                *selected_entity = world
                    .query_ref::<&DrawableComponent>()
                    .next()
                    .map(|(id, _)| id);
                if let Some(id) = selected_entity {
                    info!("UI test: selected entity {:?}", id);
                } else {
                    info!(
                        "UI test: no DrawableComponent entity found, proceeding without selection"
                    );
                }
                Some(path)
            }
            UiTestState::EntitySelected if frame == 50 => {
                let path = self.screenshot_path(self.state.screenshot_name());
                info!(
                    "UI test [{}]: taking screenshot",
                    self.state.screenshot_name()
                );
                self.screenshots_taken += 1;
                self.state = UiTestState::HierarchyExpanded;
                Some(path)
            }
            UiTestState::HierarchyExpanded if frame == 70 => {
                let path = self.screenshot_path(self.state.screenshot_name());
                info!(
                    "UI test [{}]: taking screenshot",
                    self.state.screenshot_name()
                );
                self.screenshots_taken += 1;
                self.state = UiTestState::AssetBrowser;
                Some(path)
            }
            UiTestState::AssetBrowser if frame == 90 => {
                let path = self.screenshot_path(self.state.screenshot_name());
                info!(
                    "UI test [{}]: taking screenshot",
                    self.state.screenshot_name()
                );
                self.screenshots_taken += 1;
                self.state = UiTestState::Preferences;
                Some(path)
            }
            UiTestState::Preferences if frame == 99 => {
                let path = self.screenshot_path(self.state.screenshot_name());
                info!(
                    "UI test [{}]: taking screenshot",
                    self.state.screenshot_name()
                );
                self.screenshots_taken += 1;
                self.state = UiTestState::Done;
                Some(path)
            }
            _ => None,
        }
    }
}
