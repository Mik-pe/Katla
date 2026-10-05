use std::boxed::Box;

use katla_ui::FontSize;
use katla_ui::ForkAwesome;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, Padding, StateId, Widget, WidgetBox, button, empty, grid,
    hstack, icon, labeled_slider, modal, scroll, selectable, separator_horizontal, text, textfield,
    theme_swatch, toggle, vstack,
};

use crate::Preferences;

use super::super::ColorScheme;
use super::super::types::{EditorSettings, PreferencesAction, PreferencesTab};

/// Fixed modal size: wide enough for two-column theme cells, tall enough to
/// avoid internal scrolling for every category except long lists.
pub(crate) const PREFERENCES_WIDTH: f32 = 560.0;
pub(crate) const PREFERENCES_HEIGHT: f32 = 520.0;

/// Sidebar width inside the modal.
const SIDEBAR_WIDTH: f32 = 148.0;
/// Label column width for setting rows (label left, control right).
const LABEL_WIDTH: f32 = 140.0;

/// Human-exposed UI scale steps (fractions of the default font size).
const UI_SCALE_STEPS: [f32; 6] = [0.8, 0.9, 1.0, 1.1, 1.2, 1.3];

/// Theme entries in picker order: current default first, then by hue family.
const THEME_NAMES: [(&str, &str); 15] = [
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

#[derive(Clone)]
pub(crate) struct PreferencesDrawCtx {
    pub is_open: bool,
    pub category: usize,
    pub preferences: Preferences,
    pub editor_settings: EditorSettings,
    pub theme: ColorScheme,
    pub theme_key: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PreferencesPanelSync {
    pub open: bool,
}

pub(crate) struct PreferencesView;

impl Build for PreferencesView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let draw_ctx = ctx.env::<PreferencesDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return empty().boxed();
        };

        // ── State reservation ──
        // ALL state slots are allocated unconditionally in this fixed order.
        // Category content reads from them conditionally; conditional
        // allocation would shift slots between frames and cross-assign types.
        let open_id: StateId = ctx.state(draw_ctx.is_open);
        let camera_speed_id = sync_slider(
            ctx,
            draw_ctx.editor_settings.camera_speed,
            PreferencesAction::SetCameraSpeed,
        );
        let master_id = sync_slider(
            ctx,
            draw_ctx.preferences.audio.master_volume,
            PreferencesAction::SetMasterVolume,
        );
        let sfx_id = sync_slider(
            ctx,
            draw_ctx.preferences.audio.sfx_volume,
            PreferencesAction::SetSfxVolume,
        );
        let music_id = sync_slider(
            ctx,
            draw_ctx.preferences.audio.music_volume,
            PreferencesAction::SetMusicVolume,
        );
        let ambient_id = sync_slider(
            ctx,
            draw_ctx.preferences.audio.ambient_volume,
            PreferencesAction::SetAmbientVolume,
        );
        sync_toggle(ctx, draw_ctx.preferences.show_grid, |_| {
            PreferencesAction::ToggleGrid
        });
        sync_toggle(ctx, draw_ctx.preferences.show_stats, |_| {
            PreferencesAction::ToggleStats
        });
        sync_toggle(
            ctx,
            draw_ctx.editor_settings.snap_to_grid,
            PreferencesAction::SetSnapToGrid,
        );

        let socket_id = ctx.state(draw_ctx.preferences.external_chat.socket.clone());
        let thread_id = ctx.state(draw_ctx.preferences.external_chat.thread_id.clone());

        // ── Open-state reconciliation ──
        // env drives open; Escape/outside-click flip the state during input
        // and the sync action closes the app-side panel before next frame.
        let mut open: bool = ctx.get_state(open_id).unwrap_or(draw_ctx.is_open);
        if open != draw_ctx.is_open {
            open = draw_ctx.is_open;
            ctx.set_state(open_id, open);
        }
        ctx.emit(PreferencesPanelSync { open });

        if !open {
            return empty().boxed();
        }

        let content_scroll_id: StateId = ctx.state(0.0f32);

        let category = category_from_index(ctx, draw_ctx.category);
        let content = match category {
            PreferencesTab::Appearance => build_appearance(ctx, &draw_ctx),
            PreferencesTab::Viewport => build_viewport(ctx, &draw_ctx, camera_speed_id),
            PreferencesTab::Audio => {
                build_audio(ctx, &draw_ctx, master_id, sfx_id, music_id, ambient_id)
            }
            PreferencesTab::Connection => build_connection(ctx, &draw_ctx, socket_id, thread_id),
        };

        let sidebar = build_sidebar(ctx, &draw_ctx.theme, category);

        let body_height = PREFERENCES_HEIGHT - katla_ui::tokens::MODAL_TITLE_HEIGHT;
        let body = hstack([
            sidebar,
            hstack([katla_ui::declarative::separator_vertical().boxed()])
                .flex_height(body_height)
                .boxed(),
            scroll(content_padding(content), content_scroll_id)
                .flex_grow(1.0)
                .flex_height(body_height)
                .boxed(),
        ])
        .spacing(0.0)
        .flex_height(body_height)
        .flex_width(PREFERENCES_WIDTH)
        .boxed();

        modal(PREFERENCES_WIDTH, PREFERENCES_HEIGHT, open_id, body)
            .title("Preferences")
            .on_close(ctx.on_click(|actions| {
                actions.emit(PreferencesPanelSync { open: false });
            }))
            .boxed()
    }
}

fn category_from_index(_ctx: &mut BuildContext, index: usize) -> PreferencesTab {
    match index {
        0 => PreferencesTab::Appearance,
        1 => PreferencesTab::Viewport,
        2 => PreferencesTab::Audio,
        _ => PreferencesTab::Connection,
    }
}

/// Sync a slider state: allocate state with `initial` value, read current,
/// and emit `action` if changed beyond epsilon. Returns the StateId.
fn sync_slider<F>(ctx: &mut BuildContext, initial: f32, action: F) -> StateId
where
    F: Fn(f32) -> PreferencesAction,
{
    let id: StateId = ctx.state(initial);
    let current: f32 = ctx.get_state(id).unwrap_or(initial);
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
    let current: bool = ctx.get_state(id).unwrap_or(initial);
    if current != initial {
        ctx.emit(action(current));
    }
    id
}

// ---------------------------------------------------------------------------
// Shared building blocks
// ---------------------------------------------------------------------------

/// Section title: the strongest text inside the content area.
fn section_title(label: &str, theme: &ColorScheme) -> Box<dyn Widget> {
    text(label)
        .color(theme.text_primary)
        .font_size(FontSize::Medium)
        .boxed()
}

/// One setting row: label in a fixed left column, control fills the rest.
fn setting_row(label: &str, theme: &ColorScheme, control: Box<dyn Widget>) -> Box<dyn Widget> {
    hstack([
        hstack([text(label)
            .color(theme.text_secondary)
            .font_size(FontSize::Medium)
            .boxed()])
        .flex_width(LABEL_WIDTH)
        .boxed(),
        control,
    ])
    .spacing(8.0)
    .align(Alignment::Middle)
    .flex_grow(1.0)
    .boxed()
}

/// A segmented row of mutually exclusive options (grid sizes, scale steps…).
fn segmented(
    ctx: &mut BuildContext,
    theme: &ColorScheme,
    options: &[(String, f32)],
    current: f32,
    on_select: fn(f32) -> PreferencesAction,
) -> Box<dyn Widget> {
    let buttons: Vec<Box<dyn Widget>> = options
        .iter()
        .map(|(label, value)| {
            let value = *value;
            let is_selected = (current - value).abs() < 0.05;
            let cell = hstack([text(label.clone())
                .color(if is_selected {
                    theme.text_primary
                } else {
                    theme.text_secondary
                })
                .font_size(FontSize::Small)
                .boxed()])
            .padding(Padding::horizontal(10.0))
            .flex_height(katla_ui::tokens::CONTROL_HEIGHT)
            .flex_shrink(0.0)
            .align(Alignment::Center)
            .boxed();
            selectable(cell)
                .selected(is_selected)
                .on_click(ctx.on_click(move |actions| {
                    actions.emit(on_select(value));
                }))
                .boxed()
        })
        .collect();
    hstack(buttons).spacing(8.0).boxed()
}

/// Content wrapper: consistent padding and vertical rhythm for every category.
fn content_padding(child: Box<dyn Widget>) -> Box<dyn Widget> {
    vstack([child])
        .spacing(0.0)
        .padding(Padding {
            top: 16.0,
            right: 16.0,
            bottom: 16.0,
            left: 16.0,
        })
        .align(Alignment::Leading)
        .boxed()
}

/// Hairline between sections — spacing does the grouping, the line only
/// marks a hard change of subject.
fn section_divider(_theme: &ColorScheme) -> Box<dyn Widget> {
    hstack([separator_horizontal().boxed()])
        .flex_grow(1.0)
        .flex_height(1.0)
        .boxed()
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

fn build_sidebar(
    ctx: &mut BuildContext,
    theme: &ColorScheme,
    category: PreferencesTab,
) -> Box<dyn Widget> {
    let entries: [(PreferencesTab, char, &str); 4] = [
        (
            PreferencesTab::Appearance,
            ForkAwesome::PAINT_BRUSH,
            "Appearance",
        ),
        (
            PreferencesTab::Viewport,
            ForkAwesome::VIDEO_CAMERA,
            "Viewport",
        ),
        (PreferencesTab::Audio, ForkAwesome::VOLUME_UP, "Audio"),
        (PreferencesTab::Connection, ForkAwesome::MAGIC, "Connection"),
    ];

    let mut rows: Vec<Box<dyn Widget>> = Vec::new();
    for (tab, glyph, label) in entries {
        let is_selected = tab == category;
        let index = match tab {
            PreferencesTab::Appearance => 0,
            PreferencesTab::Viewport => 1,
            PreferencesTab::Audio => 2,
            PreferencesTab::Connection => 3,
        };
        let row = hstack([
            icon(glyph)
                .icon_size(FontSize::Small)
                .color(if is_selected {
                    theme.accent
                } else {
                    theme.text_secondary
                })
                .boxed(),
            text(label)
                .color(if is_selected {
                    theme.text_primary
                } else {
                    theme.text_secondary
                })
                .font_size(FontSize::Medium)
                .boxed(),
        ])
        .spacing(8.0)
        .padding(Padding::horizontal(8.0))
        .align(Alignment::Middle)
        .flex_height(katla_ui::tokens::CONTROL_HEIGHT)
        .flex_grow(1.0);

        rows.push(
            selectable(row.boxed())
                .selected(is_selected)
                .on_click(ctx.on_click(move |actions| {
                    actions.emit(PreferencesAction::SetCategory(index));
                }))
                .boxed(),
        );
    }

    vstack(rows)
        .spacing(2.0)
        .padding(Padding {
            top: 12.0,
            right: 8.0,
            bottom: 12.0,
            left: 8.0,
        })
        .flex_width(SIDEBAR_WIDTH)
        .align(Alignment::Leading)
        .boxed()
}

// ---------------------------------------------------------------------------
// Appearance
// ---------------------------------------------------------------------------

fn build_appearance(ctx: &mut BuildContext, draw_ctx: &PreferencesDrawCtx) -> Box<dyn Widget> {
    let theme = &draw_ctx.theme;
    let mut children: Vec<Box<dyn Widget>> = Vec::new();

    // ── Interface theme ──
    children.push(section_title("Interface theme", theme));

    // Compact two-column list: one restrained swatch + name per theme.
    // The whole cell is the click target; the current theme carries the
    // accent row and a check mark so it reads in under a second.
    let mut cells: Vec<Box<dyn Widget>> = Vec::new();
    for (key, display_name) in THEME_NAMES {
        let is_selected = key == draw_ctx.theme_key;
        let scheme = katla_ui::ColorScheme::by_name(key).unwrap_or_else(|| draw_ctx.theme.clone());

        let mut row_children: Vec<Box<dyn Widget>> = vec![
            theme_swatch(scheme).boxed(),
            hstack([text(display_name)
                .color(if is_selected {
                    theme.text_primary
                } else {
                    theme.text_secondary
                })
                .font_size(FontSize::Small)
                .boxed()])
            .flex_grow(1.0)
            .boxed(),
        ];
        if is_selected {
            row_children.push(icon(ForkAwesome::CHECK).color(theme.accent).boxed());
        }
        let row = hstack(row_children)
            .spacing(8.0)
            .padding(Padding::horizontal(8.0))
            .align(Alignment::Middle)
            .flex_height(katla_ui::tokens::CONTROL_HEIGHT);

        let key_owned = key.to_string();
        cells.push(
            selectable(row.boxed())
                .selected(is_selected)
                .on_click(ctx.on_click(move |actions| {
                    actions.emit(PreferencesAction::SetTheme(key_owned.clone()));
                }))
                .boxed(),
        );
    }

    children.push(
        grid(
            2,
            katla_math::Vec2::new(
                190.0,
                katla_ui::tokens::CONTROL_HEIGHT + katla_ui::tokens::SPACING_4,
            ),
            cells,
        )
        .grid_spacing(4.0)
        .boxed(),
    );

    children.push(section_divider(theme));

    // ── Interface scale ──
    children.push(section_title("Interface scale", theme));
    children.push(
        text("Sizes apply immediately and are saved for next launch.")
            .color(theme.text_muted)
            .font_size(FontSize::Small)
            .boxed(),
    );

    let scale_options: Vec<(String, f32)> = UI_SCALE_STEPS
        .iter()
        .map(|&step| (format!("{}%", (step * 100.0) as i32), step))
        .collect();
    children.push(
        text("UI scale")
            .color(theme.text_secondary)
            .font_size(FontSize::Medium)
            .boxed(),
    );
    children.push(segmented(
        ctx,
        theme,
        &scale_options,
        draw_ctx.preferences.font_scale,
        PreferencesAction::SetFontScale,
    ));

    vstack(children).spacing(8.0).boxed()
}

// ---------------------------------------------------------------------------
// Viewport
// ---------------------------------------------------------------------------

fn build_viewport(
    ctx: &mut BuildContext,
    draw_ctx: &PreferencesDrawCtx,
    camera_speed_id: StateId,
) -> Box<dyn Widget> {
    let theme = &draw_ctx.theme;
    let mut children: Vec<Box<dyn Widget>> = Vec::new();

    children.push(section_title("Display", theme));
    let grid_toggle_id = ctx.state(draw_ctx.preferences.show_grid);
    children.push(toggle("Show Grid", grid_toggle_id).boxed());
    let stats_toggle_id = ctx.state(draw_ctx.preferences.show_stats);
    children.push(toggle("Show Stats Panel", stats_toggle_id).boxed());

    children.push(section_divider(theme));

    children.push(section_title("Grid", theme));
    let sizes: Vec<(String, f32)> = vec![
        ("0.5".to_string(), 0.5),
        ("1".to_string(), 1.0),
        ("2".to_string(), 2.0),
        ("5".to_string(), 5.0),
        ("10".to_string(), 10.0),
    ];
    children.push(setting_row(
        "Grid size",
        theme,
        segmented(
            ctx,
            theme,
            &sizes,
            draw_ctx.editor_settings.grid_size,
            PreferencesAction::SetGridSize,
        ),
    ));
    let snap_id = ctx.state(draw_ctx.editor_settings.snap_to_grid);
    children.push(toggle("Snap to Grid", snap_id).boxed());

    children.push(section_divider(theme));

    children.push(section_title("Camera", theme));
    children.push(setting_row(
        "Fly speed",
        theme,
        labeled_slider("", camera_speed_id, 5.0..=200.0)
            .show_value(true)
            .precision(0)
            .boxed(),
    ));

    vstack(children).spacing(12.0).boxed()
}

// ---------------------------------------------------------------------------
// Audio
// ---------------------------------------------------------------------------

fn build_audio(
    ctx: &mut BuildContext,
    draw_ctx: &PreferencesDrawCtx,
    master_id: StateId,
    sfx_id: StateId,
    music_id: StateId,
    ambient_id: StateId,
) -> Box<dyn Widget> {
    let theme = &draw_ctx.theme;
    let _ = ctx;
    let mut children: Vec<Box<dyn Widget>> = Vec::new();

    children.push(section_title("Volume", theme));

    for (label, id) in [
        ("Master", master_id),
        ("Sound effects", sfx_id),
        ("Music", music_id),
        ("Ambient", ambient_id),
    ] {
        children.push(
            labeled_slider(label, id, 0.0..=1.0)
                .label_width(LABEL_WIDTH)
                .show_value(true)
                .precision(0)
                .value_display(100.0, "%")
                .boxed(),
        );
    }

    vstack(children).spacing(12.0).boxed()
}

fn build_connection(
    ctx: &mut BuildContext,
    draw_ctx: &PreferencesDrawCtx,
    socket_id: StateId,
    thread_id: StateId,
) -> Box<dyn Widget> {
    let socket = ctx.get_state::<String>(socket_id).unwrap_or_default();
    let thread = ctx.get_state::<String>(thread_id).unwrap_or_default();
    vstack([
        section_title("Scene assistant connection", &draw_ctx.theme),
        text("Attach to an existing Codex conversation. Your questions include the current view.").wrap(350.0).color(draw_ctx.theme.text_muted).font_size(FontSize::Small).boxed(),
        text("Private host socket").font_size(FontSize::Small).boxed(),
        textfield("/absolute/path/to/control.sock", socket_id).boxed(),
        text("Conversation ID").font_size(FontSize::Small).boxed(),
        textfield("Existing conversation ID", thread_id).boxed(),
        button("Save and connect").on_click(ctx.on_click(move |actions| {
            actions.emit(super::co_creator::CoCreatorConnectAction {socket: socket.clone(), thread_id: thread.clone()});
        })).boxed(),
        text("The host must expose a private control socket. Sign-in, models and approvals are handled in Codex.").wrap(350.0).color(draw_ctx.theme.text_muted).font_size(FontSize::Small).boxed(),
    ]).spacing(8.0).align(Alignment::Leading).boxed()
}
