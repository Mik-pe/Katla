//! Internal widget implementations.
//!
//! This module contains the actual rendering and interaction logic for basic widgets.
//! These are private implementation details used by the declarative draw pipeline
//! and by immediate-mode builder widgets in `crate::widgets`.

use katla_math::{Color, Rect2D, Vec2};

use crate::input::mouse_button;
use crate::response::Response;

use super::super::UiContext;
use super::super::drawing::center_in_bounds;

impl UiContext {
    /// Draw a button with optional custom background colors and border.
    pub(crate) fn button_with_colors(
        &mut self,
        id: &str,
        text: &str,
        bounds: Rect2D,
        fill_color: Option<Color>,
        hover_color: Option<Color>,
        border_color: Option<Color>,
    ) -> Response {
        let widget_id = self.generate_id(id);
        self.register_focusable(widget_id, bounds);

        let hovered = self.update_hover(widget_id, bounds);
        let click_result = self.click_interaction(
            widget_id,
            hovered,
            bounds,
            super::super::interaction::ClickConfig::POPUP_BYPASS,
        );
        let clicked = click_result.is_clicked();
        let active = click_result.is_active();

        // Determine background color based on state
        let bg_color = if active {
            hover_color.unwrap_or(self.style.button_active)
        } else if hovered {
            hover_color.unwrap_or(self.style.button_hovered)
        } else {
            fill_color.unwrap_or(self.style.button_normal)
        };

        // Draw button background
        self.draw_rounded_rect(bounds, bg_color, self.style.button_rounding);

        // Subtle top highlight for raised feel (only when not pressed)
        if !active {
            let highlight = Color::new(
                (bg_color.r + 0.04).min(1.0),
                (bg_color.g + 0.04).min(1.0),
                (bg_color.b + 0.04).min(1.0),
                bg_color.a,
            );
            self.draw_line(
                Vec2::new(
                    bounds.min.x() + self.style.button_rounding,
                    bounds.min.y() + 0.5,
                ),
                Vec2::new(
                    bounds.max.x() - self.style.button_rounding,
                    bounds.min.y() + 0.5,
                ),
                highlight,
                1.0,
            );
        }

        // Draw border if specified
        if let Some(border_color) = border_color {
            self.draw_rounded_selection_border(
                bounds,
                border_color,
                1.0,
                self.style.button_rounding,
            );
        }

        // Draw button text
        let text_size = self.measure_text(text, self.style.font_size);
        let text_pos = center_in_bounds(bounds, text_size);
        self.draw_text(text, text_pos, self.style.text_color, self.style.font_size);

        if hovered {
            self.input.set_cursor(crate::input::MouseCursor::Hand);
        }

        Response::interactive(
            clicked,
            hovered,
            active,
            bounds,
            &self.input,
            Some(widget_id),
        )
    }
    pub fn image_button(
        &mut self,
        id: &str,
        icon: char,
        bounds: Rect2D,
        enabled: bool,
    ) -> Response {
        let widget_id = self.generate_id(id);
        if enabled {
            self.register_focusable(widget_id, bounds);
        }

        let hovered = self.update_hover(widget_id, bounds) && enabled;
        let click_result = self.click_interaction(
            widget_id,
            hovered,
            bounds,
            super::super::interaction::ClickConfig::POPUP_AWARE,
        );
        let clicked = click_result.is_clicked() && enabled;
        let active = click_result.is_active() && enabled;

        // Determine colors based on state
        let bg_color = if !enabled {
            self.style.button_normal * 0.5
        } else if active {
            self.style.button_active
        } else if hovered {
            self.style.button_hovered
        } else {
            Color::TRANSPARENT
        };

        // Draw button background
        self.draw_rounded_rect(bounds, bg_color, self.style.button_rounding);

        // Determine icon color based on state
        let icon_color = if !enabled {
            self.style.button_text * 0.5
        } else if hovered {
            self.style.button_text
        } else {
            self.style.button_text * 0.8
        };

        // Draw icon centered
        let icon_size = bounds.height().min(bounds.width()) * 0.6;
        self.draw_icon_centered(icon, bounds, icon_size, icon_color);

        let mut response = Response::interactive(
            clicked,
            hovered,
            active,
            bounds,
            &self.input,
            Some(widget_id),
        );
        if !enabled {
            response.clicked = false;
            response.changed = false;
            response.double_clicked = false;
        }
        if hovered {
            self.input.set_cursor(crate::input::MouseCursor::Hand);
        }
        response
    }

    /// Draw a slider (internal - used by declarative draw pipeline).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn slider(
        &mut self,
        id: &str,
        value: &mut f32,
        min: f32,
        max: f32,
        bounds: Rect2D,
        show_value: bool,
        value_precision: usize,
    ) -> Response {
        let widget_id = self.generate_id(id);
        self.register_focusable(widget_id, bounds);

        let hovered = self.update_hover(widget_id, bounds);
        let active = self.active_id == Some(widget_id);

        // Handle dragging
        let mut changed = false;

        if active {
            if self.input.mouse_down[mouse_button::LEFT] {
                let t =
                    ((self.input.mouse_pos.x() - bounds.min.x()) / bounds.width()).clamp(0.0, 1.0);
                let new_value = min + t * (max - min);
                if (new_value - *value).abs() > 0.0001 {
                    *value = new_value;
                    changed = true;
                }
            } else {
                self.active_id = None;
            }
        } else if hovered && self.input.mouse_pressed[mouse_button::LEFT] {
            self.active_id = Some(widget_id);
        }

        // Draw track
        let track_height = self.style.slider_track_height;
        let track_bounds = Rect2D::from_center_size(
            Vec2::new(bounds.center().x(), bounds.center().y()),
            Vec2::new(bounds.width(), track_height),
        );
        self.draw_rounded_rect(track_bounds, self.style.slider_track, track_height * 0.5);

        // Draw filled portion of the track
        let t = (*value - min) / (max - min);
        let fill_width = t * bounds.width();
        if fill_width > 0.0 {
            let fill_bounds =
                Rect2D::from_origin_size(track_bounds.min, Vec2::new(fill_width, track_height));
            self.draw_rounded_rect(fill_bounds, self.style.slider_grab, track_height * 0.5);
        }

        // Draw grab as a circle with shadow and hover scale
        let grab_color = if active {
            self.style.slider_grab_active
        } else if hovered {
            self.style.slider_grab_hovered
        } else {
            self.style.slider_grab
        };
        let grab_center_x = bounds.min.x() + t * bounds.width();
        let grab_center = Vec2::new(grab_center_x, bounds.center().y());
        let base_radius = self.style.slider_grab_size * 0.5;
        let grab_radius = if active {
            base_radius * 1.25
        } else if hovered {
            base_radius * 1.15
        } else {
            base_radius
        };

        // Shadow
        self.draw_circle(
            Vec2::new(grab_center.x(), grab_center.y() + 1.0),
            grab_radius,
            Color::new(0.0, 0.0, 0.0, 0.3),
        );
        // Main grab
        self.draw_circle(grab_center, grab_radius, grab_color);

        if show_value {
            let value_text = format!("{:.1$}", *value, value_precision);
            let text_size = self.measure_text(&value_text, self.style.font_size);
            let text_pos = center_in_bounds(bounds, text_size);
            self.draw_text(
                &value_text,
                text_pos,
                self.style.text_color,
                self.style.font_size,
            );
        }

        let mut response =
            Response::interactive(false, hovered, active, bounds, &self.input, Some(widget_id));
        response.changed = changed;
        response
    }
}
