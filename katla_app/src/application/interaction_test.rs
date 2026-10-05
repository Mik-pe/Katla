//! Headless interaction test: drives synthetic mouse input through the real
//! UI hit-testing pipeline and the viewport GPU-picking path, capturing a
//! screenshot at each state plus programmatic checks.
//!
//! Scene and dock click coordinates use the 1280x720 default layout. Inspector and menu
//! targets are resolved from the live widget tree.

use log::info;

#[cfg(feature = "editor")]
use crate::application::Application;
#[cfg(feature = "editor")]
use crate::components::scene::NameComponent;
#[cfg(feature = "editor")]
use crate::ui::Panel;
#[cfg(feature = "editor")]
use katla_math::Vec2;
#[cfg(feature = "editor")]
use winit::event::{ElementState, MouseButton};

/// Logical-pixel click targets for the default scene layout.
#[cfg(feature = "editor")]
mod target {
    /// Hierarchy row for entity "Sphere_1_0" (rows start at y=112, 28px pitch).
    pub const HIERARCHY_SPHERE_1_0: (f32, f32) = (117.0, 181.0);
    /// Hierarchy list body, used as the wheel-scroll position.
    pub const HIERARCHY_BODY: (f32, f32) = (117.0, 300.0);
    /// Solid front face of CenterCube, below the light icon and away from the selected gizmo.
    pub const VIEWPORT_OBJECT: (f32, f32) = (412.0, 261.0);
    /// Empty sky above the torus, away from all geometry.
    pub const VIEWPORT_EMPTY_SKY: (f32, f32) = (940.0, 110.0);
    /// "Console" tab in the central bottom dock strip.
    pub const CONSOLE_TAB: (f32, f32) = (440.0, 526.0);
    /// "Light" theme swatch row inside the centered Preferences modal.
    pub const PREFERENCES_LIGHT_SWATCH: (f32, f32) = (550.0, 227.0);
    /// "Dark" theme swatch row inside the centered Preferences modal.
    pub const PREFERENCES_DARK_SWATCH: (f32, f32) = (800.0, 192.0);
    /// Close button of the Preferences modal (top-right).
    pub const PREFERENCES_CLOSE: (f32, f32) = (900.0, 120.0);
}

/// What the runner should do next. `begin_frame` performs press/release/scroll
/// actions; `end_frame` performs checks and screenshots, then advances to the
/// next action state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    PressHierarchy,
    ReleaseHierarchy,
    CheckHierarchy,
    ShotHierarchy,
    ScrollDown,
    ShotScrolledDown,
    ScrollUp,
    ShotScrolledUp,
    HoverViewport,
    PressViewport,
    ReleaseViewport,
    ShotViewport,
    PressEmpty,
    ReleaseEmpty,
    ShotEmpty,
    PressConsoleTab,
    ReleaseConsoleTab,
    ShotConsoleTab,
    OpenPreferences,
    PressLightSwatch,
    ReleaseLightSwatch,
    ShotLightSwatch,
    PressDarkSwatch,
    ReleaseDarkSwatch,
    ShotDarkSwatch,
    PressClose,
    ReleaseClose,
    ShotClose,
    PressHierarchyAgain,
    ReleaseHierarchyAgain,
    PressPreset,
    ReleasePreset,
    CheckPreset,
    DragMaterial,
    CheckMaterialDrag,
    UndoMaterial,
    CheckMaterialUndo,
    RedoMaterial,
    CheckMaterialRedo,
    CollapseMaterial,
    PressAddComponent,
    ReleaseAddComponent,
    ShotAddOpen,
    PressAddRow,
    ReleaseAddRow,
    CheckAddComponent,
    PressRemoveComponent,
    ReleaseRemoveComponent,
    CheckRemoveComponent,
    PrefabWalkthrough,
    Done,
}

/// One behavioral check with its outcome, reported in the summary.
struct Check {
    name: &'static str,
    passed: bool,
    detail: String,
}

pub struct InteractionTestRunner {
    output_dir: String,
    state: State,
    screenshots_taken: usize,
    checks: Vec<Check>,
    #[cfg(feature = "editor")]
    material_history_before: usize,
}

impl InteractionTestRunner {
    pub fn new(output_dir: String) -> Self {
        std::fs::create_dir_all(&output_dir).unwrap_or_else(|e| {
            log::error!(
                "Failed to create interaction test output dir '{}': {}",
                output_dir,
                e
            );
        });
        info!(
            "Interaction test mode: screenshots will be saved to {}",
            output_dir
        );
        Self {
            output_dir,
            state: State::Idle,
            screenshots_taken: 0,
            checks: Vec::new(),
            #[cfg(feature = "editor")]
            material_history_before: 0,
        }
    }

    #[cfg(feature = "editor")]
    fn screenshot_path(&self, name: &str) -> String {
        format!("{}/{}.png", self.output_dir, name)
    }

    #[cfg(feature = "editor")]
    fn record(&mut self, name: &'static str, passed: bool, detail: String) {
        info!(
            "Interaction check [{}]: {} ({})",
            name,
            if passed { "PASS" } else { "FAIL" },
            detail
        );
        self.checks.push(Check {
            name,
            passed,
            detail,
        });
    }

    /// Name of the currently selected entity, if any and named.
    #[cfg(feature = "editor")]
    fn selected_name(app: &Application) -> Option<String> {
        let id = app.editor.editor_ui.selected_entity?;
        app.world
            .get_component::<NameComponent>(id)
            .map(|n| n.name.clone())
    }

    /// Whether the selected entity carries a component of type `T`.
    #[cfg(feature = "editor")]
    fn selected_has_component<T: katla_ecs::Component>(app: &Application) -> bool {
        app.editor
            .editor_ui
            .selected_entity
            .is_some_and(|id| app.world.get_component::<T>(id).is_some())
    }

    /// Synthetic UI press: position the mouse and press the left button.
    /// Widgets see `mouse_clicked` during this frame's `process_input`.
    #[cfg(feature = "editor")]
    fn ui_press(app: &mut Application, pos: (f32, f32)) {
        let input = app.ui_context.input_mut();
        input.set_mouse_pos(Vec2::new(pos.0, pos.1));
        let time = input.last_click_time[katla_ui::mouse_button::LEFT] + 0.1;
        input.set_mouse_button_with_time(katla_ui::mouse_button::LEFT, true, time);
    }

    /// Synthetic UI release on the following frame.
    #[cfg(feature = "editor")]
    fn ui_release(app: &mut Application) {
        app.ui_context
            .input_mut()
            .set_mouse_button(katla_ui::mouse_button::LEFT, false);
    }

    /// Full press: UI input plus the editor mouse path (focused panel, gizmo
    /// hit test, viewport pick request) — mirrors the winit event routing.
    #[cfg(feature = "editor")]
    fn full_press(app: &mut Application, pos: (f32, f32)) {
        Self::ui_press(app, pos);
        app.on_mouse_input(&ElementState::Pressed, &MouseButton::Left);
    }

    #[cfg(feature = "editor")]
    fn full_release(app: &mut Application) {
        Self::ui_release(app);
        app.on_mouse_input(&ElementState::Released, &MouseButton::Left);
    }

    /// Wheel tick over the hierarchy list body.
    #[cfg(feature = "editor")]
    fn scroll_hierarchy(app: &mut Application, delta_y: f32) {
        let input = app.ui_context.input_mut();
        input.set_mouse_pos(Vec2::new(
            target::HIERARCHY_BODY.0,
            target::HIERARCHY_BODY.1,
        ));
        input.scroll_delta = Vec2::new(0.0, delta_y);
    }

    /// Move the mouse into the viewport so the pick gate
    /// (`prev_hover_z_index == DEFAULT`) is satisfied at press time.
    #[cfg(feature = "editor")]
    fn hover_viewport(app: &mut Application) {
        app.ui_context.input_mut().set_mouse_pos(Vec2::new(
            target::VIEWPORT_OBJECT.0,
            target::VIEWPORT_OBJECT.1,
        ));
    }

    #[cfg(feature = "editor")]
    fn click_widget(app: &mut Application, kind: &str, label: &str, remove: bool) {
        use katla_ui::declarative::widgets::{
            button::Button, image_button::ImageButton, section::Section, text::Text,
        };
        let tree = app.editor.editor_ui.view_tree();
        let position = tree.iter_nodes().find_map(|(id, node)| {
            let any = node.widget.as_any();
            let matches = match kind {
                "button" => any
                    .downcast_ref::<Button>()
                    .is_some_and(|w| w.label == label),
                "section" => any
                    .downcast_ref::<Section>()
                    .is_some_and(|w| w.title == label),
                "icon" => any
                    .downcast_ref::<ImageButton>()
                    .is_some_and(|w| w.tooltip.as_deref() == Some(label)),
                "prefix" => any
                    .downcast_ref::<Text>()
                    .is_some_and(|w| w.content.starts_with(label)),
                "text" => any
                    .downcast_ref::<Text>()
                    .is_some_and(|w| w.content == label),
                _ => false,
            };
            if !matches {
                return None;
            }
            let bounds = tree.resolved_bounds().get(&id)?;
            let y = if kind == "section" {
                bounds.min.y() + 10.0
            } else {
                bounds.center().y()
            };
            Some((
                if remove {
                    bounds.max.x() - 8.0
                } else {
                    bounds.center().x()
                },
                y,
            ))
        });
        if let Some(position) = position {
            log::info!("Interaction click {kind} {label} at {position:?}");
            Self::ui_press(app, position);
        } else {
            log::error!("Interaction target missing: {kind} {label}");
        }
    }

    #[cfg(feature = "editor")]
    fn click_menu(app: &mut Application, label: &str, entry: Option<&str>) {
        use katla_ui::declarative::widgets::menubar::MenuBar;
        let tree = app.editor.editor_ui.view_tree();
        let position = tree.iter_nodes().find_map(|(id, node)| {
            let bar = node.widget.as_any().downcast_ref::<MenuBar>()?;
            let index = bar.groups.iter().position(|group| group.label == label)?;
            let bounds = *tree.resolved_bounds().get(&id)?;
            let group = bar.group_bounds(bounds)[index];
            let region = if let Some(entry) = entry {
                let row = bar.groups[index]
                    .items
                    .iter()
                    .position(|item| item.label == entry)?;
                bar.entry_bounds(index, group)[row]
            } else {
                group
            };
            Some((region.center().x(), region.center().y()))
        });
        if let Some(position) = position {
            Self::ui_press(app, position);
        } else {
            log::error!("Interaction menu target missing: {label} {entry:?}");
        }
    }

    #[cfg(feature = "editor")]
    fn drag_material(app: &mut Application, value: f32, outside_row: bool) {
        use katla_ui::declarative::widgets::labeled_slider::LabeledSlider;
        let tree = app.editor.editor_ui.view_tree();
        let position = tree.iter_nodes().find_map(|(id, node)| {
            let slider = node.widget.as_any().downcast_ref::<LabeledSlider>()?;
            if slider.label != "Roughness" {
                return None;
            }
            let track = slider.track_bounds(*tree.resolved_bounds().get(&id)?);
            Some((
                track.min.x() + track.width() * value,
                track.center().y() + if outside_row { 32.0 } else { 0.0 },
            ))
        });
        if let Some(position) = position {
            Self::ui_press(app, position);
        }
    }

    #[cfg(feature = "editor")]
    fn selected_material(app: &Application) -> Option<katla_agent::material::MaterialValues> {
        let entity = app.editor.editor_ui.selected_entity?;
        app.world
            .get_component::<crate::components::DrawableComponent>(entity)
            .map(crate::application::editor::material::values)
    }

    /// Fail a walkthrough with missing steps or failed behavioral checks.
    #[cfg(feature = "editor")]
    pub fn validate(&self) -> crate::AppResult<()> {
        let receipt = serde_json::json!({
            "complete": self.state == State::Done,
            "screenshots": self.screenshots_taken,
            "checks": self.checks.iter().map(|check| serde_json::json!({
                "name": check.name, "passed": check.passed, "detail": check.detail,
            })).collect::<Vec<_>>(),
        });
        std::fs::write(
            format!("{}/receipt.json", self.output_dir),
            receipt.to_string(),
        )
        .map_err(|error| crate::AppError::Other {
            message: format!("Writing walkthrough receipt: {error}"),
        })?;
        if self.state != State::Done || self.checks.iter().any(|check| !check.passed) {
            return Err(crate::AppError::Other {
                message: "Interaction walkthrough incomplete or failed; see receipt.json".into(),
            });
        }
        Ok(())
    }

    /// Called before each headless frame renders. `frame` is the index of the
    /// frame about to render (equals `Application::frame_count`).
    #[cfg(feature = "editor")]
    pub fn begin_frame(&mut self, app: &mut Application, frame: usize) {
        match self.state {
            State::PressHierarchy if frame == 14 => {
                Self::ui_press(app, target::HIERARCHY_SPHERE_1_0);
                self.state = State::ReleaseHierarchy;
            }
            State::ReleaseHierarchy if frame == 15 => {
                Self::ui_release(app);
                self.state = State::CheckHierarchy;
            }
            State::ScrollDown if (19..=24).contains(&frame) => {
                Self::scroll_hierarchy(app, -5.0);
                if frame == 24 {
                    self.state = State::ShotScrolledDown;
                }
            }
            State::ScrollUp if (27..=32).contains(&frame) => {
                Self::scroll_hierarchy(app, 5.0);
                if frame == 32 {
                    self.state = State::ShotScrolledUp;
                }
            }
            State::HoverViewport if (35..=37).contains(&frame) => {
                Self::hover_viewport(app);
                if frame == 37 {
                    self.state = State::PressViewport;
                }
            }
            State::PressViewport if frame == 38 => {
                Self::full_press(app, target::VIEWPORT_OBJECT);
                self.state = State::ReleaseViewport;
            }
            State::ReleaseViewport if frame == 39 => {
                Self::full_release(app);
                self.state = State::ShotViewport;
            }
            State::PressEmpty if frame == 44 => {
                Self::full_press(app, target::VIEWPORT_EMPTY_SKY);
                self.state = State::ReleaseEmpty;
            }
            State::ReleaseEmpty if frame == 45 => {
                Self::full_release(app);
                self.state = State::ShotEmpty;
            }
            State::PressConsoleTab if frame == 52 => {
                Self::ui_press(app, target::CONSOLE_TAB);
                self.state = State::ReleaseConsoleTab;
            }
            State::ReleaseConsoleTab if frame == 53 => {
                Self::ui_release(app);
                self.state = State::ShotConsoleTab;
            }
            State::OpenPreferences if frame == 58 => {
                app.editor.editor_ui.open_panel(Panel::Preferences);
                self.state = State::PressLightSwatch;
            }
            State::PressLightSwatch if frame == 62 => {
                Self::ui_press(app, target::PREFERENCES_LIGHT_SWATCH);
                self.state = State::ReleaseLightSwatch;
            }
            State::ReleaseLightSwatch if frame == 63 => {
                Self::ui_release(app);
                self.state = State::ShotLightSwatch;
            }
            State::PressDarkSwatch if frame == 68 => {
                Self::ui_press(app, target::PREFERENCES_DARK_SWATCH);
                self.state = State::ReleaseDarkSwatch;
            }
            State::ReleaseDarkSwatch if frame == 69 => {
                Self::ui_release(app);
                self.state = State::ShotDarkSwatch;
            }
            State::PressClose if frame == 74 => {
                Self::ui_press(app, target::PREFERENCES_CLOSE);
                self.state = State::ReleaseClose;
            }
            State::ReleaseClose if frame == 75 => {
                Self::ui_release(app);
                self.state = State::ShotClose;
            }
            State::PressHierarchyAgain if frame == 84 => {
                Self::ui_press(app, target::HIERARCHY_SPHERE_1_0);
                self.state = State::ReleaseHierarchyAgain;
            }
            State::ReleaseHierarchyAgain if frame == 85 => {
                Self::ui_release(app);
                self.state = State::PressPreset;
            }
            State::PressPreset if frame == 88 => {
                self.material_history_before = app.editor.undo_stack.len();
                Self::click_widget(app, "button", "Brushed metal", false);
                self.state = State::ReleasePreset;
            }
            State::ReleasePreset if frame == 89 => {
                Self::ui_release(app);
                self.state = State::CheckPreset;
            }
            State::DragMaterial if (94..=97).contains(&frame) => match frame {
                94 => Self::drag_material(app, 0.25, false),
                95 => Self::drag_material(app, 0.5, true),
                96 => Self::drag_material(app, 0.75, true),
                _ => {
                    Self::ui_release(app);
                    self.state = State::CheckMaterialDrag;
                }
            },
            State::UndoMaterial if (102..=107).contains(&frame) => match frame {
                102 => Self::click_menu(app, "Edit", None),
                103 => Self::ui_release(app),
                106 => Self::click_menu(app, "Edit", Some("Undo")),
                107 => {
                    Self::ui_release(app);
                    self.state = State::CheckMaterialUndo;
                }
                _ => {}
            },
            State::RedoMaterial if (112..=117).contains(&frame) => match frame {
                112 => Self::click_menu(app, "Edit", None),
                113 => Self::ui_release(app),
                116 => Self::click_menu(app, "Edit", Some("Redo")),
                117 => {
                    Self::ui_release(app);
                    self.state = State::CheckMaterialRedo;
                }
                _ => {}
            },
            State::CollapseMaterial if (122..=123).contains(&frame) => {
                if frame == 122 {
                    Self::click_widget(app, "section", "Material", false);
                } else {
                    Self::ui_release(app);
                    self.state = State::PressAddComponent;
                }
            }
            State::PressAddComponent if frame == 128 => {
                Self::click_widget(app, "button", "+ Add Component", false);
                self.state = State::ReleaseAddComponent;
            }
            State::ReleaseAddComponent if frame == 129 => {
                Self::ui_release(app);
                self.state = State::ShotAddOpen;
            }
            State::PressAddRow if frame == 135 => {
                Self::click_widget(app, "text", "Collider", false);
                self.state = State::ReleaseAddRow;
            }
            State::ReleaseAddRow if frame == 136 => {
                Self::ui_release(app);
                self.state = State::CheckAddComponent;
            }
            State::PressRemoveComponent if frame == 143 => {
                Self::click_widget(app, "section", "Collider", true);
                self.state = State::ReleaseRemoveComponent;
            }
            State::ReleaseRemoveComponent if frame == 144 => {
                Self::ui_release(app);
                self.state = State::CheckRemoveComponent;
            }
            State::PrefabWalkthrough => match frame {
                150 => Self::ui_press(app, (260.0, 526.0)),
                151 | 155 | 157 | 161 | 163 | 169 | 175 => Self::ui_release(app),
                154 | 156 => Self::click_widget(app, "text", "prefabs", false),
                160 | 162 => Self::click_widget(app, "prefix", "chair.kat", false),
                168 => Self::click_widget(app, "icon", "Play", false),
                174 => Self::click_widget(app, "icon", "Stop", false),
                _ => {}
            },
            _ => {}
        }
    }

    /// Called after each headless frame rendered. Returns a screenshot
    /// destination when this frame should be captured.
    #[cfg(feature = "editor")]
    pub fn end_frame(&mut self, app: &mut Application, frame: usize) -> Option<String> {
        match self.state {
            State::Idle if frame == 10 => {
                self.screenshots_taken += 1;
                self.state = State::PressHierarchy;
                Some(self.screenshot_path("01_default"))
            }
            State::CheckHierarchy if frame == 17 => {
                let name = Self::selected_name(app);
                self.record(
                    "hierarchy_click_selects_sphere_1_0",
                    name.as_deref() == Some("Sphere_1_0"),
                    format!("selected after click: {:?}", name),
                );
                self.state = State::ShotHierarchy;
                None
            }
            State::ShotHierarchy if frame == 18 => {
                self.screenshots_taken += 1;
                self.state = State::ScrollDown;
                Some(self.screenshot_path("02_hierarchy_selected"))
            }
            State::ShotScrolledDown if frame == 26 => {
                self.screenshots_taken += 1;
                self.state = State::ScrollUp;
                Some(self.screenshot_path("03_hierarchy_scrolled_down"))
            }
            State::ShotScrolledUp if frame == 34 => {
                self.screenshots_taken += 1;
                self.state = State::HoverViewport;
                Some(self.screenshot_path("04_hierarchy_scrolled_up"))
            }
            State::ShotViewport if frame == 42 => {
                let name = Self::selected_name(app);
                let picked = name.as_deref() == Some("CenterCube");
                self.record(
                    "viewport_click_picks_object",
                    picked,
                    format!("selected after pick: {:?}", name),
                );
                self.screenshots_taken += 1;
                self.state = State::PressEmpty;
                Some(self.screenshot_path("05_viewport_picked"))
            }
            State::ShotEmpty if frame == 48 => {
                let selected = app.editor.editor_ui.selected_entity;
                self.record(
                    "empty_click_deselects",
                    selected.is_none(),
                    format!("selected after empty click: {:?}", selected),
                );
                self.screenshots_taken += 1;
                self.state = State::PressConsoleTab;
                Some(self.screenshot_path("06_deselected"))
            }
            State::ShotConsoleTab if frame == 56 => {
                self.screenshots_taken += 1;
                self.state = State::OpenPreferences;
                Some(self.screenshot_path("07_console_tab"))
            }
            State::ShotLightSwatch if frame == 66 => {
                let theme = app.editor.editor_ui.theme_name().to_string();
                self.record(
                    "light_theme_applies",
                    theme.eq_ignore_ascii_case("light"),
                    format!("theme after swatch click: {}", theme),
                );
                self.screenshots_taken += 1;
                self.state = State::PressDarkSwatch;
                Some(self.screenshot_path("08_preferences_light"))
            }
            State::ShotDarkSwatch if frame == 72 => {
                let theme = app.editor.editor_ui.theme_name().to_string();
                self.record(
                    "dark_theme_restores",
                    theme.eq_ignore_ascii_case("dark"),
                    format!("theme after restore click: {}", theme),
                );
                self.screenshots_taken += 1;
                self.state = State::PressClose;
                Some(self.screenshot_path("09_preferences_dark"))
            }
            State::ShotClose if frame == 78 => {
                let visible = app.editor.editor_ui.preferences_panel_visible();
                self.record(
                    "preferences_close_works",
                    !visible,
                    format!("preferences panel visible after close: {}", visible),
                );
                self.screenshots_taken += 1;
                self.state = State::PressHierarchyAgain;
                Some(self.screenshot_path("10_preferences_closed"))
            }
            State::CheckPreset if frame == 92 => {
                let values = Self::selected_material(app);
                self.record(
                    "material_preset_applies",
                    values.is_some_and(|v| {
                        v.metallic == 1.0
                            && v.roughness
                                == katla_agent::material::MaterialPreset::BrushedMetal
                                    .values()
                                    .roughness
                    }),
                    format!("material: {values:?}"),
                );
                self.state = State::DragMaterial;
                self.screenshots_taken += 1;
                Some(self.screenshot_path("11_material_preset"))
            }
            State::CheckMaterialDrag if frame == 100 => {
                let values = Self::selected_material(app);
                self.record(
                    "material_drag_follows_pointer",
                    values.is_some_and(|v| (v.roughness - 0.75).abs() < 0.001),
                    format!("material: {values:?}"),
                );
                let history = app.editor.undo_stack.len() - self.material_history_before;
                self.record(
                    "material_drag_is_one_undo",
                    history == 2,
                    format!("preset plus gesture history entries: {history}"),
                );
                self.state = State::UndoMaterial;
                self.screenshots_taken += 1;
                Some(self.screenshot_path("12_material_drag"))
            }
            State::CheckMaterialUndo if frame == 110 => {
                let values = Self::selected_material(app);
                self.record(
                    "edit_menu_undo_restores_material",
                    values.is_some_and(|v| {
                        v.roughness
                            == katla_agent::material::MaterialPreset::BrushedMetal
                                .values()
                                .roughness
                    }),
                    format!("material: {values:?}"),
                );
                self.state = State::RedoMaterial;
                self.screenshots_taken += 1;
                Some(self.screenshot_path("13_material_undo"))
            }
            State::CheckMaterialRedo if frame == 120 => {
                let values = Self::selected_material(app);
                self.record(
                    "edit_menu_redo_restores_material",
                    values.is_some_and(|v| (v.roughness - 0.75).abs() < 0.001),
                    format!("material: {values:?}"),
                );
                self.state = State::CollapseMaterial;
                self.screenshots_taken += 1;
                Some(self.screenshot_path("14_material_redo"))
            }
            State::ShotAddOpen if frame == 132 => {
                self.screenshots_taken += 1;
                self.state = State::PressAddRow;
                Some(self.screenshot_path("15_add_component_open"))
            }
            State::CheckAddComponent if frame == 139 => {
                let has_collider =
                    Self::selected_has_component::<katla_physics::ColliderShape>(app);
                self.record(
                    "add_component_click_adds_collider",
                    has_collider,
                    format!(
                        "ColliderShape on selected entity after pick: {}",
                        has_collider
                    ),
                );
                self.screenshots_taken += 1;
                self.state = State::PressRemoveComponent;
                Some(self.screenshot_path("16_component_added"))
            }
            State::CheckRemoveComponent if frame == 147 => {
                let has_collider =
                    Self::selected_has_component::<katla_physics::ColliderShape>(app);
                self.record(
                    "remove_component_click_removes_collider",
                    !has_collider,
                    format!(
                        "ColliderShape on selected entity after remove: {}",
                        has_collider
                    ),
                );
                self.screenshots_taken += 1;
                self.state = State::PrefabWalkthrough;
                Some(self.screenshot_path("17_component_removed"))
            }
            State::PrefabWalkthrough if frame == 166 => {
                let selected = app.editor.editor_ui.selected_entity;
                let name = Self::selected_name(app);
                let size =
                    selected.and_then(|id| crate::systems::subtree_render_bounds(&app.world, id));
                let children = selected.map(|root| {
                    app.world
                        .query_ref::<&crate::components::Parent>()
                        .filter(|(_, p)| p.parent == root)
                        .count()
                });
                self.record(
                    "double_click_prefab_creates_complete_selected_subtree",
                    name.as_deref() == Some("Chair") && children == Some(2) && size.is_some(),
                    format!("selected={name:?}, children={children:?}, bounds={size:?}"),
                );
                self.screenshots_taken += 1;
                Some(self.screenshot_path("18_prefab_instantiated"))
            }
            State::PrefabWalkthrough if frame == 172 => {
                self.record(
                    "play_button_starts_prefab_preview",
                    app.play_mode == crate::application::game_state::PlayMode::Playing,
                    format!("mode={:?}", app.play_mode),
                );
                self.screenshots_taken += 1;
                Some(self.screenshot_path("19_prefab_play"))
            }
            State::PrefabWalkthrough if frame == 178 => {
                let roots = app
                    .world
                    .query_ref::<&NameComponent>()
                    .filter(|(_, name)| name.name == "Chair")
                    .count();
                self.record(
                    "stop_button_restores_authored_prefab",
                    app.play_mode == crate::application::game_state::PlayMode::Editing
                        && roots == 1,
                    format!("mode={:?}, roots={roots}", app.play_mode),
                );
                self.state = State::Done;
                self.screenshots_taken += 1;
                Some(self.screenshot_path("20_prefab_stopped"))
            }
            State::Done if frame >= 179 => {
                let passed = self.checks.iter().filter(|c| c.passed).count();
                info!(
                    "Interaction test summary: {}/{} checks passed",
                    passed,
                    self.checks.len()
                );
                for check in &self.checks {
                    info!(
                        "  {} {}: {}",
                        if check.passed { "PASS" } else { "FAIL" },
                        check.name,
                        check.detail
                    );
                }
                None
            }
            _ => None,
        }
    }

    /// Log the final summary (also covers runs that end before the summary frame).
    pub fn log_summary(&self) {
        let passed = self.checks.iter().filter(|c| c.passed).count();
        info!(
            "Interaction test complete: {} screenshots, {}/{} checks passed",
            self.screenshots_taken,
            passed,
            self.checks.len()
        );
    }
}
