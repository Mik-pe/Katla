use katla_icons::ForkAwesome;
use katla_ui::FontSize;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, DraggablePanelState, DraggablePanelVisibility, StateId, Widget,
    WidgetBox, draggable_panel, grid, hstack, icon, labeled_slider, selectable, tab_bar, tab_item,
    text, textfield, toggle, vstack,
};

use crate::Preferences;

use super::super::ColorScheme;
use super::super::types::{EditorSettings, PreferencesAction, PreferencesTab};

/// Sync a slider state: allocate state with `initial` value, read current,
/// and emit `action` if changed beyond epsilon. Returns the StateId.
fn sync_slider<F>(ctx: &mut BuildContext, initial: f32, action: F) -> StateId
where
    F: Fn(f32) -> PreferencesAction,
{
    let id: StateId = ctx.state(initial);
    let current: f32 = ctx.get_state(id).unwrap();
    if (current - initial).abs() > 1e-4 {
        ctx.emit(action(current));
    }
    id
}

/// Sync a toggle state: allocate state with `initial` value, read current,
/// and emit `action` if changed. Returns the StateId.
fn sync_toggle<F>(ctx: &mut BuildContext, initial: bool, action: F) -> StateId
where
    F: Fn(bool) -> PreferencesAction,
{
    let id: StateId = ctx.state(initial);
    let current: bool = ctx.get_state(id).unwrap();
    if current != initial {
        ctx.emit(action(current));
    }
    id
}

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
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        use katla_ui::declarative::{WidgetBox, empty};

        let draw_ctx = ctx.env::<PreferencesDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return empty().boxed();
        };

        let panel_id: StateId = ctx.state(DraggablePanelState::default());
        let mut panel_state: DraggablePanelState = ctx.get_state(panel_id).unwrap();

        if draw_ctx.is_open && !panel_state.visibility.is_visible() {
            panel_state.visibility = DraggablePanelVisibility::JustOpened;
            ctx.set_state(panel_id, panel_state);
        } else if !draw_ctx.is_open && panel_state.visibility.is_visible() {
            panel_state.visibility = DraggablePanelVisibility::Hidden;
            ctx.set_state(panel_id, panel_state);
        }

        let current_panel: DraggablePanelState = ctx.get_state(panel_id).unwrap();
        ctx.emit(PreferencesPanelSync {
            visibility: current_panel.visibility,
        });

        if !current_panel.visibility.is_visible() {
            return empty().boxed();
        }

        let theme = &draw_ctx.theme;
        let tab_sel_id: StateId = ctx.state(0usize);
        let current_tab: usize = ctx.get_state(tab_sel_id).unwrap();
        let active_tab = match current_tab {
            0 => PreferencesTab::General,
            1 => PreferencesTab::Viewport,
            2 => PreferencesTab::Audio,
            _ => PreferencesTab::Ai,
        };

        let content = match active_tab {
            PreferencesTab::General => build_general_tab(ctx, theme, &draw_ctx),
            PreferencesTab::Viewport => build_viewport_tab(ctx, theme, &draw_ctx),
            PreferencesTab::Audio => build_audio_tab(ctx, theme, &draw_ctx),
            PreferencesTab::Ai => build_ai_tab(ctx, theme, &draw_ctx),
        };

        draggable_panel(
            "Preferences",
            450.0,
            500.0,
            vstack([tab_bar(
                vec![
                    tab_item("General"),
                    tab_item("Viewport"),
                    tab_item("Audio"),
                    tab_item("AI"),
                ],
                tab_sel_id,
                content,
            )
            .boxed()])
            .spacing(4.0)
            .padding_all(8.0)
            .align(Alignment::Leading)
            .boxed(),
            panel_id,
        )
        .close_on_outside(false)
        .boxed()
    }
}

fn build_general_tab(
    ctx: &mut BuildContext,
    theme: &ColorScheme,
    draw_ctx: &PreferencesDrawCtx,
) -> Box<dyn Widget> {
    let mut children: Vec<Box<dyn Widget>> = Vec::new();

    // Color theme grid
    children.push(
        text("COLOR THEME")
            .color(theme.text_secondary)
            .font_size(FontSize::Small)
            .boxed(),
    );

    let theme_names = [
        ("rcp", "Reality Composer Pro"),
        ("dark", "Dark"),
        ("light", "Light"),
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

    let theme_buttons: Vec<Box<dyn Widget>> = theme_names
        .iter()
        .map(|(key, display_name)| {
            let is_selected = *key == draw_ctx.theme_key;
            let key_owned = key.to_string();
            let label: Box<dyn Widget> = if is_selected {
                hstack([
                    icon(ForkAwesome::CHECK).boxed(),
                    text(*display_name).boxed(),
                ])
                .spacing(4.0)
                .boxed()
            } else {
                text(*display_name).boxed()
            };
            selectable(label)
                .selected(is_selected)
                .on_click(ctx.on_click(move |actions| {
                    actions.emit(PreferencesAction::SetTheme(key_owned.clone()));
                }))
                .boxed()
        })
        .collect();

    children.push(
        grid(3, katla_math::Vec2::new(130.0, 40.0), theme_buttons)
            .grid_spacing(8.0)
            .boxed(),
    );

    // Font scale
    children.push(
        text("FONT SCALE")
            .color(theme.text_secondary)
            .font_size(FontSize::Small)
            .boxed(),
    );

    let scale_id = sync_slider(
        ctx,
        draw_ctx.preferences.font_scale,
        PreferencesAction::SetFontScale,
    );

    children.push(
        labeled_slider("Scale:", scale_id, 0.75..=2.0)
            .label_width(60.0)
            .show_value(true)
            .precision(0)
            .boxed(),
    );

    vstack(children).spacing(8.0).boxed()
}

fn build_viewport_tab(
    ctx: &mut BuildContext,
    theme: &ColorScheme,
    draw_ctx: &PreferencesDrawCtx,
) -> Box<dyn Widget> {
    let mut children: Vec<Box<dyn Widget>> = Vec::new();

    // Display section
    children.push(
        text("DISPLAY")
            .color(theme.text_secondary)
            .font_size(FontSize::Small)
            .boxed(),
    );

    let grid_toggle_id = sync_toggle(ctx, draw_ctx.preferences.show_grid, |_| {
        PreferencesAction::ToggleGrid
    });
    children.push(toggle("Show Grid", grid_toggle_id).boxed());

    let stats_toggle_id = sync_toggle(ctx, draw_ctx.preferences.show_stats, |_| {
        PreferencesAction::ToggleStats
    });
    children.push(toggle("Show Stats Panel", stats_toggle_id).boxed());

    // Grid section
    children.push(
        text("GRID")
            .color(theme.text_secondary)
            .font_size(FontSize::Small)
            .boxed(),
    );

    let sizes = [0.5, 1.0, 2.0, 5.0, 10.0];
    let grid_buttons: Vec<Box<dyn Widget>> = sizes
        .iter()
        .map(|&size| {
            let is_selected = (draw_ctx.editor_settings.grid_size - size).abs() < 0.01;
            selectable(text(format!("{:.1}", size)).boxed())
                .selected(is_selected)
                .on_click(ctx.on_click(move |actions| {
                    actions.emit(PreferencesAction::SetGridSize(size));
                }))
                .boxed()
        })
        .collect();
    children.push(
        grid(5, katla_math::Vec2::new(70.0, 40.0), grid_buttons)
            .grid_spacing(8.0)
            .boxed(),
    );

    let snap_id = sync_toggle(
        ctx,
        draw_ctx.editor_settings.snap_to_grid,
        PreferencesAction::SetSnapToGrid,
    );
    children.push(toggle("Snap to Grid", snap_id).boxed());

    // Camera section
    children.push(
        text("CAMERA")
            .color(theme.text_secondary)
            .font_size(FontSize::Small)
            .boxed(),
    );

    let speed_id = sync_slider(
        ctx,
        draw_ctx.editor_settings.camera_speed,
        PreferencesAction::SetCameraSpeed,
    );
    children.push(
        labeled_slider("Speed:", speed_id, 5.0..=200.0)
            .label_width(60.0)
            .show_value(true)
            .precision(0)
            .boxed(),
    );

    vstack(children).spacing(8.0).boxed()
}

fn build_audio_tab(
    ctx: &mut BuildContext,
    theme: &ColorScheme,
    draw_ctx: &PreferencesDrawCtx,
) -> Box<dyn Widget> {
    let mut children: Vec<Box<dyn Widget>> = Vec::new();

    children.push(
        text("VOLUME")
            .color(theme.text_secondary)
            .font_size(FontSize::Small)
            .boxed(),
    );

    let master_id = sync_slider(
        ctx,
        draw_ctx.preferences.audio.master_volume,
        PreferencesAction::SetMasterVolume,
    );
    children.push(
        labeled_slider("Master:", master_id, 0.0..=1.0)
            .label_width(70.0)
            .show_value(true)
            .precision(0)
            .boxed(),
    );

    let sfx_id = sync_slider(
        ctx,
        draw_ctx.preferences.audio.sfx_volume,
        PreferencesAction::SetSfxVolume,
    );
    children.push(
        labeled_slider("SFX:", sfx_id, 0.0..=1.0)
            .label_width(70.0)
            .show_value(true)
            .precision(0)
            .boxed(),
    );

    let music_id = sync_slider(
        ctx,
        draw_ctx.preferences.audio.music_volume,
        PreferencesAction::SetMusicVolume,
    );
    children.push(
        labeled_slider("Music:", music_id, 0.0..=1.0)
            .label_width(70.0)
            .show_value(true)
            .precision(0)
            .boxed(),
    );

    let ambient_id = sync_slider(
        ctx,
        draw_ctx.preferences.audio.ambient_volume,
        PreferencesAction::SetAmbientVolume,
    );
    children.push(
        labeled_slider("Ambient:", ambient_id, 0.0..=1.0)
            .label_width(70.0)
            .show_value(true)
            .precision(0)
            .boxed(),
    );

    vstack(children).spacing(8.0).boxed()
}

fn build_ai_tab(
    ctx: &mut BuildContext,
    theme: &ColorScheme,
    draw_ctx: &PreferencesDrawCtx,
) -> Box<dyn Widget> {
    use katla_agent::config::LlmProviderKind;

    let llm_config = &draw_ctx.llm_config;
    let mut children: Vec<Box<dyn Widget>> = Vec::new();

    // Provider section
    children.push(
        text("PROVIDER")
            .color(theme.text_secondary)
            .font_size(FontSize::Small)
            .boxed(),
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

    let provider_buttons: Vec<Box<dyn Widget>> = providers
        .iter()
        .map(|(kind, label, key)| {
            let is_selected = llm_config.provider == *kind;
            let key_owned = key.to_string();
            selectable(text(*label).boxed())
                .selected(is_selected)
                .on_click(ctx.on_click(move |actions| {
                    actions.emit(PreferencesAction::SetLlmProvider(key_owned.clone()));
                    actions.emit(PreferencesAction::SaveLlmConfig);
                }))
                .boxed()
        })
        .collect();
    children.push(
        grid(3, katla_math::Vec2::new(130.0, 40.0), provider_buttons)
            .grid_spacing(8.0)
            .boxed(),
    );

    if llm_config.provider == LlmProviderKind::Disabled {
        children.push(
            text("Configure an LLM provider to enable AI-powered scene building")
                .color(theme.text_muted)
                .font_size(FontSize::Small)
                .boxed(),
        );
        return vstack(children).spacing(8.0).boxed();
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
        .font_size(FontSize::Small)
        .boxed(),
    );

    // Credentials section
    children.push(
        text("CREDENTIALS")
            .color(theme.text_secondary)
            .font_size(FontSize::Small)
            .boxed(),
    );

    let api_key_id: StateId = ctx.state(llm_config.api_key.clone());
    let current_api_key: String = ctx.get_state(api_key_id).unwrap();
    if current_api_key != llm_config.api_key {
        ctx.emit(PreferencesAction::SetLlmApiKey(current_api_key));
        ctx.emit(PreferencesAction::SaveLlmConfig);
    }
    children.push(textfield("Enter API key...", api_key_id).boxed());

    // Model settings section
    children.push(
        text("MODEL SETTINGS")
            .color(theme.text_secondary)
            .font_size(FontSize::Small)
            .boxed(),
    );

    let model_id: StateId = ctx.state(llm_config.model.clone());
    let current_model: String = ctx.get_state(model_id).unwrap();
    if current_model != llm_config.model {
        ctx.emit(PreferencesAction::SetLlmModel(current_model));
        ctx.emit(PreferencesAction::SaveLlmConfig);
    }
    children.push(textfield("gpt-4o", model_id).boxed());

    if llm_config.provider == LlmProviderKind::OpenAiCompatible {
        let base_url_id: StateId = ctx.state(llm_config.base_url.clone().unwrap_or_default());
        let current_base_url: String = ctx.get_state(base_url_id).unwrap();
        let expected = llm_config.base_url.clone().unwrap_or_default();
        if current_base_url != expected {
            ctx.emit(PreferencesAction::SetLlmBaseUrl(current_base_url));
            ctx.emit(PreferencesAction::SaveLlmConfig);
        }
        children.push(textfield("http://localhost:11434/v1", base_url_id).boxed());
    }

    // Temperature
    let temp_id = sync_slider(
        ctx,
        llm_config.temperature,
        PreferencesAction::SetLlmTemperature,
    );
    children.push(
        labeled_slider("Temperature:", temp_id, 0.0..=2.0)
            .label_width(100.0)
            .show_value(true)
            .precision(2)
            .boxed(),
    );

    // Max tokens
    let token_sizes = [1024, 2048, 4096, 8192];
    let token_buttons: Vec<Box<dyn Widget>> = token_sizes
        .iter()
        .map(|&tokens| {
            let is_selected = llm_config.max_tokens == tokens;
            selectable(text(format!("{}", tokens)).boxed())
                .selected(is_selected)
                .on_click(ctx.on_click(move |actions| {
                    actions.emit(PreferencesAction::SetLlmMaxTokens(tokens));
                    actions.emit(PreferencesAction::SaveLlmConfig);
                }))
                .boxed()
        })
        .collect();
    children.push(
        grid(4, katla_math::Vec2::new(90.0, 40.0), token_buttons)
            .grid_spacing(8.0)
            .boxed(),
    );

    vstack(children).spacing(8.0).boxed()
}
