use katla_ui::FontSize;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, DraggablePanelState, DraggablePanelVisibility, StateId,
    ViewDescriptor, draggable_panel, grid, labeled_slider, selectable, tab_bar, tab_item, text,
    textfield, toggle, vstack,
};

use crate::Preferences;

use super::super::ColorScheme;
use super::super::types::{EditorSettings, PreferencesAction, PreferencesTab};

#[derive(Clone)]
pub(crate) struct PreferencesDrawCtx {
    pub is_open: bool,
    pub preferences: Preferences,
    pub editor_settings: EditorSettings,
    pub theme: ColorScheme,
    pub theme_key: String,
    pub llm_config: katla_agent::LlmConfig,
}

#[derive(Clone, Debug)]
pub(crate) struct PreferencesPanelSync {
    pub visibility: DraggablePanelVisibility,
}

pub(crate) struct PreferencesView;

impl Build for PreferencesView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let draw_ctx = ctx.env::<PreferencesDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return ViewDescriptor::Empty;
        };

        let panel_id: StateId = ctx.state(DraggablePanelState::default());
        let mut panel_state: DraggablePanelState = ctx.get_state(panel_id);

        if draw_ctx.is_open && !panel_state.visibility.is_visible() {
            panel_state.visibility = DraggablePanelVisibility::JustOpened;
            ctx.set_state(panel_id, panel_state);
        } else if !draw_ctx.is_open && panel_state.visibility.is_visible() {
            panel_state.visibility = DraggablePanelVisibility::Hidden;
            ctx.set_state(panel_id, panel_state);
        }

        let current_panel: DraggablePanelState = ctx.get_state(panel_id);
        ctx.emit(PreferencesPanelSync {
            visibility: current_panel.visibility,
        });

        if !current_panel.visibility.is_visible() {
            return ViewDescriptor::Empty;
        }

        let theme = &draw_ctx.theme;
        let tab_sel_id: StateId = ctx.state(0usize);
        let current_tab: usize = ctx.get_state(tab_sel_id);
        let active_tab = match current_tab {
            0 => PreferencesTab::General,
            1 => PreferencesTab::Viewport,
            _ => PreferencesTab::Ai,
        };

        let content = match active_tab {
            PreferencesTab::General => build_general_tab(ctx, theme, &draw_ctx),
            PreferencesTab::Viewport => build_viewport_tab(ctx, theme, &draw_ctx),
            PreferencesTab::Ai => build_ai_tab(ctx, theme, &draw_ctx),
        };

        let _scroll_id: StateId = ctx.state(0.0f32);

        draggable_panel(
            "Preferences",
            450.0,
            500.0,
            vstack([tab_bar(
                vec![tab_item("General"), tab_item("Viewport"), tab_item("AI")],
                tab_sel_id,
                content,
            )])
            .spacing(4.0)
            .padding_all(8.0)
            .align(Alignment::Leading),
            panel_id,
        )
        .close_on_outside(false)
    }
}

fn build_general_tab(
    ctx: &mut BuildContext,
    theme: &ColorScheme,
    draw_ctx: &PreferencesDrawCtx,
) -> ViewDescriptor {
    let mut children: Vec<ViewDescriptor> = Vec::new();

    // Color theme grid
    children.push(
        text("COLOR THEME")
            .color(theme.text_secondary)
            .font_size(FontSize::Small),
    );

    let theme_names = [
        ("default", "Default"),
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

    let theme_buttons: Vec<ViewDescriptor> = theme_names
        .iter()
        .map(|(key, display_name)| {
            let is_selected = *key == draw_ctx.theme_key;
            let key_owned = key.to_string();
            selectable(text(*display_name))
                .selected(is_selected)
                .on_click(ctx.on_click(move |actions| {
                    actions.emit(PreferencesAction::SetTheme(key_owned.clone()));
                }))
        })
        .collect();

    children.push(grid(3, katla_math::Vec2::new(130.0, 28.0), theme_buttons).grid_spacing(8.0));

    // Font scale
    children.push(
        text("FONT SCALE")
            .color(theme.text_secondary)
            .font_size(FontSize::Small),
    );

    let scale_id: StateId = ctx.state(draw_ctx.preferences.font_scale);
    let current_scale: f32 = ctx.get_state(scale_id);
    if (current_scale - draw_ctx.preferences.font_scale).abs() > 1e-4 {
        ctx.emit(PreferencesAction::SetFontScale(current_scale));
    }

    children.push(
        labeled_slider("Scale:", scale_id, 0.75..=2.0)
            .label_width(60.0)
            .show_value(true)
            .precision(0),
    );

    vstack(children).spacing(8.0)
}

fn build_viewport_tab(
    ctx: &mut BuildContext,
    theme: &ColorScheme,
    draw_ctx: &PreferencesDrawCtx,
) -> ViewDescriptor {
    let mut children: Vec<ViewDescriptor> = Vec::new();

    // Display section
    children.push(
        text("DISPLAY")
            .color(theme.text_secondary)
            .font_size(FontSize::Small),
    );

    let grid_toggle_id: StateId = ctx.state(draw_ctx.preferences.show_grid);
    let current_show_grid: bool = ctx.get_state(grid_toggle_id);
    if current_show_grid != draw_ctx.preferences.show_grid {
        ctx.emit(PreferencesAction::ToggleGrid);
    }
    children.push(toggle("Show Grid", grid_toggle_id));

    let stats_toggle_id: StateId = ctx.state(draw_ctx.preferences.show_stats);
    let current_show_stats: bool = ctx.get_state(stats_toggle_id);
    if current_show_stats != draw_ctx.preferences.show_stats {
        ctx.emit(PreferencesAction::ToggleStats);
    }
    children.push(toggle("Show Stats Panel", stats_toggle_id));

    // Grid section
    children.push(
        text("GRID")
            .color(theme.text_secondary)
            .font_size(FontSize::Small),
    );

    let sizes = [0.5, 1.0, 2.0, 5.0, 10.0];
    let grid_buttons: Vec<ViewDescriptor> = sizes
        .iter()
        .map(|&size| {
            let is_selected = (draw_ctx.editor_settings.grid_size - size).abs() < 0.01;
            selectable(text(format!("{:.1}", size)))
                .selected(is_selected)
                .on_click(ctx.on_click(move |actions| {
                    actions.emit(PreferencesAction::SetGridSize(size));
                }))
        })
        .collect();
    children.push(grid(5, katla_math::Vec2::new(70.0, 28.0), grid_buttons).grid_spacing(8.0));

    let snap_id: StateId = ctx.state(draw_ctx.editor_settings.snap_to_grid);
    let current_snap: bool = ctx.get_state(snap_id);
    if current_snap != draw_ctx.editor_settings.snap_to_grid {
        ctx.emit(PreferencesAction::SetSnapToGrid(current_snap));
    }
    children.push(toggle("Snap to Grid", snap_id));

    // Camera section
    children.push(
        text("CAMERA")
            .color(theme.text_secondary)
            .font_size(FontSize::Small),
    );

    let speed_id: StateId = ctx.state(draw_ctx.editor_settings.camera_speed);
    let current_speed: f32 = ctx.get_state(speed_id);
    if (current_speed - draw_ctx.editor_settings.camera_speed).abs() > 1e-4 {
        ctx.emit(PreferencesAction::SetCameraSpeed(current_speed));
    }
    children.push(
        labeled_slider("Speed:", speed_id, 5.0..=200.0)
            .label_width(60.0)
            .show_value(true)
            .precision(0),
    );

    vstack(children).spacing(8.0)
}

fn build_ai_tab(
    ctx: &mut BuildContext,
    theme: &ColorScheme,
    draw_ctx: &PreferencesDrawCtx,
) -> ViewDescriptor {
    use katla_agent::config::LlmProviderKind;

    let llm_config = &draw_ctx.llm_config;
    let mut children: Vec<ViewDescriptor> = Vec::new();

    // Provider section
    children.push(
        text("PROVIDER")
            .color(theme.text_secondary)
            .font_size(FontSize::Small),
    );

    let providers = [
        (LlmProviderKind::Disabled, "Disabled", "disabled"),
        (LlmProviderKind::OpenAi, "OpenAI", "open_ai"),
        (
            LlmProviderKind::OpenAiCompatible,
            "OpenAI Compatible",
            "open_ai_compatible",
        ),
    ];

    let provider_buttons: Vec<ViewDescriptor> = providers
        .iter()
        .map(|(kind, label, key)| {
            let is_selected = llm_config.provider == *kind;
            let key_owned = key.to_string();
            selectable(text(*label))
                .selected(is_selected)
                .on_click(ctx.on_click(move |actions| {
                    actions.emit(PreferencesAction::SetLlmProvider(key_owned.clone()));
                    actions.emit(PreferencesAction::SaveLlmConfig);
                }))
        })
        .collect();
    children.push(grid(3, katla_math::Vec2::new(130.0, 28.0), provider_buttons).grid_spacing(8.0));

    if llm_config.provider == LlmProviderKind::Disabled {
        children.push(
            text("Configure an LLM provider to enable AI-powered scene building")
                .color(theme.text_muted)
                .font_size(FontSize::Small),
        );
        return vstack(children).spacing(8.0);
    }

    let provider_name = match llm_config.provider {
        LlmProviderKind::OpenAi => "OpenAI",
        LlmProviderKind::OpenAiCompatible => "OpenAI Compatible",
        LlmProviderKind::Disabled => unreachable!(),
    };
    children.push(
        text(format!(
            "AI: Configured ({}, {})",
            provider_name, llm_config.model
        ))
        .color(theme.success)
        .font_size(FontSize::Small),
    );

    // Credentials section
    children.push(
        text("CREDENTIALS")
            .color(theme.text_secondary)
            .font_size(FontSize::Small),
    );

    let api_key_id: StateId = ctx.state(llm_config.api_key.clone());
    let current_api_key: String = ctx.get_state(api_key_id);
    if current_api_key != llm_config.api_key {
        ctx.emit(PreferencesAction::SetLlmApiKey(current_api_key));
        ctx.emit(PreferencesAction::SaveLlmConfig);
    }
    children.push(textfield("Enter API key...", api_key_id));

    // Model settings section
    children.push(
        text("MODEL SETTINGS")
            .color(theme.text_secondary)
            .font_size(FontSize::Small),
    );

    let model_id: StateId = ctx.state(llm_config.model.clone());
    let current_model: String = ctx.get_state(model_id);
    if current_model != llm_config.model {
        ctx.emit(PreferencesAction::SetLlmModel(current_model));
        ctx.emit(PreferencesAction::SaveLlmConfig);
    }
    children.push(textfield("gpt-4o", model_id));

    if llm_config.provider == LlmProviderKind::OpenAiCompatible {
        let base_url_id: StateId = ctx.state(llm_config.base_url.clone().unwrap_or_default());
        let current_base_url: String = ctx.get_state(base_url_id);
        let expected = llm_config.base_url.clone().unwrap_or_default();
        if current_base_url != expected {
            ctx.emit(PreferencesAction::SetLlmBaseUrl(current_base_url));
            ctx.emit(PreferencesAction::SaveLlmConfig);
        }
        children.push(textfield("http://localhost:11434/v1", base_url_id));
    }

    // Temperature
    let temp_id: StateId = ctx.state(llm_config.temperature);
    let current_temp: f32 = ctx.get_state(temp_id);
    if (current_temp - llm_config.temperature).abs() > 1e-4 {
        ctx.emit(PreferencesAction::SetLlmTemperature(current_temp));
        ctx.emit(PreferencesAction::SaveLlmConfig);
    }
    children.push(
        labeled_slider("Temperature:", temp_id, 0.0..=2.0)
            .label_width(100.0)
            .show_value(true)
            .precision(2),
    );

    // Max tokens
    let token_sizes = [1024, 2048, 4096, 8192];
    let token_buttons: Vec<ViewDescriptor> = token_sizes
        .iter()
        .map(|&tokens| {
            let is_selected = llm_config.max_tokens == tokens;
            selectable(text(format!("{}", tokens)))
                .selected(is_selected)
                .on_click(ctx.on_click(move |actions| {
                    actions.emit(PreferencesAction::SetLlmMaxTokens(tokens));
                    actions.emit(PreferencesAction::SaveLlmConfig);
                }))
        })
        .collect();
    children.push(grid(4, katla_math::Vec2::new(90.0, 28.0), token_buttons).grid_spacing(8.0));

    vstack(children).spacing(8.0)
}
