//! Preferences Panel Widget
//!
//! Contains the preferences/settings panel for configuring editor options.

use katla_math::{Rect2D, Vec2};
use katla_ui::{
    widgets::{Button, DraggablePanel, DraggablePanelState, DraggablePanelStyle},
    FontId, FontSize, ForkAwesome, Response, ScrollArea, ScrollAreaState, UiContext, Widget,
};

use crate::Preferences;

use super::Theme;

impl From<&Theme> for DraggablePanelStyle {
    fn from(theme: &Theme) -> Self {
        Self {
            panel_bg: theme.panel_bg,
            panel_border: theme.panel_border,
            panel_header: theme.panel_header,
            background_light: theme.background_light,
            text_primary: theme.text_primary,
            text_muted: theme.text_muted,
        }
    }
}

/// Preferences panel tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferencesTab {
    #[default]
    Appearance,
    Editor,
    Keybindings,
    About,
}

impl PreferencesTab {
    pub fn all() -> &'static [PreferencesTab] {
        &[
            PreferencesTab::Appearance,
            PreferencesTab::Editor,
            PreferencesTab::Keybindings,
            PreferencesTab::About,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            PreferencesTab::Appearance => "Appearance",
            PreferencesTab::Editor => "Editor",
            PreferencesTab::Keybindings => "Keybindings",
            PreferencesTab::About => "About",
        }
    }

    pub fn icon(&self) -> char {
        match self {
            PreferencesTab::Appearance => ForkAwesome::PAINT_BRUSH,
            PreferencesTab::Editor => ForkAwesome::PENCIL,
            PreferencesTab::Keybindings => ForkAwesome::KEY,
            PreferencesTab::About => ForkAwesome::INFO_CIRCLE,
        }
    }
}

/// Session-only editor settings (not persisted between sessions).
#[derive(Debug, Clone)]
pub struct EditorSettings {
    pub snap_to_grid: bool,
    pub camera_speed: f32,
    pub grid_size: f32,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            snap_to_grid: true,
            camera_speed: 50.0,
            grid_size: 1.0,
        }
    }
}

/// Internal state for the preferences panel widget.
#[derive(Debug, Clone, Default)]
pub struct PreferencesPanelState {
    pub panel: DraggablePanelState,
    pub current_tab: PreferencesTab,
    pub scroll_state: ScrollAreaState,
}

/// Actions emitted by the preferences panel.
#[derive(Debug, Clone)]
pub enum PreferencesAction {
    SetTheme(String),
    ToggleGrid,
    ToggleStats,
    SetFontScale(f32),
    SetSnapToGrid(bool),
    SetCameraSpeed(f32),
    SetGridSize(f32),
}

pub struct PreferencesPanel<'a> {
    pub screen_size: Vec2,
    pub state: &'a mut PreferencesPanelState,
    pub preferences: &'a Preferences,
    pub editor_settings: &'a EditorSettings,
    pub theme: &'a Theme,
    pub theme_key: &'a str,
    pub pending_actions: &'a mut Vec<PreferencesAction>,
}

impl<'a> PreferencesPanel<'a> {
    pub fn new(
        screen_size: Vec2,
        state: &'a mut PreferencesPanelState,
        preferences: &'a Preferences,
        editor_settings: &'a EditorSettings,
        theme: &'a Theme,
        theme_key: &'a str,
        pending_actions: &'a mut Vec<PreferencesAction>,
    ) -> Self {
        Self {
            screen_size,
            state,
            preferences,
            editor_settings,
            theme,
            theme_key,
            pending_actions,
        }
    }
}

impl<'a> Widget for PreferencesPanel<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let style = DraggablePanelStyle::from(self.theme);
        let panel_width = 450.0;
        let panel_height = 500.0;
        let title_bar_height = DraggablePanel::title_bar_height();
        let tab_bar_height = 36.0;

        let frame = DraggablePanel::begin(
            ui,
            "prefs",
            "Settings",
            panel_width,
            panel_height,
            self.screen_size,
            &mut self.state.panel,
            &style,
        );

        let panel_bounds = frame.panel_bounds;

        let tab_bar_bounds = Rect2D::from_origin_size(
            Vec2::new(
                panel_bounds.min.x(),
                panel_bounds.min.y() + title_bar_height,
            ),
            Vec2::new(panel_width, tab_bar_height),
        );
        ui.draw_rect(tab_bar_bounds, self.theme.background_dark);

        let tab_width = panel_width / PreferencesTab::all().len() as f32;
        for (i, tab) in PreferencesTab::all().iter().enumerate() {
            let tab_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    panel_bounds.min.x() + i as f32 * tab_width,
                    tab_bar_bounds.min.y(),
                ),
                Vec2::new(tab_width, tab_bar_height),
            );
            let is_selected = *tab == self.state.current_tab;

            let tab_color = if is_selected {
                self.theme.panel_bg
            } else {
                self.theme.background_dark
            };
            ui.draw_rect(tab_bounds, tab_color);

            if is_selected {
                ui.draw_line(
                    Vec2::new(tab_bounds.min.x(), tab_bounds.max.y()),
                    Vec2::new(tab_bounds.max.x(), tab_bounds.max.y()),
                    self.theme.selection,
                    2.0,
                );
            }

            if ui
                .add(
                    Button::new("")
                        .bounds(tab_bounds)
                        .id(&format!("tab_{:?}", tab)),
                )
                .clicked
                && !is_selected
            {
                self.state.current_tab = *tab;
            }

            let icon = tab.icon();
            let icon_size = ui.scaled_font_size(FontSize::Medium);
            let text = tab.name();
            let text_size = ui.measure_text(text, ui.scaled_font_size(FontSize::Small));
            let total_width = icon_size + 4.0 + text_size.x();
            let start_x = tab_bounds.center().x() - total_width * 0.5;
            let top_y = tab_bounds.center().y() - text_size.y() * 0.5;

            let icon_color = if is_selected {
                self.theme.text_primary
            } else {
                self.theme.text_muted
            };
            ui.draw_icon_aligned(
                icon,
                Vec2::new(start_x, top_y),
                icon_size,
                icon_color,
                FontId::DEFAULT,
            );

            let text_color = if is_selected {
                self.theme.text_primary
            } else {
                self.theme.text_muted
            };
            ui.draw_text(
                text,
                Vec2::new(start_x + icon_size + 4.0, top_y),
                text_color,
                ui.scaled_font_size(FontSize::Small),
            );
        }

        let content_start_y = panel_bounds.min.y() + title_bar_height + tab_bar_height + 8.0;
        let content_height = panel_height - title_bar_height - tab_bar_height - 16.0;
        let scroll_bounds = Rect2D::from_origin_size(
            Vec2::new(panel_bounds.min.x(), content_start_y),
            Vec2::new(panel_width, content_height),
        );

        let current_tab = self.state.current_tab;
        let editor_settings = self.editor_settings.clone();
        let theme = self.theme;
        let theme_key = self.theme_key;
        let show_grid = self.preferences.show_grid;
        let show_stats = self.preferences.show_stats;
        let font_scale = self.preferences.font_scale;
        let pending_actions = &mut *self.pending_actions;

        self.state.scroll_state = ui.scroll_area(
            ScrollArea::new("prefs_scroll").max_height(content_height),
            self.state.scroll_state,
            scroll_bounds,
            move |ui| {
                let scroll_offset = ui.scroll_offset();
                let content_width = panel_width - 32.0;
                let row_height = 28.0;
                let spacing = 8.0;

                let cursor =
                    Vec2::new(panel_bounds.min.x() + 16.0, content_start_y - scroll_offset);

                let final_y = match current_tab {
                    PreferencesTab::Appearance => build_appearance_tab(
                        ui,
                        theme,
                        cursor,
                        content_width,
                        row_height,
                        spacing,
                        theme_key,
                        show_grid,
                        show_stats,
                        font_scale,
                        pending_actions,
                    ),
                    PreferencesTab::Editor => build_editor_tab(
                        ui,
                        theme,
                        cursor,
                        content_width,
                        row_height,
                        &editor_settings,
                        pending_actions,
                    ),
                    PreferencesTab::Keybindings => {
                        build_keybindings_tab(ui, theme, cursor, content_width, row_height)
                    }
                    PreferencesTab::About => build_about_tab(ui, theme, cursor, content_width),
                };

                final_y - content_start_y + scroll_offset + 16.0
            },
        );

        DraggablePanel::end(&mut self.state.panel, &frame);

        Response::default()
    }
}

#[allow(clippy::too_many_arguments)]
fn build_appearance_tab(
    ui: &mut UiContext,
    theme: &Theme,
    cursor: Vec2,
    content_width: f32,
    _row_height: f32,
    spacing: f32,
    current_theme_key: &str,
    show_grid: bool,
    show_stats: bool,
    font_scale: f32,
    pending_actions: &mut Vec<PreferencesAction>,
) -> f32 {
    ui.set_cursor(cursor);
    ui.label_auto_colored("Color Theme", theme.text_secondary);
    ui.spacing(20.0);

    let col_width = (content_width - spacing) / 2.0;
    let row_height = 24.0;

    let theme_names = [
        ("catppuccin", "Catppuccin"),
        ("nord", "Nord"),
        ("tokyo_night", "Tokyo Night"),
        ("dracula", "Dracula"),
        ("gruvbox", "Gruvbox"),
        ("one_dark", "One Dark"),
        ("material_palenight", "Material Palenight"),
        ("ayu_dark", "Ayu Dark"),
        ("github_dark", "GitHub Dark"),
        ("monokai", "Monokai"),
        ("rose_pine", "Rosé Pine"),
        ("kanagawa", "Kanagawa"),
        ("solarized_dark", "Solarized Dark"),
    ];

    ui.begin_grid(2, col_width, row_height, spacing);
    for (key, display_name) in theme_names.iter() {
        let btn_bounds = ui.grid_item(Vec2::new(col_width, row_height));
        let is_selected = *key == current_theme_key;

        if themed_select_button(
            ui,
            &format!("theme_{}", key),
            display_name,
            btn_bounds,
            is_selected,
            theme,
        ) {
            pending_actions.push(PreferencesAction::SetTheme(key.to_string()));
        }
    }
    ui.end_grid();

    ui.spacing(row_height * 7.0 + 4.0 + 16.0);

    ui.label_auto_colored("View Options", theme.text_secondary);
    ui.spacing(24.0);

    let grid_btn_bounds =
        Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, row_height));
    if ui
        .toggle_button(
            "pref_grid_toggle",
            "Show Grid",
            show_grid,
            grid_btn_bounds,
            theme.success,
            theme.button_bg,
            theme.button_text,
        )
        .clicked
    {
        pending_actions.push(PreferencesAction::ToggleGrid);
    }
    ui.spacing(row_height + 4.0);

    let stats_btn_bounds =
        Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, row_height));
    if ui
        .toggle_button(
            "pref_stats_toggle",
            "Show Stats Panel",
            show_stats,
            stats_btn_bounds,
            theme.success,
            theme.button_bg,
            theme.button_text,
        )
        .clicked
    {
        pending_actions.push(PreferencesAction::ToggleStats);
    }
    ui.spacing(row_height + 16.0);

    ui.label_auto_colored("Font Scale", theme.text_secondary);
    ui.spacing(24.0);

    let font_scales = [
        (0.75, "75%"),
        (0.9, "90%"),
        (1.0, "100%"),
        (1.1, "110%"),
        (1.25, "125%"),
        (1.5, "150%"),
        (1.75, "175%"),
        (2.0, "200%"),
    ];
    let scale_btn_width = (content_width - 3.0 * spacing) / 4.0;

    ui.begin_grid(4, scale_btn_width, row_height, spacing);
    for (scale, label) in font_scales.iter() {
        let btn_bounds = ui.grid_item(Vec2::new(scale_btn_width, row_height));
        let is_selected = (font_scale - scale).abs() < 0.01;

        if themed_select_button(
            ui,
            &format!("font_scale_{}", scale),
            label,
            btn_bounds,
            is_selected,
            theme,
        ) {
            pending_actions.push(PreferencesAction::SetFontScale(*scale));
        }
    }
    ui.end_grid();

    ui.cursor().y()
}

fn build_editor_tab(
    ui: &mut UiContext,
    theme: &Theme,
    cursor: Vec2,
    content_width: f32,
    row_height: f32,
    editor_settings: &EditorSettings,
    pending_actions: &mut Vec<PreferencesAction>,
) -> f32 {
    ui.set_cursor(cursor);
    ui.label_auto_colored("Editor Settings", theme.text_secondary);
    ui.spacing(24.0);

    let snap_btn_bounds =
        Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, row_height));
    if ui
        .toggle_button(
            "pref_snap_toggle",
            "Snap to Grid",
            editor_settings.snap_to_grid,
            snap_btn_bounds,
            theme.success,
            theme.button_bg,
            theme.button_text,
        )
        .clicked
    {
        pending_actions.push(PreferencesAction::SetSnapToGrid(
            !editor_settings.snap_to_grid,
        ));
    }
    ui.spacing(row_height + ui.scaled_font_size(FontSize::Medium));

    ui.label_auto_colored("Camera Speed", theme.text_secondary);
    ui.spacing(20.0);

    let speed_text = format!("{:.0}", editor_settings.camera_speed);
    ui.label_auto_colored(&speed_text, theme.text_primary);
    ui.spacing(20.0);

    let slider_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, 20.0));
    let mut camera_speed = editor_settings.camera_speed;
    let slider_response = ui.add(
        katla_ui::widgets::Slider::new(&mut camera_speed, 5.0..=200.0)
            .bounds(slider_bounds)
            .id("camera_speed_slider"),
    );
    if slider_response.changed {
        pending_actions.push(PreferencesAction::SetCameraSpeed(camera_speed));
    }

    ui.spacing(40.0);

    ui.label_auto_colored(
        &format!("Grid Size: {:.1}", editor_settings.grid_size),
        theme.text_secondary,
    );
    ui.spacing(24.0);

    let sizes = [0.5, 1.0, 2.0, 5.0, 10.0];
    let btn_width = (content_width - 4.0 * 8.0) / 5.0;
    let spacing = 8.0;

    ui.begin_grid(5, btn_width, row_height, spacing);
    for &size in sizes.iter() {
        let btn_bounds = ui.grid_item(Vec2::new(btn_width, row_height));
        let is_selected = (editor_settings.grid_size - size).abs() < 0.01;
        let text = format!("{:.1}", size);
        if themed_select_button(
            ui,
            &format!("grid_size_{}", size),
            &text,
            btn_bounds,
            is_selected,
            theme,
        ) {
            pending_actions.push(PreferencesAction::SetGridSize(size));
        }
    }
    ui.end_grid();

    ui.cursor().y()
}

fn build_keybindings_tab(
    ui: &mut UiContext,
    theme: &Theme,
    cursor: Vec2,
    content_width: f32,
    row_height: f32,
) -> f32 {
    ui.set_cursor(cursor);
    ui.label_auto_colored("Keyboard Shortcuts", theme.text_secondary);
    ui.spacing(24.0);

    let shortcuts = [
        ("Delete", "Delete selected entity"),
        ("↑ / ↓", "Navigate entity list"),
        ("← / →", "Collapse/Expand hierarchy"),
        ("Escape", "Deselect / Close panel"),
        ("T", "Test mesh spawn"),
    ];

    for (key, desc) in shortcuts {
        let row_bounds =
            Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, row_height));
        ui.draw_rect(row_bounds, theme.button_bg);

        let badge_width = 60.0;
        let badge_bounds =
            Rect2D::from_origin_size(ui.cursor(), Vec2::new(badge_width, row_height));
        ui.draw_rect(badge_bounds, theme.background_light);
        let key_size = ui.measure_text(key, ui.scaled_font_size(FontSize::Small));
        ui.draw_text(
            key,
            Vec2::new(
                badge_bounds.center().x() - key_size.x() * 0.5,
                badge_bounds.center().y() - key_size.y() * 0.5,
            ),
            theme.text_accent,
            ui.scaled_font_size(FontSize::Small),
        );

        ui.draw_text(
            desc,
            Vec2::new(
                ui.cursor().x() + badge_width + ui.scaled_font_size(FontSize::Medium),
                ui.cursor().y() + 6.0,
            ),
            theme.text_primary,
            ui.scaled_font_size(FontSize::Medium),
        );

        ui.spacing(row_height + 4.0);
    }

    ui.spacing(16.0);
    ui.label_auto_colored("Custom keybindings coming soon", theme.text_muted);

    ui.cursor().y()
}

fn build_about_tab(ui: &mut UiContext, theme: &Theme, cursor: Vec2, content_width: f32) -> f32 {
    ui.set_cursor(cursor);
    let center_x = cursor.x() + content_width * 0.5;

    let title = "Katla Engine";
    let title_size = ui.measure_text(title, ui.scaled_font_size(FontSize::Huge));
    ui.draw_text(
        title,
        Vec2::new(center_x - title_size.x() * 0.5, ui.cursor().y()),
        theme.text_primary,
        ui.scaled_font_size(FontSize::Huge),
    );
    ui.spacing(40.0);

    let version = "Version 0.1.0";
    let version_size = ui.measure_text(version, ui.scaled_font_size(FontSize::Large));
    ui.draw_text(
        version,
        Vec2::new(center_x - version_size.x() * 0.5, ui.cursor().y()),
        theme.text_secondary,
        ui.scaled_font_size(FontSize::Large),
    );
    ui.spacing(30.0);

    let desc = "A Vulkan-based 3D game engine\nwritten in Rust with ECS architecture.";
    for line in desc.split('\n') {
        let line_size = ui.measure_text(line, ui.scaled_font_size(FontSize::Medium));
        ui.draw_text(
            line,
            Vec2::new(center_x - line_size.x() * 0.5, ui.cursor().y()),
            theme.text_muted,
            ui.scaled_font_size(FontSize::Medium),
        );
        ui.spacing(20.0);
    }

    ui.spacing(30.0);

    ui.draw_text(
        "Features",
        Vec2::new(center_x - 30.0, ui.cursor().y()),
        theme.text_secondary,
        ui.scaled_font_size(FontSize::Medium),
    );
    ui.spacing(24.0);

    let features = [
        "Vulkan 1.3 with Dynamic Rendering",
        "ECS Architecture",
        "Skeletal Animation",
        "Particle Systems",
        "Hot Reloadable Shaders",
        "Immediate Mode UI",
    ];

    let check_icon = ForkAwesome::CHECK;
    let font_size = ui.scaled_font_size(FontSize::Medium);
    for feature in features {
        ui.draw_icon_label(
            check_icon,
            feature,
            Vec2::new(center_x - 100.0, ui.cursor().y()),
            font_size,
            font_size,
            theme.success,
        );
        ui.spacing(18.0);
    }

    ui.cursor().y()
}

fn themed_select_button(
    ui: &mut UiContext,
    id: &str,
    label: &str,
    bounds: Rect2D,
    is_selected: bool,
    theme: &Theme,
) -> bool {
    let clicked = ui.add(Button::new("").bounds(bounds).id(id)).clicked;

    let btn_color = if is_selected {
        theme.selection
    } else {
        theme.button_bg
    };
    ui.draw_rect(bounds, btn_color);

    let text_color = if is_selected {
        theme.button_text
    } else {
        theme.text_primary
    };
    let text_size = ui.measure_text(label, ui.scaled_font_size(FontSize::Small));
    let text_pos = Vec2::new(
        bounds.center().x() - text_size.x() * 0.5,
        bounds.center().y() - text_size.y() * 0.5,
    );
    ui.draw_text(
        label,
        text_pos,
        text_color,
        ui.scaled_font_size(FontSize::Small),
    );

    clicked
}
