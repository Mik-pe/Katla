//! Headless interaction test: drives synthetic mouse input through the real
//! UI hit-testing pipeline and the viewport GPU-picking path, capturing a
//! screenshot at each state plus programmatic checks.
//!
//! Click coordinates are logical pixels in the 1280x720 headless layout,
//! measured from `--dump-layout` output of the default scene.

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
    /// Green torus on the right side of the viewport — clear of the selected
    /// entity's gizmo hit zone (axes extend ~12px around their lines).
    pub const VIEWPORT_OBJECT: (f32, f32) = (803.0, 255.0);
    /// Empty sky above the torus, away from all geometry.
    pub const VIEWPORT_EMPTY_SKY: (f32, f32) = (940.0, 110.0);
    /// "Console" tab in the bottom dock strip (tabs at y 525..555).
    pub const CONSOLE_TAB: (f32, f32) = (211.0, 540.0);
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

    /// Synthetic UI press: position the mouse and press the left button.
    /// Widgets see `mouse_clicked` during this frame's `process_input`.
    #[cfg(feature = "editor")]
    fn ui_press(app: &mut Application, pos: (f32, f32)) {
        let input = app.ui_context.input_mut();
        input.set_mouse_pos(Vec2::new(pos.0, pos.1));
        input.set_mouse_button(katla_ui::mouse_button::LEFT, true);
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
                let picked = name.is_some() && name.as_deref() != Some("Sphere_1_0");
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
                self.state = State::Done;
                Some(self.screenshot_path("10_preferences_closed"))
            }
            State::Done if frame == 88 => {
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
