//! Preferences Panel Widget
//!
//! Contains the preferences/settings panel for configuring editor options.

use katla_math::{Rect2D, Vec2};
use katla_ui::{
    FontId, FontSize, ForkAwesome, Response, ScrollArea, ScrollAreaState, UiContext, Widget,
    widgets::{Button, DraggablePanel, DraggablePanelConfig, DraggablePanelState, ToggleButton},
};

use crate::Preferences;

use super::ColorScheme;

// --- Spacing & sizing constants ---

/// Horizontal padding on each side of the tab content area.
const HORIZONTAL_PADDING: f32 = 16.0;
/// Standard height for interactive rows (buttons, toggles, text inputs).
const ROW_HEIGHT: f32 = 28.0;
/// Gap between a section header and its first widget.
const HEADER_TO_WIDGET: f32 = 12.0;
/// Gap between related widgets within a section.
const WIDGET_GAP: f32 = 8.0;
/// Gap between sections (separator to next header).
const SECTION_GAP: f32 = 20.0;
/// Gap between a label and its associated widget.
const LABEL_GAP: f32 = 8.0;
/// Grid cell spacing for button grids.
const GRID_SPACING: f32 = 8.0;

/// Preferences panel tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferencesTab {
    #[default]
    General,
    Viewport,
    Ai,
}

impl PreferencesTab {
    pub fn all() -> &'static [PreferencesTab] {
        &[
            PreferencesTab::General,
            PreferencesTab::Viewport,
            PreferencesTab::Ai,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            PreferencesTab::General => "General",
            PreferencesTab::Viewport => "Viewport",
            PreferencesTab::Ai => "AI",
        }
    }

    pub fn icon(&self) -> char {
        match self {
            PreferencesTab::General => ForkAwesome::PAINT_BRUSH,
            PreferencesTab::Viewport => ForkAwesome::CUBE,
            PreferencesTab::Ai => ForkAwesome::CUBE,
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
    /// Snapshot of LLM config, refreshed from EditorState each frame.
    pub llm_config: katla_agent::LlmConfig,
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
    SetLlmProvider(String),
    SetLlmApiKey(String),
    SetLlmBaseUrl(String),
    SetLlmModel(String),
    SetLlmMaxTokens(u32),
    SetLlmTemperature(f32),
    SaveLlmConfig,
}

pub struct PreferencesPanel<'a> {
    pub screen_size: Vec2,
    pub state: &'a mut PreferencesPanelState,
    pub preferences: &'a Preferences,
    pub editor_settings: &'a EditorSettings,
    pub theme: &'a ColorScheme,
    pub theme_key: &'a str,
    pub pending_actions: &'a mut Vec<PreferencesAction>,
}

impl<'a> PreferencesPanel<'a> {
    pub fn new(
        screen_size: Vec2,
        state: &'a mut PreferencesPanelState,
        preferences: &'a Preferences,
        editor_settings: &'a EditorSettings,
        theme: &'a ColorScheme,
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
        let panel_width = 450.0;
        let panel_height = 500.0;
        let title_bar_height = DraggablePanel::title_bar_height();
        let tab_bar_height = 36.0;

        let mut panel_bounds = Rect2D::from_size(Vec2::new(panel_width, panel_height));

        DraggablePanel::show(
            ui,
            &mut self.state.panel,
            DraggablePanelConfig::new("prefs", "Preferences")
                .size(panel_width, panel_height)
                .screen_size(self.screen_size)
                .close_on_outside_click(false),
            |ui, frame| {
                panel_bounds = frame.panel_bounds;

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

                let content_start_y = panel_bounds.min.y()
                    + title_bar_height
                    + tab_bar_height
                    + ui.style().panel_padding;
                let content_height = panel_height
                    - title_bar_height
                    - tab_bar_height
                    - 2.0 * ui.style().panel_padding;
                let scroll_bounds = Rect2D::from_origin_size(
                    Vec2::new(panel_bounds.min.x(), content_start_y),
                    Vec2::new(panel_width, content_height),
                );

                let current_tab = self.state.current_tab;
                let editor_settings = self.editor_settings.clone();
                let llm_config = self.state.llm_config.clone();
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
                        let content_width = panel_width - HORIZONTAL_PADDING * 2.0;

                        let cursor = Vec2::new(
                            panel_bounds.min.x() + HORIZONTAL_PADDING,
                            content_start_y - scroll_offset,
                        );

                        let final_y = match current_tab {
                            PreferencesTab::General => build_general_tab(
                                ui,
                                theme,
                                &GeneralTabParams {
                                    cursor,
                                    content_width,
                                    current_theme_key: theme_key,
                                    font_scale,
                                },
                                pending_actions,
                            ),
                            PreferencesTab::Viewport => build_viewport_tab(
                                ui,
                                theme,
                                cursor,
                                content_width,
                                &editor_settings,
                                show_grid,
                                show_stats,
                                pending_actions,
                            ),
                            PreferencesTab::Ai => build_ai_tab(
                                ui,
                                theme,
                                cursor,
                                content_width,
                                &llm_config,
                                pending_actions,
                            ),
                        };

                        final_y - content_start_y + scroll_offset + SECTION_GAP
                    },
                );
            },
        );

        Response::default()
    }
}

// --- Shared helpers ---

/// Draw a section header: tinted background bar with text label.
fn draw_section_header(ui: &mut UiContext, theme: &ColorScheme, text: &str, content_width: f32) {
    let header_height = 24.0;
    let bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, header_height));
    ui.draw_rect(bounds, theme.background_light);

    let font_size = ui.scaled_font_size(FontSize::Small);
    let text_size = ui.measure_text(text, font_size);
    ui.draw_text(
        text,
        Vec2::new(
            bounds.min.x() + ui.style().panel_padding,
            bounds.center().y() - text_size.y() * 0.5,
        ),
        theme.text_secondary,
        font_size,
    );
    ui.spacing(header_height + HEADER_TO_WIDGET);
}

struct GeneralTabParams<'a> {
    cursor: Vec2,
    content_width: f32,
    current_theme_key: &'a str,
    font_scale: f32,
}

fn build_general_tab(
    ui: &mut UiContext,
    theme: &ColorScheme,
    params: &GeneralTabParams,
    pending_actions: &mut Vec<PreferencesAction>,
) -> f32 {
    let content_width = params.content_width;
    let current_theme_key = params.current_theme_key;
    let font_scale = params.font_scale;

    ui.set_cursor(params.cursor);

    // --- Color Theme section ---
    draw_section_header(ui, theme, "COLOR THEME", content_width);

    let col_width = (content_width - 2.0 * GRID_SPACING) / 3.0;

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

    ui.begin_grid(3, col_width, ROW_HEIGHT, GRID_SPACING);
    for (key, display_name) in theme_names.iter() {
        let btn_bounds = ui.grid_item(Vec2::new(col_width, ROW_HEIGHT));
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

    ui.spacing(SECTION_GAP);

    // --- Font Scale section ---
    draw_section_header(ui, theme, "FONT SCALE", content_width);

    let scale_text = format!("Scale: {:.0}%", font_scale * 100.0);
    ui.label_auto_colored(&scale_text, theme.text_primary);
    ui.spacing(LABEL_GAP);
    let slider_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, 20.0));
    let mut scale = font_scale;
    let response = ui.add(
        katla_ui::widgets::Slider::new("font_scale_slider", &mut scale, 0.75..=2.0)
            .bounds(slider_bounds)
            .id("font_scale_slider"),
    );
    if response.changed {
        pending_actions.push(PreferencesAction::SetFontScale(scale));
    }

    ui.cursor().y()
}

fn build_viewport_tab(
    ui: &mut UiContext,
    theme: &ColorScheme,
    cursor: Vec2,
    content_width: f32,
    editor_settings: &EditorSettings,
    show_grid: bool,
    show_stats: bool,
    pending_actions: &mut Vec<PreferencesAction>,
) -> f32 {
    ui.set_cursor(cursor);

    // --- Display section ---
    draw_section_header(ui, theme, "DISPLAY", content_width);

    let grid_btn_bounds =
        Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, ROW_HEIGHT));
    if ui
        .add(
            ToggleButton::new(show_grid, "Show Grid")
                .bounds(grid_btn_bounds)
                .id("pref_grid_toggle")
                .checked_color(theme.success)
                .unchecked_color(theme.button_bg),
        )
        .clicked
    {
        pending_actions.push(PreferencesAction::ToggleGrid);
    }
    ui.spacing(ROW_HEIGHT + WIDGET_GAP);

    let stats_btn_bounds =
        Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, ROW_HEIGHT));
    if ui
        .add(
            ToggleButton::new(show_stats, "Show Stats Panel")
                .bounds(stats_btn_bounds)
                .id("pref_stats_toggle")
                .checked_color(theme.success)
                .unchecked_color(theme.button_bg),
        )
        .clicked
    {
        pending_actions.push(PreferencesAction::ToggleStats);
    }
    ui.spacing(ROW_HEIGHT + SECTION_GAP);

    // --- Grid section ---
    draw_section_header(ui, theme, "GRID", content_width);

    let sizes = [0.5, 1.0, 2.0, 5.0, 10.0];
    let btn_width = (content_width - 4.0 * GRID_SPACING) / 5.0;

    ui.begin_grid(5, btn_width, ROW_HEIGHT, GRID_SPACING);
    for &size in sizes.iter() {
        let btn_bounds = ui.grid_item(Vec2::new(btn_width, ROW_HEIGHT));
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

    ui.spacing(ROW_HEIGHT + WIDGET_GAP);

    let snap_btn_bounds =
        Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, ROW_HEIGHT));
    if ui
        .add(
            ToggleButton::new(editor_settings.snap_to_grid, "Snap to Grid")
                .bounds(snap_btn_bounds)
                .id("pref_snap_toggle")
                .checked_color(theme.success)
                .unchecked_color(theme.button_bg),
        )
        .clicked
    {
        pending_actions.push(PreferencesAction::SetSnapToGrid(
            !editor_settings.snap_to_grid,
        ));
    }
    ui.spacing(ROW_HEIGHT + SECTION_GAP);

    // --- Camera section ---
    draw_section_header(ui, theme, "CAMERA", content_width);

    let speed_text = format!("Speed: {:.0}", editor_settings.camera_speed);
    ui.label_auto_colored(&speed_text, theme.text_primary);
    ui.spacing(LABEL_GAP);

    let slider_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, 20.0));
    let mut camera_speed = editor_settings.camera_speed;
    let response = ui.add(
        katla_ui::widgets::Slider::new("camera_speed_slider", &mut camera_speed, 5.0..=200.0)
            .bounds(slider_bounds)
            .id("camera_speed_slider"),
    );
    if response.changed {
        pending_actions.push(PreferencesAction::SetCameraSpeed(camera_speed));
    }
    ui.spacing(4.0);
    let tiny_font = ui.scaled_font_size(FontSize::XSmall);
    ui.draw_text(
        "5",
        Vec2::new(slider_bounds.min.x(), ui.cursor().y()),
        theme.text_muted,
        tiny_font,
    );
    let max_label = "200";
    let max_size = ui.measure_text(max_label, tiny_font);
    ui.draw_text(
        max_label,
        Vec2::new(slider_bounds.max.x() - max_size.x(), ui.cursor().y()),
        theme.text_muted,
        tiny_font,
    );
    ui.spacing(20.0 + SECTION_GAP);

    ui.cursor().y()
}

fn themed_select_button(
    ui: &mut UiContext,
    id: &str,
    label: &str,
    bounds: Rect2D,
    is_selected: bool,
    theme: &ColorScheme,
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

/// Draw an inline label+input row. Returns the full-width text input bounds.
fn inline_field_row(
    ui: &mut UiContext,
    theme: &ColorScheme,
    label: &str,
    label_width: f32,
    content_width: f32,
) -> Rect2D {
    let row_y = ui.cursor().y();
    let font_size = ui.scaled_font_size(FontSize::Small);

    let label_size = ui.measure_text(label, font_size);
    ui.draw_text(
        label,
        Vec2::new(ui.cursor().x(), row_y + (ROW_HEIGHT - label_size.y()) * 0.5),
        theme.text_secondary,
        font_size,
    );

    let input_x = ui.cursor().x() + label_width + WIDGET_GAP;
    let input_width = content_width - label_width - WIDGET_GAP;
    Rect2D::from_origin_size(
        Vec2::new(input_x, row_y),
        Vec2::new(input_width, ROW_HEIGHT),
    )
}

fn build_ai_tab(
    ui: &mut UiContext,
    theme: &ColorScheme,
    cursor: Vec2,
    content_width: f32,
    llm_config: &katla_agent::LlmConfig,
    pending_actions: &mut Vec<PreferencesAction>,
) -> f32 {
    use katla_agent::config::LlmProviderKind;

    ui.set_cursor(cursor);

    // --- Provider section ---
    draw_section_header(ui, theme, "PROVIDER", content_width);

    let col_width = (content_width - 2.0 * GRID_SPACING) / 3.0;

    let providers = [
        (LlmProviderKind::Disabled, "Disabled"),
        (LlmProviderKind::OpenAi, "OpenAI"),
        (LlmProviderKind::OpenAiCompatible, "OpenAI Compatible"),
    ];

    ui.begin_grid(3, col_width, ROW_HEIGHT, GRID_SPACING);
    for (kind, label) in providers.iter() {
        let btn_bounds = ui.grid_item(Vec2::new(col_width, ROW_HEIGHT));
        let is_selected = llm_config.provider == *kind;

        if themed_select_button(
            ui,
            &format!("llm_provider_{:?}", kind),
            label,
            btn_bounds,
            is_selected,
            theme,
        ) {
            let key = match kind {
                LlmProviderKind::Disabled => "disabled",
                LlmProviderKind::OpenAi => "open_ai",
                LlmProviderKind::OpenAiCompatible => "open_ai_compatible",
            };
            pending_actions.push(PreferencesAction::SetLlmProvider(key.to_string()));
            pending_actions.push(PreferencesAction::SaveLlmConfig);
        }
    }
    ui.end_grid();

    ui.spacing(ROW_HEIGHT + WIDGET_GAP);

    // Status display
    if llm_config.provider == LlmProviderKind::Disabled {
        ui.label_auto_colored(
            "Configure an LLM provider to enable AI-powered scene building",
            theme.text_muted,
        );
        ui.spacing(LABEL_GAP);
        return ui.cursor().y();
    }

    let provider_name = match llm_config.provider {
        LlmProviderKind::OpenAi => "OpenAI",
        LlmProviderKind::OpenAiCompatible => "OpenAI Compatible",
        LlmProviderKind::Disabled => unreachable!(),
    };
    let status = format!("AI: Configured ({}, {})", provider_name, llm_config.model);
    ui.label_auto_colored(&status, theme.success);
    ui.spacing(SECTION_GAP);

    // --- Credentials section ---
    draw_section_header(ui, theme, "CREDENTIALS", content_width);

    // API Key — full width since keys are long and sensitive
    let api_key_bounds =
        Rect2D::from_origin_size(ui.cursor(), Vec2::new(content_width, ROW_HEIGHT));
    let mut api_key = llm_config.api_key.clone();
    let api_key_response = ui.add(
        katla_ui::widgets::TextInput::new("api_key", &mut api_key)
            .bounds(api_key_bounds)
            .id("llm_api_key")
            .placeholder("Enter API key..."),
    );
    if api_key_response.changed {
        pending_actions.push(PreferencesAction::SetLlmApiKey(api_key));
        pending_actions.push(PreferencesAction::SaveLlmConfig);
    }
    ui.spacing(ROW_HEIGHT + SECTION_GAP);

    // --- Model Settings section ---
    draw_section_header(ui, theme, "MODEL SETTINGS", content_width);

    let label_width = content_width * 0.3;

    // Model — inline label + input
    let model_bounds = inline_field_row(ui, theme, "Model", label_width, content_width);
    let mut model = llm_config.model.clone();
    let model_response = ui.add(
        katla_ui::widgets::TextInput::new("model", &mut model)
            .bounds(model_bounds)
            .id("llm_model")
            .placeholder("gpt-4o"),
    );
    if model_response.changed {
        pending_actions.push(PreferencesAction::SetLlmModel(model));
        pending_actions.push(PreferencesAction::SaveLlmConfig);
    }
    ui.spacing(ROW_HEIGHT + WIDGET_GAP);

    // Base URL (only for OpenAI Compatible) — inline label + input
    if llm_config.provider == LlmProviderKind::OpenAiCompatible {
        let url_bounds = inline_field_row(ui, theme, "Base URL", label_width, content_width);
        let mut base_url = llm_config.base_url.clone().unwrap_or_default();
        let url_response = ui.add(
            katla_ui::widgets::TextInput::new("base_url", &mut base_url)
                .bounds(url_bounds)
                .id("llm_base_url")
                .placeholder("http://localhost:11434/v1"),
        );
        if url_response.changed {
            pending_actions.push(PreferencesAction::SetLlmBaseUrl(base_url));
            pending_actions.push(PreferencesAction::SaveLlmConfig);
        }
        ui.spacing(ROW_HEIGHT + WIDGET_GAP);
    }

    // Temperature — inline label + slider
    let temp_label = format!("Temperature: {:.2}", llm_config.temperature);
    let temp_bounds = inline_field_row(ui, theme, &temp_label, label_width, content_width);
    let slider_bounds =
        Rect2D::from_origin_size(temp_bounds.min, Vec2::new(temp_bounds.width(), 20.0));
    let mut temperature = llm_config.temperature;
    let temp_response = ui.add(
        katla_ui::widgets::Slider::new("temperature", &mut temperature, 0.0..=2.0)
            .bounds(slider_bounds)
            .id("llm_temperature"),
    );
    if temp_response.changed {
        pending_actions.push(PreferencesAction::SetLlmTemperature(temperature));
        pending_actions.push(PreferencesAction::SaveLlmConfig);
    }

    ui.spacing(20.0 + WIDGET_GAP);

    // Max Tokens — inline label + button row
    let tokens_label = format!("Max Tokens: {}", llm_config.max_tokens);
    let _tokens_bounds = inline_field_row(ui, theme, &tokens_label, label_width, content_width);
    // Drop back to cursor to draw the button grid under the label
    let btn_row_y = ui.cursor().y();
    ui.spacing(LABEL_GAP);

    let token_sizes = [1024, 2048, 4096, 8192];
    let btn_width = (content_width - 3.0 * GRID_SPACING) / 4.0;

    ui.begin_grid(4, btn_width, ROW_HEIGHT, GRID_SPACING);
    for &tokens in token_sizes.iter() {
        let btn_bounds = ui.grid_item(Vec2::new(btn_width, ROW_HEIGHT));
        let is_selected = llm_config.max_tokens == tokens;
        let text = format!("{}", tokens);
        if themed_select_button(
            ui,
            &format!("max_tokens_{}", tokens),
            &text,
            btn_bounds,
            is_selected,
            theme,
        ) {
            pending_actions.push(PreferencesAction::SetLlmMaxTokens(tokens));
            pending_actions.push(PreferencesAction::SaveLlmConfig);
        }
    }
    ui.end_grid();

    // Fix: the _tokens_bounds inline row pushed cursor past the label, but we already
    // drew the grid below it. The grid items use their own cursor, so this is fine.
    // Just undo the extra label spacing for the grid since we already drew it.
    let _ = btn_row_y;

    ui.spacing(ROW_HEIGHT + SECTION_GAP);

    ui.cursor().y()
}
