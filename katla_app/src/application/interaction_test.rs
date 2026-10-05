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
    NumericWalkthrough,
    MixerWalkthrough,
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
    #[cfg(feature = "editor")]
    material_preview_region: Option<[u32; 4]>,
    #[cfg(feature = "editor")]
    library_preview_regions: [Option<[u32; 4]>; 6],
    #[cfg(feature = "editor")]
    numeric_before: Option<(katla_ecs::EntityId, katla_math::Vec3, usize)>,
    #[cfg(feature = "editor")]
    imported_preview_region: Option<[u32; 4]>,
    #[cfg(feature = "editor")]
    imported_maps: Option<std::sync::Arc<super::editor::preview_maps::PreviewMaps>>,
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
            #[cfg(feature = "editor")]
            material_preview_region: None,
            #[cfg(feature = "editor")]
            library_preview_regions: [None; 6],
            #[cfg(feature = "editor")]
            numeric_before: None,
            #[cfg(feature = "editor")]
            imported_preview_region: None,
            #[cfg(feature = "editor")]
            imported_maps: None,
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
            button::Button, image_button::ImageButton, number_input::NumberInput, section::Section,
            text::Text,
        };
        let tree = app.editor.editor_ui.view_tree();
        let position = tree.iter_nodes().find_map(|(id, node)| {
            let any = node.widget.as_any();
            let matches = match kind {
                "number" => any
                    .downcast_ref::<NumberInput>()
                    .is_some_and(|w| w.label == label),
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
    fn preview_region(app: &Application, texture: Option<katla_ui::TextureId>) -> Option<[u32; 4]> {
        use katla_ui::declarative::widgets::image::Image;
        let texture = texture?;
        let tree = app.editor.editor_ui.view_tree();
        tree.iter_nodes().find_map(|(id, node)| {
            let image = node.widget.as_any().downcast_ref::<Image>()?;
            if image.texture != texture {
                return None;
            }
            let bounds = tree.resolved_bounds().get(&id)?;
            let min = bounds.min * app.scale_factor;
            let max = bounds.max * app.scale_factor;
            Some([
                min.x().floor() as u32,
                min.y().floor() as u32,
                max.x().ceil() as u32,
                max.y().ceil() as u32,
            ])
        })
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
    fn drag_slider(app: &mut Application, label: &str, value: f32, outside_row: bool) {
        use katla_ui::declarative::widgets::labeled_slider::LabeledSlider;
        let tree = app.editor.editor_ui.view_tree();
        let position = tree.iter_nodes().find_map(|(id, node)| {
            let slider = node.widget.as_any().downcast_ref::<LabeledSlider>()?;
            if slider.label != label {
                return None;
            }
            let track = slider.track_bounds(*tree.resolved_bounds().get(&id)?);
            Some((
                track.min.x() + track.width() * value,
                track.center().y() + if outside_row { 32.0 } else { 0.0 },
            ))
        });
        if let Some(position) = position {
            log::info!("Native slider {label} to {value} at {position:?}");
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
            "preview_regions": { "library": self.library_preview_regions, "inspector": self.material_preview_region, "imported": self.imported_preview_region },
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
            State::PressPreset if frame == 86 => {
                Self::click_widget(app, "button", "Browse materials", false);
            }
            State::PressPreset if frame == 87 => {
                Self::ui_release(app);
            }
            State::PressPreset if frame == 88 => {
                self.material_history_before = app.editor.undo_stack.len();
                Self::click_widget(app, "text", "Brushed metal", false);
                self.state = State::ReleasePreset;
            }
            State::ReleasePreset if frame == 89 => {
                Self::ui_release(app);
                self.state = State::CheckPreset;
            }
            State::DragMaterial if (94..=97).contains(&frame) => match frame {
                94 => Self::drag_slider(app, "Roughness", 0.25, false),
                95 => Self::drag_slider(app, "Roughness", 0.5, true),
                96 => Self::drag_slider(app, "Roughness", 0.75, true),
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
            State::NumericWalkthrough => match frame {
                182 => Self::ui_press(app, target::HIERARCHY_SPHERE_1_0),
                183 | 191 | 199 | 201 | 207 | 209 | 215 | 223 => Self::ui_release(app),
                188 => {
                    self.numeric_before = app.editor.editor_ui.selected_entity.and_then(|id| {
                        app.world
                            .get_component::<crate::components::TransformComponent>(id)
                            .map(|t| (id, t.transform.position, app.editor.undo_stack.len()))
                    });
                    Self::click_widget(app, "number", "Position X", false);
                }
                189 | 190 => {
                    let input = app.ui_context.input_mut();
                    input.set_mouse_pos(input.mouse_pos + Vec2::new(20.0, 230.0));
                }
                198 | 206 => Self::click_menu(app, "Edit", None),
                200 => Self::click_menu(app, "Edit", Some("Undo")),
                208 => Self::click_menu(app, "Edit", Some("Redo")),
                214 => Self::click_widget(app, "number", "Rotation Z", false),
                216 => {
                    let input = app.ui_context.input_mut();
                    input.characters.extend("45".chars());
                    input.keys_pressed.push(katla_ui::KeyCode::Enter);
                }
                225 => app
                    .ui_context
                    .input_mut()
                    .keys_pressed
                    .push(katla_ui::KeyCode::Enter),
                222 => Self::click_widget(app, "number", "Scale X", false),
                224 => app.ui_context.input_mut().characters.extend("1e99".chars()),
                230 => app
                    .ui_context
                    .input_mut()
                    .keys_pressed
                    .push(katla_ui::KeyCode::Escape),
                232 => {
                    let input = app.ui_context.input_mut();
                    input.characters.push('2');
                    input.keys_pressed.push(katla_ui::KeyCode::Enter);
                }
                237 => app
                    .ui_context
                    .input_mut()
                    .keys_pressed
                    .push(katla_ui::KeyCode::ArrowUp),
                238 => {
                    let path = app.resources.model_path("DamagedHelmet.glb");
                    match app.spawn_gltf_model(path, [0.0, 0.0, 0.0], None) {
                        Ok(id) => app.editor.editor_ui.selected_entity = Some(id),
                        Err(error) => log::error!("Native imported preview fixture: {error}"),
                    }
                }
                240 => Self::click_widget(app, "section", "Material", false),
                241 => Self::ui_release(app),
                246 => {
                    if let Some(id) = app.editor.editor_ui.selected_entity
                        && let Some(textures) = app
                            .world
                            .get_component_mut::<super::spawning::ModelTextures>(id)
                    {
                        self.imported_maps = textures.preview_maps.take();
                    }
                }
                262 => {
                    use katla_gfx::GpuRenderer;
                    if let Err(error) = app.renderer.resize(1920, 1200) {
                        log::error!("Native responsive resize failed: {error}");
                    }
                }
                254 => {
                    if let Some(id) = app.editor.editor_ui.selected_entity
                        && let Some(textures) = app
                            .world
                            .get_component_mut::<super::spawning::ModelTextures>(id)
                    {
                        textures.preview_maps = self.imported_maps.take();
                    }
                }
                _ => {}
            },
            State::MixerWalkthrough => match frame {
                274 => {
                    use katla_gfx::GpuRenderer;
                    if let Err(error) = app.renderer.resize(2560, 1440) {
                        log::error!("Native mixer resize failed: {error}");
                    }
                }
                278 => Self::ui_press(app, (510.0, 526.0)),
                279 | 285 | 291 => Self::ui_release(app),
                284 => Self::drag_slider(app, "Master", 0.0, false),
                290 => Self::drag_slider(app, "Master", 0.75, false),
                298 => Self::click_widget(app, "button", "Browse materials", false),
                299 => Self::ui_release(app),
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
                self.library_preview_regions = app
                    .editor
                    .material_previews
                    .presets()
                    .map(|texture| Self::preview_region(app, texture));
                self.record(
                    "material_palette_renders_all_previews",
                    self.library_preview_regions.iter().all(Option::is_some),
                    format!("regions: {:?}", self.library_preview_regions),
                );
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
                self.material_preview_region =
                    Self::preview_region(app, app.editor.material_previews.current());
                self.record(
                    "selected_material_has_live_preview",
                    self.material_preview_region.is_some(),
                    format!("region: {:?}", self.material_preview_region),
                );
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
                self.state = State::NumericWalkthrough;
                self.screenshots_taken += 1;
                Some(self.screenshot_path("20_prefab_stopped"))
            }
            State::NumericWalkthrough if matches!(frame, 196 | 204 | 212) => {
                let position = app.editor.editor_ui.selected_entity.and_then(|id| {
                    app.world
                        .get_component::<crate::components::TransformComponent>(id)
                        .map(|t| t.transform.position)
                });
                let expected = self.numeric_before.map(|(_, x, _)| {
                    x + katla_math::Vec3::new(if frame == 204 { 0.0 } else { 0.4 }, 0.0, 0.0)
                });
                let same_entity = self
                    .numeric_before
                    .is_some_and(|(id, _, _)| app.editor.editor_ui.selected_entity == Some(id));
                let one_step = self.numeric_before.is_some_and(|(_, _, count)| {
                    app.editor.undo_stack.len() == count + usize::from(frame != 204)
                });
                self.record(
                    match frame {
                        196 => "numeric_scrub_outside_row_is_one_undo_step",
                        204 => "numeric_undo_restores_transform",
                        _ => "numeric_redo_restores_transform",
                    },
                    same_entity
                        && one_step
                        && position
                            .zip(expected)
                            .is_some_and(|(a, b)| (a - b).length() < 0.001),
                    format!(
                        "position={position:?}, expected={expected:?}, history_single={one_step}"
                    ),
                );
                self.screenshots_taken += 1;
                Some(self.screenshot_path(&format!("21_numeric_{frame}")))
            }
            State::NumericWalkthrough if matches!(frame, 220 | 228 | 236) => {
                let transform = app.editor.editor_ui.selected_entity.and_then(|id| {
                    app.world
                        .get_component::<crate::components::TransformComponent>(id)
                });
                let pass = transform.is_some_and(|t| match frame {
                    220 => {
                        (t.transform.rotation.to_euler().2 - std::f32::consts::FRAC_PI_4).abs()
                            < 0.001
                    }
                    228 => (t.transform.scale.x() - 1.0).abs() < 0.001,
                    _ => (t.transform.scale.x() - 2.0).abs() < 0.001,
                });
                self.record(
                    match frame {
                        220 => "numeric_keyboard_degrees_convert_to_scene_radians",
                        228 => "numeric_nonfinite_draft_preserves_scene",
                        _ => "numeric_invalid_entry_recovers_with_escape",
                    },
                    pass,
                    format!("transform={:?}", transform.map(|t| &t.transform)),
                );
                self.screenshots_taken += 1;
                Some(self.screenshot_path(&format!("22_keyboard_{frame}")))
            }
            State::NumericWalkthrough if frame == 237 => {
                let scale = app
                    .editor
                    .editor_ui
                    .selected_entity
                    .and_then(|id| {
                        app.world
                            .get_component::<crate::components::TransformComponent>(id)
                    })
                    .map(|t| t.transform.scale.x());
                // The action is drained on the following frame after retained state changes.
                self.record(
                    "numeric_arrow_keeps_keyboard_focus",
                    scale == Some(2.0) && app.editor.editor_ui.view_tree().interaction().focused_id.and_then(|id| {
                            let tree = app.editor.editor_ui.view_tree();
                            let widget = tree.get(id)?.widget.as_any()
                                .downcast_ref::<katla_ui::declarative::widgets::number_input::NumberInput>()?;
                            tree.state_arena().get::<katla_ui::declarative::widgets::number_input::NumberState>(widget.state_id)
                        }).is_some_and(|s| (s.value - 2.01).abs() < 0.001),
                    format!("scale={scale:?}"),
                );
                None
            }
            State::NumericWalkthrough if frame == 244 => {
                self.imported_preview_region =
                    Self::preview_region(app, app.editor.material_previews.current());
                let maps = app
                    .editor
                    .editor_ui
                    .selected_entity
                    .and_then(|id| {
                        app.world
                            .get_component::<super::spawning::ModelTextures>(id)
                    })
                    .and_then(|textures| textures.preview_maps.as_ref());
                self.record(
                    "imported_pbr_maps_mount_live_preview",
                    maps.is_some_and(|m| {
                        m.albedo.is_some() && m.normal.is_some() && m.metallic_roughness.is_some()
                    }) && self.imported_preview_region.is_some(),
                    format!("region={:?}", self.imported_preview_region),
                );
                self.screenshots_taken += 1;
                Some(self.screenshot_path("23_imported_maps"))
            }
            State::NumericWalkthrough if frame == 252 => {
                self.screenshots_taken += 1;
                Some(self.screenshot_path("24_imported_factors"))
            }
            State::NumericWalkthrough if frame == 260 => {
                self.screenshots_taken += 1;
                Some(self.screenshot_path("25_imported_restored"))
            }
            State::NumericWalkthrough if frame == 272 => {
                let tree = app.editor.editor_ui.view_tree();
                let fields: Vec<_> = tree.iter_nodes().filter_map(|(id, node)| {
                    node.widget.as_any().downcast_ref::<katla_ui::declarative::widgets::number_input::NumberInput>()?;
                    tree.resolved_bounds().get(&id)
                }).collect();
                let sliders_fit =
                    tree.iter_nodes()
                        .filter(|(_, n)| {
                            n.widget.as_any()
                    .is::<katla_ui::declarative::widgets::labeled_slider::LabeledSlider>()
                        })
                        .all(|(id, _)| {
                            tree.resolved_bounds()
                                .get(&id)
                                .is_some_and(|b| b.max.x() <= 948.0)
                        });
                self.record(
                    "narrow_layout_keeps_numeric_fields_inside_inspector",
                    fields.len() == 9
                        && sliders_fit
                        && fields.iter().all(|b| {
                            b.min.x() >= 960.0 * 0.78 && b.max.x() <= 960.0 && b.width() >= 44.0
                        }),
                    format!("field_bounds={fields:?}"),
                );
                self.state = State::MixerWalkthrough;
                self.screenshots_taken += 1;
                Some(self.screenshot_path("26_narrow_layout"))
            }
            State::MixerWalkthrough if frame == 282 => {
                let tree = app.editor.editor_ui.view_tree();
                let buses: Vec<_> = tree.iter_nodes().filter_map(|(id, node)| {
                    let slider = node.widget.as_any().downcast_ref::<katla_ui::declarative::widgets::labeled_slider::LabeledSlider>()?;
                    ["Master", "SFX", "Music", "Ambient"].contains(&slider.label.as_str())
                        .then(|| tree.resolved_bounds().get(&id)).flatten()
                }).collect();
                self.record(
                    "mixer_four_buses_fit_central_panel",
                    buses.len() == 4
                        && buses
                            .iter()
                            .all(|b| b.min.x() >= 211.0 && b.max.x() <= 994.0 && b.max.y() < 698.0),
                    format!("buses={buses:?}"),
                );
                self.screenshots_taken += 1;
                Some(self.screenshot_path("27_mixer"))
            }
            State::MixerWalkthrough if frame == 288 || frame == 294 => {
                let actual = app.preferences.audio.master_volume;
                let expected = if frame == 288 { 0.0 } else { 0.75 };
                self.record(
                    if frame == 288 {
                        "mixer_zero_volume_is_retained"
                    } else {
                        "mixer_volume_recovers_after_mute"
                    },
                    (actual - expected).abs() < 0.001,
                    format!("volume={actual}"),
                );
                None
            }
            State::MixerWalkthrough if frame == 302 => {
                self.state = State::Done;
                self.screenshots_taken += 1;
                Some(self.screenshot_path("28_assets_restored"))
            }
            State::Done if frame >= 303 => {
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
