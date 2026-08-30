use std::boxed::Box;

use katla_math::Color;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, StateId, Widget, WidgetBox, hstack, image_button, menu_entry,
    menu_group, menubar, text, zstack,
};
use katla_ui::{FontSize, ForkAwesome};

use crate::ui::editor_ui::types::SpawnableModel;

/// Environment data injected before each frame.
#[derive(Clone)]
pub(crate) struct ToolbarDrawCtx {
    pub show_grid: bool,
    pub show_stats: bool,
    pub show_physics_debug: bool,
    pub show_reverb_debug: bool,
    pub text_muted: Color,
    pub is_playing: bool,
    pub is_paused: bool,
    pub highlight: Color,
    pub warning: Color,
    pub accent: Color,
    pub error: Color,
}

/// Actions emitted by the declarative toolbar.
#[derive(Clone, Debug)]
pub(crate) enum ToolbarAction {
    NewScene,
    OpenScene,
    SaveScene,
    Quit,
    Undo,
    Redo,
    OpenPreferences,
    ToggleGrid,
    ToggleStats,
    TogglePhysicsDebug,
    ToggleReverbDebug,
    OpenParticleInspector,
    OpenCoCreator,
    SpawnModel(SpawnableModel),
    PlayStart,
    PlayPause,
    PlayStop,
}

pub(crate) struct ToolbarView;

pub(crate) const TOOLBAR_HEIGHT: f32 = katla_ui::tokens::APP_BAR_HEIGHT;

impl Build for ToolbarView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let draw_ctx = ctx.env::<ToolbarDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return katla_ui::declarative::empty().boxed();
        };

        let file_open_id: StateId = ctx.state(false);
        let edit_open_id: StateId = ctx.state(false);
        let view_open_id: StateId = ctx.state(false);
        let create_open_id: StateId = ctx.state(false);

        let file_menu = build_file_menu(ctx);
        let edit_menu = build_edit_menu(ctx);
        let view_menu = build_view_menu(ctx, &draw_ctx);
        let create_menu = build_create_menu(ctx);

        let bar = menubar(vec![
            menu_group("File", file_open_id, file_menu),
            menu_group("Edit", edit_open_id, edit_menu),
            menu_group("View", view_open_id, view_menu),
            menu_group("Create", create_open_id, create_menu),
        ])
        .menubar_height(TOOLBAR_HEIGHT);

        let title = build_title(&draw_ctx);
        let controls = build_controls(ctx, &draw_ctx);

        zstack([
            (Alignment::TopLeading, bar.boxed()),
            (Alignment::Center, title),
            (Alignment::TopTrailing, controls),
        ])
        .flex_height(TOOLBAR_HEIGHT)
        .boxed()
    }
}

fn build_file_menu(ctx: &mut BuildContext) -> Vec<katla_ui::declarative::MenuEntry> {
    vec![
        menu_entry("New Scene").on_click(ctx.on_click(|actions| {
            actions.emit(ToolbarAction::NewScene);
        })),
        menu_entry("Open...").on_click(ctx.on_click(|actions| {
            actions.emit(ToolbarAction::OpenScene);
        })),
        menu_entry("Save").on_click(ctx.on_click(|actions| {
            actions.emit(ToolbarAction::SaveScene);
        })),
        katla_ui::declarative::MenuEntry {
            label: String::new(),
            on_click: None,
            disabled: false,
        },
        menu_entry("Quit").on_click(ctx.on_click(|actions| {
            actions.emit(ToolbarAction::Quit);
        })),
    ]
}

fn build_edit_menu(ctx: &mut BuildContext) -> Vec<katla_ui::declarative::MenuEntry> {
    vec![
        menu_entry("Undo").on_click(ctx.on_click(|actions| {
            actions.emit(ToolbarAction::Undo);
        })),
        menu_entry("Redo").on_click(ctx.on_click(|actions| {
            actions.emit(ToolbarAction::Redo);
        })),
        katla_ui::declarative::MenuEntry {
            label: String::new(),
            on_click: None,
            disabled: false,
        },
        menu_entry("Preferences...").on_click(ctx.on_click(|actions| {
            actions.emit(ToolbarAction::OpenPreferences);
        })),
    ]
}

fn build_view_menu(
    ctx: &mut BuildContext,
    draw_ctx: &ToolbarDrawCtx,
) -> Vec<katla_ui::declarative::MenuEntry> {
    vec![
        menu_entry(if draw_ctx.show_grid {
            "Grid (on)"
        } else {
            "Grid (off)"
        })
        .on_click(ctx.on_click(|actions| {
            actions.emit(ToolbarAction::ToggleGrid);
        })),
        menu_entry(if draw_ctx.show_stats {
            "Stats (on)"
        } else {
            "Stats (off)"
        })
        .on_click(ctx.on_click(|actions| {
            actions.emit(ToolbarAction::ToggleStats);
        })),
        menu_entry(if draw_ctx.show_physics_debug {
            "Physics Debug (on)"
        } else {
            "Physics Debug (off)"
        })
        .on_click(ctx.on_click(|actions| {
            actions.emit(ToolbarAction::TogglePhysicsDebug);
        })),
        menu_entry(if draw_ctx.show_reverb_debug {
            "Reverb Zones (on)"
        } else {
            "Reverb Zones (off)"
        })
        .on_click(ctx.on_click(|actions| {
            actions.emit(ToolbarAction::ToggleReverbDebug);
        })),
        katla_ui::declarative::MenuEntry {
            label: String::new(),
            on_click: None,
            disabled: false,
        },
        menu_entry("Particle Inspector").on_click(ctx.on_click(|actions| {
            actions.emit(ToolbarAction::OpenParticleInspector);
        })),
        menu_entry("AI Co-Creator").on_click(ctx.on_click(|actions| {
            actions.emit(ToolbarAction::OpenCoCreator);
        })),
    ]
}

fn build_create_menu(ctx: &mut BuildContext) -> Vec<katla_ui::declarative::MenuEntry> {
    SpawnableModel::all()
        .iter()
        .map(|model| {
            let model = *model;
            menu_entry(model.name()).on_click(ctx.on_click(move |actions| {
                actions.emit(ToolbarAction::SpawnModel(model));
            }))
        })
        .collect()
}

fn build_title(draw_ctx: &ToolbarDrawCtx) -> Box<dyn Widget> {
    let title = if draw_ctx.is_playing && !draw_ctx.is_paused {
        "Katla Engine — Playing"
    } else if draw_ctx.is_paused {
        "Katla Engine — Paused"
    } else {
        "Katla Engine"
    };
    let title_color = if draw_ctx.is_playing || draw_ctx.is_paused {
        draw_ctx.highlight
    } else {
        draw_ctx.text_muted
    };
    text(title)
        .color(title_color)
        .font_size(FontSize::Medium)
        .boxed()
}

/// Run controls form one cluster: [Play/Pause] [Stop]. Stop only appears
/// while a session is running or paused; idle state shows Play alone.
fn build_controls(ctx: &mut BuildContext, draw_ctx: &ToolbarDrawCtx) -> Box<dyn Widget> {
    let running = draw_ctx.is_playing || draw_ctx.is_paused;

    let (primary_icon, primary_fill, primary_action): (
        char,
        katla_math::Color,
        fn() -> ToolbarAction,
    ) = if running && !draw_ctx.is_paused {
        (ForkAwesome::PAUSE, draw_ctx.warning, || {
            ToolbarAction::PlayPause
        })
    } else if running {
        (ForkAwesome::PLAY, draw_ctx.accent, || {
            ToolbarAction::PlayPause
        })
    } else {
        (ForkAwesome::PLAY, draw_ctx.accent, || {
            ToolbarAction::PlayStart
        })
    };

    let primary = image_button(primary_icon)
        .fill(primary_fill)
        .on_click(ctx.on_click(move |actions| {
            actions.emit(primary_action());
        }));

    let mut children: Vec<Box<dyn Widget>> = vec![primary.boxed()];
    if running {
        let stop = image_button(ForkAwesome::STOP)
            .fill(draw_ctx.error)
            .on_click(ctx.on_click(|actions| {
                actions.emit(ToolbarAction::PlayStop);
            }));
        children.push(stop.boxed());
    }

    hstack(children).spacing(4.0).padding_all(4.0).boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toolbar_height_matches_app_bar_token() {
        assert_eq!(TOOLBAR_HEIGHT, katla_ui::tokens::APP_BAR_HEIGHT);
    }
}
