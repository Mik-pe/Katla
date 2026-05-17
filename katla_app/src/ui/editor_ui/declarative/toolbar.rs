use std::cell::RefCell;

use katla_math::{Color, Rect2D, Vec2, Vec3};
use katla_ui::declarative::{Build, BuildContext, ViewDescriptor};
use katla_ui::input::mouse_button;
use katla_ui::{FontSize, ForkAwesome, UiContext};

use crate::Preferences;
use crate::ui::EditorAction;
use crate::ui::editor_ui::Panel;
use crate::ui::editor_ui::types::{SpawnableModel, ToolbarState};

thread_local! {
    static TOOLBAR_CTX: RefCell<Option<ToolbarDrawCtx>> = const { RefCell::new(None) };
}

pub(crate) struct ToolbarDrawCtx {
    pub state: ToolbarState,
    pub screen_width: f32,
    pub show_grid: bool,
    pub show_stats: bool,
    pub text_muted: Color,
    pub is_playing: bool,
    pub is_paused: bool,
    pub highlight: Color,
    pub success: Color,
    pub warning: Color,
}

impl ToolbarDrawCtx {
    pub fn new(
        state: ToolbarState,
        screen_width: f32,
        preferences: &Preferences,
        text_muted: Color,
        is_playing: bool,
        is_paused: bool,
        highlight: Color,
        success: Color,
        warning: Color,
    ) -> Self {
        Self {
            state,
            screen_width,
            show_grid: preferences.show_grid,
            show_stats: preferences.show_stats,
            text_muted,
            is_playing,
            is_paused,
            highlight,
            success,
            warning,
        }
    }
}

pub(crate) fn set_toolbar_ctx(ctx: ToolbarDrawCtx) {
    TOOLBAR_CTX.with(|c| *c.borrow_mut() = Some(ctx));
}

pub(crate) fn take_toolbar_ctx() -> Option<ToolbarDrawCtx> {
    TOOLBAR_CTX.with(|c| c.borrow_mut().take())
}

pub(crate) struct ToolbarView;

impl Build for ToolbarView {
    fn build(&self, _ctx: &mut BuildContext) -> ViewDescriptor {
        ViewDescriptor::Custom(draw_toolbar)
    }
}

const TOOLBAR_HEIGHT: f32 = 36.0;

fn draw_toolbar(ui: &mut UiContext, _bounds: Rect2D) {
    let ctx = TOOLBAR_CTX.with(|c| c.borrow_mut().take());
    let Some(mut ctx) = ctx else {
        return;
    };

    let height = TOOLBAR_HEIGHT;
    let screen_width = ctx.screen_width;

    let menu_bar = katla_ui::widgets::MenuBar::new(screen_width, height);
    menu_bar.show(ui);

    let original_button_normal = ui.style().button_normal;
    ui.style_mut().button_normal = Color::TRANSPARENT;

    let padding = 4.0;
    let menu_item_width = 50.0;
    let button_height = height;

    let file_bounds =
        Rect2D::from_origin_size(ui.cursor(), Vec2::new(menu_item_width, button_height));
    ui.menu_bar_dropdown(
        "file_menu",
        "File",
        file_bounds,
        &mut ctx.state.file_menu_open,
        |ui, open| {
            if ui.menu_item_clicked("New Scene") {
                ctx.state.pending_actions.push(EditorAction::NewScene);
                *open = false;
            }
            if ui.menu_item_clicked("Open...") {
                ctx.state.pending_actions.push(EditorAction::OpenScene);
                *open = false;
            }
            if ui.menu_item_clicked("Save") {
                ctx.state.pending_actions.push(EditorAction::SaveScene);
                *open = false;
            }
            ui.menu_separator();
            if ui.menu_item_clicked("Quit") {
                ctx.state.pending_actions.push(EditorAction::Quit);
                *open = false;
            }
        },
    );
    ui.spacing(menu_item_width);

    let edit_bounds =
        Rect2D::from_origin_size(ui.cursor(), Vec2::new(menu_item_width, button_height));
    ui.menu_bar_dropdown(
        "edit_menu",
        "Edit",
        edit_bounds,
        &mut ctx.state.edit_menu_open,
        |ui, open| {
            let can_undo = ctx.state.undo_count > 0;
            if ui.menu_item_clicked_with_icon_and_shortcut(
                "Undo",
                katla_ui::ForkAwesome::UNDO,
                can_undo,
                "Ctrl+Z",
            ) {
                ctx.state.pending_actions.push(EditorAction::Undo);
                *open = false;
            }
            let can_redo = ctx.state.redo_count > 0;
            if ui.menu_item_clicked_with_icon_and_shortcut(
                "Redo",
                katla_ui::ForkAwesome::REDO,
                can_redo,
                "Ctrl+Shift+Z",
            ) {
                ctx.state.pending_actions.push(EditorAction::Redo);
                *open = false;
            }
            ui.menu_separator();
            if ui.menu_item_clicked("Preferences...") {
                ctx.state
                    .pending_actions
                    .push(EditorAction::OpenPanel(Panel::Preferences));
                *open = false;
            }
        },
    );
    ui.spacing(menu_item_width);

    let view_bounds =
        Rect2D::from_origin_size(ui.cursor(), Vec2::new(menu_item_width, button_height));
    let show_grid = ctx.show_grid;
    let show_stats = ctx.show_stats;
    ui.menu_bar_dropdown(
        "view_menu",
        "View",
        view_bounds,
        &mut ctx.state.view_menu_open,
        |ui, open| {
            if ui.toggle_menu_item_clicked("Grid", show_grid) {
                ctx.state.pending_actions.push(EditorAction::ToggleGrid);
                *open = false;
            }
            if ui.toggle_menu_item_clicked("Stats", show_stats) {
                ctx.state.pending_actions.push(EditorAction::ToggleStats);
                *open = false;
            }
            ui.menu_separator();
            if ui.menu_item_clicked("Particle Inspector") {
                ctx.state
                    .pending_actions
                    .push(EditorAction::OpenPanel(Panel::ParticleInspector));
                *open = false;
            }
            if ui.menu_item_clicked("AI Co-Creator") {
                ctx.state
                    .pending_actions
                    .push(EditorAction::OpenPanel(Panel::CoCreator));
                *open = false;
            }
        },
    );
    ui.spacing(menu_item_width);

    let create_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(60.0, button_height));
    ui.menu_bar_dropdown(
        "create_menu",
        "Create",
        create_bounds,
        &mut ctx.state.create_menu_open,
        |ui, open| {
            for model in SpawnableModel::all() {
                if ui.menu_item_clicked(model.name()) {
                    ctx.state
                        .pending_actions
                        .push(EditorAction::SpawnModel(*model, Vec3::new(0.0, 0.0, 0.0)));
                    *open = false;
                }
            }
        },
    );
    ui.spacing(60.0 + padding);

    menu_bar.end(ui);

    let title = if ctx.is_playing && !ctx.is_paused {
        "Katla Engine [PLAYING]"
    } else if ctx.is_paused {
        "Katla Engine [PAUSED]"
    } else {
        "Katla Engine"
    };
    let title_color = if ctx.is_playing || ctx.is_paused {
        ctx.highlight
    } else {
        ctx.text_muted
    };
    let title_size = ui.measure_text(title, ui.scaled_font_size(FontSize::Medium));
    let title_pos = Vec2::new(
        screen_width * 0.5 - title_size.x() * 0.5,
        height * 0.5 - title_size.y() * 0.5,
    );
    ui.draw_text(
        title,
        title_pos,
        title_color,
        ui.scaled_font_size(FontSize::Medium),
    );

    let original_text_color = ui.style().text_color;

    let btn_size = 24.0;
    let btn_padding = 4.0;
    let btn_y = (height - btn_size) * 0.5;
    let btn_x_start = title_pos.x() + title_size.x() + 12.0;

    fn draw_icon_button(ui: &mut UiContext, icon: char, bounds: Rect2D, color: Color) -> bool {
        let hovered = ui.is_hovered(bounds);
        let bg = if hovered {
            ui.style().button_hovered
        } else {
            Color::TRANSPARENT
        };
        ui.draw_rounded_rect(bounds, bg, ui.style().button_rounding);
        let icon_size = bounds.height() * 0.6;
        let center = bounds.center();
        ui.draw_icon(
            icon,
            Vec2::new(center.x() - icon_size * 0.5, center.y() - icon_size * 0.5),
            icon_size,
            color,
        );
        hovered && ui.mouse_clicked(mouse_button::LEFT)
    }

    if !ctx.is_playing && !ctx.is_paused {
        let play_bounds =
            Rect2D::from_origin_size(Vec2::new(btn_x_start, btn_y), Vec2::new(btn_size, btn_size));
        if draw_icon_button(ui, ForkAwesome::PLAY, play_bounds, ctx.success) {
            ctx.state.pending_actions.push(EditorAction::PlayStart);
        }
    } else if ctx.is_playing && !ctx.is_paused {
        let pause_bounds =
            Rect2D::from_origin_size(Vec2::new(btn_x_start, btn_y), Vec2::new(btn_size, btn_size));
        if draw_icon_button(ui, ForkAwesome::PAUSE, pause_bounds, ctx.warning) {
            ctx.state.pending_actions.push(EditorAction::PlayPause);
        }

        let stop_bounds = Rect2D::from_origin_size(
            Vec2::new(btn_x_start + btn_size + btn_padding, btn_y),
            Vec2::new(btn_size, btn_size),
        );
        if draw_icon_button(
            ui,
            ForkAwesome::STOP,
            stop_bounds,
            Color::from_rgb_hex(0xe06c75),
        ) {
            ctx.state.pending_actions.push(EditorAction::PlayStop);
        }
    } else {
        let play_bounds =
            Rect2D::from_origin_size(Vec2::new(btn_x_start, btn_y), Vec2::new(btn_size, btn_size));
        if draw_icon_button(ui, ForkAwesome::PLAY, play_bounds, ctx.success) {
            ctx.state.pending_actions.push(EditorAction::PlayPause);
        }

        let stop_bounds = Rect2D::from_origin_size(
            Vec2::new(btn_x_start + btn_size + btn_padding, btn_y),
            Vec2::new(btn_size, btn_size),
        );
        if draw_icon_button(
            ui,
            ForkAwesome::STOP,
            stop_bounds,
            Color::from_rgb_hex(0xe06c75),
        ) {
            ctx.state.pending_actions.push(EditorAction::PlayStop);
        }
    }

    ui.style_mut().button_normal = original_button_normal;

    TOOLBAR_CTX.with(|c| *c.borrow_mut() = Some(ctx));
}
