use katla_math::Color;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, StateId, ViewDescriptor, hstack, image_button, menu_entry,
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
    pub success: Color,
    pub warning: Color,
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

const TOOLBAR_HEIGHT: f32 = 36.0;

impl Build for ToolbarView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let draw_ctx = ctx.env::<ToolbarDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return ViewDescriptor::Empty;
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
            (Alignment::TopLeading, bar),
            (Alignment::Center, title),
            (Alignment::TopTrailing, controls),
        ])
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

fn build_title(draw_ctx: &ToolbarDrawCtx) -> ViewDescriptor {
    let title = if draw_ctx.is_playing && !draw_ctx.is_paused {
        "Katla Engine [PLAYING]"
    } else if draw_ctx.is_paused {
        "Katla Engine [PAUSED]"
    } else {
        "Katla Engine"
    };
    let title_color = if draw_ctx.is_playing || draw_ctx.is_paused {
        draw_ctx.highlight
    } else {
        draw_ctx.text_muted
    };
    text(title).color(title_color).font_size(FontSize::Medium)
}

fn build_controls(ctx: &mut BuildContext, draw_ctx: &ToolbarDrawCtx) -> ViewDescriptor {
    let stop_color = Color::from_rgb_hex(0xe06c75);

    if !draw_ctx.is_playing && !draw_ctx.is_paused {
        let play = image_button(ForkAwesome::PLAY)
            .fill(draw_ctx.success)
            .on_click(ctx.on_click(|actions| {
                actions.emit(ToolbarAction::PlayStart);
            }));
        hstack([play]).spacing(4.0).padding_all(6.0)
    } else if draw_ctx.is_playing && !draw_ctx.is_paused {
        let pause = image_button(ForkAwesome::PAUSE)
            .fill(draw_ctx.warning)
            .on_click(ctx.on_click(|actions| {
                actions.emit(ToolbarAction::PlayPause);
            }));
        let stop = image_button(ForkAwesome::STOP)
            .fill(stop_color)
            .on_click(ctx.on_click(|actions| {
                actions.emit(ToolbarAction::PlayStop);
            }));
        hstack([pause, stop]).spacing(4.0).padding_all(6.0)
    } else {
        let play = image_button(ForkAwesome::PLAY)
            .fill(draw_ctx.success)
            .on_click(ctx.on_click(|actions| {
                actions.emit(ToolbarAction::PlayPause);
            }));
        let stop = image_button(ForkAwesome::STOP)
            .fill(stop_color)
            .on_click(ctx.on_click(|actions| {
                actions.emit(ToolbarAction::PlayStop);
            }));
        hstack([play, stop]).spacing(4.0).padding_all(6.0)
    }
}
