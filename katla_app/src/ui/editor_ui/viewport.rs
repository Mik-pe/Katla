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

        ui.image(self.texture_id, self.bounds, None, Some(Color::WHITE));

        ui.draw_selection_border(self.bounds, self.theme.viewport_border, 2.0);

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
