use katla_math::{Rect2D, Vec2};
use katla_ui::declarative::{Build, BuildContext, ViewDescriptor};
use katla_ui::{ColorScheme, UiContext, widgets::StatusBar as StatusBarWidget};

/// Snapshot of data needed to render the status bar each frame.
#[derive(Clone)]
pub(crate) struct StatusBarData {
    pub screen_size: Vec2,
    pub height: f32,
    pub fps: f32,
    pub frame_time_ms: f32,
    pub frame_count: usize,
    pub entity_count: usize,
    pub draw_call_count: usize,
    pub selected_count: usize,
    pub total_assets: usize,
    pub is_playing: bool,
    pub theme: ColorScheme,
    pub save_confirmation_timer: f32,
}

pub(crate) struct StatusBarView;

impl Build for StatusBarView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let data = ctx.env::<StatusBarData>();
        if data.is_none() {
            return ViewDescriptor::Empty;
        }
        ViewDescriptor::Custom(draw_status_bar)
    }
}

fn draw_status_bar(ui: &mut UiContext, _bounds: Rect2D) {
    let data = match ui.get_scratch::<StatusBarData>().cloned() {
        Some(d) => d,
        None => return,
    };

    let screen_size = data.screen_size;
    let height = data.height;
    let y = screen_size.y() - height;

    let bar = StatusBarWidget::new(screen_size.x(), height, y);
    bar.show(ui);

    let font_size = ui.style().font_size;
    let bar_top_y = y + (height - font_size) * 0.5;

    let fps_text = format!("FPS: {:.0}", data.fps);
    let fps_color = if data.fps >= 55.0 {
        data.theme.success
    } else if data.fps >= 30.0 {
        data.theme.warning
    } else {
        data.theme.error
    };
    ui.status_label(&fps_text, fps_color);

    ui.status_separator();

    let frame_time_text = format!("{:.2} ms", data.frame_time_ms);
    let frame_time_color = if data.frame_time_ms <= 18.0 {
        data.theme.success
    } else if data.frame_time_ms <= 33.0 {
        data.theme.warning
    } else {
        data.theme.error
    };
    ui.status_label(&frame_time_text, frame_time_color);

    ui.status_separator();

    let frame_text = format!("Frame: {}", data.frame_count);
    ui.status_label(&frame_text, data.theme.text_secondary);

    ui.status_separator();

    let entity_text = format!("Entities: {}", data.entity_count);
    ui.status_label(&entity_text, data.theme.text_secondary);

    ui.status_separator();

    let draw_text = format!("Draws: {}", data.draw_call_count);
    ui.status_label(&draw_text, data.theme.text_secondary);

    ui.status_separator();

    let selection_text = if data.selected_count > 0 {
        format!("Selected: {} / {}", data.selected_count, data.total_assets)
    } else {
        format!("Assets: {}", data.total_assets)
    };
    let selection_color = if data.selected_count > 0 {
        data.theme.highlight
    } else {
        data.theme.text_secondary
    };
    ui.status_label(&selection_text, selection_color);

    let mode_text = if data.is_playing {
        "PLAYING"
    } else {
        "EDITING"
    };
    let mode_color = if data.is_playing {
        data.theme.success
    } else {
        data.theme.text_secondary
    };
    let mode_size = ui.measure_text(mode_text, font_size);
    let mode_pos = Vec2::new(
        screen_size.x() - mode_size.x() - ui.style().panel_padding,
        bar_top_y,
    );
    ui.draw_text(mode_text, mode_pos, mode_color, font_size);

    let theme_text = format!("ColorScheme: {}", data.theme.name);
    let theme_size = ui.measure_text(&theme_text, font_size);
    let theme_pos = Vec2::new(
        screen_size.x() - mode_size.x() - theme_size.x() - 100.0,
        bar_top_y,
    );
    ui.draw_text(&theme_text, theme_pos, data.theme.text_muted, font_size);

    if data.save_confirmation_timer > 0.0 {
        let save_text = "✓ Scene saved";
        let save_size = ui.measure_text(save_text, font_size);
        let save_x = (screen_size.x() - save_size.x()) * 0.5;
        let alpha = if data.save_confirmation_timer < 0.5 {
            data.save_confirmation_timer / 0.5
        } else {
            1.0
        };
        let save_color = data.theme.success.with_alpha(alpha);
        ui.draw_text(
            save_text,
            Vec2::new(save_x, bar_top_y),
            save_color,
            font_size,
        );
    }
}
