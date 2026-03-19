use katla_math::{Rect2D, Vec2};
use katla_ui::{FontSize, Response, UiContext, Widget};

use super::Theme;

pub struct StatusBar<'a> {
    pub screen_size: Vec2,
    pub height: f32,
    pub fps: f32,
    pub frame_count: usize,
    pub entity_count: usize,
    pub selected_count: usize,
    pub total_assets: usize,
    pub is_playing: bool,
    pub theme: &'a Theme,
}

impl<'a> StatusBar<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        screen_size: Vec2,
        height: f32,
        fps: f32,
        frame_count: usize,
        entity_count: usize,
        selected_count: usize,
        total_assets: usize,
        is_playing: bool,
        theme: &'a Theme,
    ) -> Self {
        Self {
            screen_size,
            height,
            fps,
            frame_count,
            entity_count,
            selected_count,
            total_assets,
            is_playing,
            theme,
        }
    }
}

impl<'a> Widget for StatusBar<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let bar_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, self.screen_size.y() - self.height),
            Vec2::new(self.screen_size.x(), self.height),
        );

        ui.draw_rect(bar_bounds, self.theme.background_dark);
        ui.draw_line(
            bar_bounds.min,
            Vec2::new(self.screen_size.x(), bar_bounds.min.y()),
            self.theme.separator,
            1.0,
        );

        ui.begin_row();
        ui.set_cursor(Vec2::new(8.0, bar_bounds.min.y() + 4.0));

        let font_size = ui.scaled_font_size(FontSize::Small);
        let fps_text = format!("FPS: {:.0}", self.fps);
        let fps_color = if self.fps >= 55.0 {
            self.theme.success
        } else if self.fps >= 30.0 {
            self.theme.warning
        } else {
            self.theme.error
        };
        let fps_slot_width = ui.measure_text("FPS: 1000", font_size).x();
        ui.draw_text(&fps_text, ui.cursor(), fps_color, font_size);
        ui.spacer(fps_slot_width);

        ui.separator_text();

        let frame_text = format!("Frame: {}", self.frame_count);
        ui.text_label(&frame_text);
        ui.separator_text();

        let entity_text = format!("Entities: {}", self.entity_count);
        ui.text_label(&entity_text);
        ui.separator_text();

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
        ui.text_label_colored(&selection_text, selection_color);

        ui.end_row();

        let start_y = bar_bounds.min.y() + 4.0;

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
        let mode_pos = Vec2::new(self.screen_size.x() - mode_size.x() - 8.0, start_y);
        ui.draw_text(mode_text, mode_pos, mode_color, font_size);

        let theme_text = format!("Theme: {}", self.theme.name);
        let theme_size = ui.measure_text(&theme_text, font_size);
        let theme_pos = Vec2::new(
            self.screen_size.x() - mode_size.x() - theme_size.x() - 100.0,
            start_y,
        );
        ui.draw_text(&theme_text, theme_pos, self.theme.text_muted, font_size);

        Response::default()
    }
}
