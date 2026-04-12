use katla_math::Vec2;
use katla_ui::{Response, UiContext, Widget, widgets::StatusBar as StatusBarWidget};

use super::ColorScheme;

pub struct StatusBarConfig<'a> {
    pub screen_size: Vec2,
    pub height: f32,
    pub fps: f32,
    pub frame_count: usize,
    pub entity_count: usize,
    pub selected_count: usize,
    pub total_assets: usize,
    pub is_playing: bool,
    pub theme: &'a ColorScheme,
    pub save_confirmation_timer: f32,
}

pub struct StatusBar<'a> {
    screen_size: Vec2,
    height: f32,
    fps: f32,
    frame_count: usize,
    entity_count: usize,
    selected_count: usize,
    total_assets: usize,
    is_playing: bool,
    theme: &'a ColorScheme,
    save_confirmation_timer: f32,
}

impl<'a> StatusBar<'a> {
    pub fn new(config: StatusBarConfig<'a>) -> Self {
        Self {
            screen_size: config.screen_size,
            height: config.height,
            fps: config.fps,
            frame_count: config.frame_count,
            entity_count: config.entity_count,
            selected_count: config.selected_count,
            total_assets: config.total_assets,
            is_playing: config.is_playing,
            theme: config.theme,
            save_confirmation_timer: config.save_confirmation_timer,
        }
    }
}

impl<'a> Widget for StatusBar<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let y = self.screen_size.y() - self.height;
        let bar = StatusBarWidget::new(self.screen_size.x(), self.height, y);
        bar.show(ui);

        let font_size = ui.style().font_size;
        let bar_top_y = y + (self.height - font_size) * 0.5;

        let fps_text = format!("FPS: {:.0}", self.fps);
        let fps_color = if self.fps >= 55.0 {
            self.theme.success
        } else if self.fps >= 30.0 {
            self.theme.warning
        } else {
            self.theme.error
        };
        ui.status_label(&fps_text, fps_color);

        ui.status_separator();

        let frame_text = format!("Frame: {}", self.frame_count);
        ui.status_label(&frame_text, self.theme.text_secondary);

        ui.status_separator();

        let entity_text = format!("Entities: {}", self.entity_count);
        ui.status_label(&entity_text, self.theme.text_secondary);

        ui.status_separator();

        let selection_text = if self.selected_count > 0 {
            format!("Selected: {} / {}", self.selected_count, self.total_assets)
        } else {
            format!("Assets: {}", self.total_assets)
        };
        let selection_color = if self.selected_count > 0 {
            self.theme.highlight
        } else {
            self.theme.text_secondary
        };
        ui.status_label(&selection_text, selection_color);

        let mode_text = if self.is_playing {
            "PLAYING"
        } else {
            "EDITING"
        };
        let mode_color = if self.is_playing {
            self.theme.success
        } else {
            self.theme.text_secondary
        };
        let mode_size = ui.measure_text(mode_text, font_size);
        let mode_pos = Vec2::new(
            self.screen_size.x() - mode_size.x() - ui.style().panel_padding,
            bar_top_y,
        );
        ui.draw_text(mode_text, mode_pos, mode_color, font_size);

        let theme_text = format!("ColorScheme: {}", self.theme.name);
        let theme_size = ui.measure_text(&theme_text, font_size);
        let theme_pos = Vec2::new(
            self.screen_size.x() - mode_size.x() - theme_size.x() - 100.0,
            bar_top_y,
        );
        ui.draw_text(&theme_text, theme_pos, self.theme.text_muted, font_size);

        if self.save_confirmation_timer > 0.0 {
            let save_text = "✓ Scene saved";
            let save_size = ui.measure_text(save_text, font_size);
            let save_x = (self.screen_size.x() - save_size.x()) * 0.5;
            let alpha = if self.save_confirmation_timer < 0.5 {
                self.save_confirmation_timer / 0.5
            } else {
                1.0
            };
            let save_color = self.theme.success.with_alpha(alpha);
            ui.draw_text(
                save_text,
                Vec2::new(save_x, bar_top_y),
                save_color,
                font_size,
            );
        }

        Response::default()
    }
}
