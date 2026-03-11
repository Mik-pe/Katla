use katla_math::{Color, Rect2D, Vec2};
use katla_ui::{mouse_button, FontSize, Response, TextureId, UiContext, Widget};

use super::{FocusedPanel, Theme};

pub struct Viewport<'a> {
    pub bounds: Rect2D,
    pub texture_id: TextureId,
    pub theme: &'a Theme,
    pub focused_panel: &'a mut FocusedPanel,
}

impl<'a> Viewport<'a> {
    pub fn new(
        bounds: Rect2D,
        texture_id: TextureId,
        theme: &'a Theme,
        focused_panel: &'a mut FocusedPanel,
    ) -> Self {
        Self {
            bounds,
            texture_id,
            theme,
            focused_panel,
        }
    }
}

impl<'a> Widget for Viewport<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        if ui.is_hovered(self.bounds) && ui.mouse_clicked(mouse_button::LEFT) {
            *self.focused_panel = FocusedPanel::Viewport;
        }

        ui.draw_image(
            self.bounds,
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 1.0),
            Color::WHITE,
            self.texture_id,
        );

        let border_width = 2.0;
        let border_color = self.theme.viewport_border;

        ui.draw_rect(
            Rect2D::from_origin_size(
                self.bounds.min,
                Vec2::new(self.bounds.width(), border_width),
            ),
            border_color,
        );
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(self.bounds.min.x(), self.bounds.max.y() - border_width),
                Vec2::new(self.bounds.width(), border_width),
            ),
            border_color,
        );
        ui.draw_rect(
            Rect2D::from_origin_size(
                self.bounds.min,
                Vec2::new(border_width, self.bounds.height()),
            ),
            border_color,
        );
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(self.bounds.max.x() - border_width, self.bounds.min.y()),
                Vec2::new(border_width, self.bounds.height()),
            ),
            border_color,
        );

        let vp_label = "3D View";
        let label_pos = Vec2::new(self.bounds.min.x() + 8.0, self.bounds.min.y() + 8.0);
        ui.draw_text(
            vp_label,
            label_pos,
            self.theme.text_muted,
            ui.scaled_font_size(FontSize::XSmall),
        );

        Response::default()
    }
}
