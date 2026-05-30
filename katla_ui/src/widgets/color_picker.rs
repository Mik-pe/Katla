use katla_math::{Color, Rect2D, Vec2};

use crate::input::mouse_button;
use crate::style::DEFAULTS;
use crate::{Response, UiContext, Widget, z_index};

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    if s < 1e-6 {
        return [v, v, v];
    }
    let h = ((h % 1.0) + 1.0) % 1.0;
    let h6 = h * 6.0;
    let i = h6.floor() as i32;
    let f = h6 - h6.floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

pub fn rgb_to_hsv(r: f32, g: f32, b: f32) -> [f32; 3] {
    let mut k = 0.0;
    let (mut mx, mut mid, mut mn) = (r, g, b);
    if mid < mn {
        std::mem::swap(&mut mid, &mut mn);
        k = -1.0;
    }
    if mx < mid {
        std::mem::swap(&mut mx, &mut mid);
        k = -2.0 / 6.0 - k;
    }
    let chroma = mx - mn;
    let h = (k + (mid - mn) / (6.0 * chroma + 1e-20)).abs();
    let s = chroma / (mx + 1e-20);
    [h, s, mx]
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ColorPickerState {
    pub open: bool,
    hsv: [f32; 3],
    hsv_initialized: bool,
    dragging_sv: bool,
    dragging_hue: bool,
}

impl ColorPickerState {
    pub fn new() -> Self {
        Self {
            open: false,
            hsv: [0.0, 1.0, 1.0],
            hsv_initialized: false,
            dragging_sv: false,
            dragging_hue: false,
        }
    }
}

pub struct ColorPickerButton<'a> {
    label: &'a str,
    color: &'a mut [f32; 3],
    state: &'a mut ColorPickerState,
    bounds: Rect2D,
    id: Option<&'a str>,
}

impl<'a> ColorPickerButton<'a> {
    pub fn new(label: &'a str, color: &'a mut [f32; 3], state: &'a mut ColorPickerState) -> Self {
        Self {
            label,
            color,
            state,
            bounds: Rect2D::from_size(Vec2::new(
                DEFAULTS.combo_default_width,
                DEFAULTS.combo_default_height,
            )),
            id: None,
        }
    }

    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }
}

const SV_SIZE: f32 = 150.0;
const BAR_WIDTH: f32 = 20.0;
const BAR_SPACING: f32 = 4.0;
const PICKER_PADDING: f32 = 8.0;
const HUE_SEGMENTS: u32 = 32;

impl<'a> Widget for ColorPickerButton<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let id = self.id.unwrap_or(self.label);
        let widget_id = ui.generate_id(id);

        ui.register_focusable(widget_id, self.bounds);
        let hovered = ui.update_hover(widget_id, self.bounds);
        let clicked = ui
            .click_interaction(
                widget_id,
                hovered,
                self.bounds,
                crate::context::interaction::ClickConfig::POPUP_BYPASS,
            )
            .is_clicked();

        if hovered {
            ui.input.set_cursor(crate::input::MouseCursor::Hand);
        }

        if clicked {
            self.state.open = !self.state.open;
            if self.state.open && !self.state.hsv_initialized {
                self.state.hsv = rgb_to_hsv(self.color[0], self.color[1], self.color[2]);
                self.state.hsv_initialized = true;
            }
        }

        let current_color = Color::rgb(self.color[0], self.color[1], self.color[2]);
        let bg_color = if self.state.open || hovered {
            ui.style.combo_hovered
        } else {
            ui.style.combo_bg
        };
        ui.draw_rounded_rect(self.bounds, bg_color, ui.style.button_rounding);
        ui.draw_rounded_selection_border(
            self.bounds,
            ui.style.combo_border,
            1.0,
            ui.style.button_rounding,
        );

        let swatch_padding = 4.0;
        let swatch_size = self.bounds.height() - swatch_padding * 2.0;
        let swatch_bounds = Rect2D::from_origin_size(
            Vec2::new(
                self.bounds.min.x() + swatch_padding,
                self.bounds.min.y() + swatch_padding,
            ),
            Vec2::new(swatch_size, swatch_size),
        );
        ui.draw_rounded_rect(swatch_bounds, current_color, 2.0);
        ui.draw_rounded_selection_border(swatch_bounds, ui.style.border, 1.0, 2.0);

        let label_text = self.label;
        let label_size = ui.measure_text(label_text, ui.style.font_size);
        let label_x = swatch_bounds.max.x() + swatch_padding;
        let label_y = self.bounds.center().y() - label_size.y() * 0.5;
        ui.draw_text(
            label_text,
            Vec2::new(label_x, label_y),
            ui.style.text_color,
            ui.style.font_size,
        );

        let hex_text = format!(
            "#{:02X}{:02X}{:02X}",
            (self.color[0] * 255.0) as u8,
            (self.color[1] * 255.0) as u8,
            (self.color[2] * 255.0) as u8
        );
        let hex_size = ui.measure_text(&hex_text, ui.style.font_size);
        let hex_x = self.bounds.max.x() - hex_size.x() - swatch_padding;
        ui.draw_text(
            &hex_text,
            Vec2::new(hex_x, label_y),
            ui.style.text_disabled,
            ui.style.font_size,
        );

        if self.state.open {
            render_color_picker_popup(ui, self.color, self.state, self.bounds);
        }

        let mut response = Response::interactive(
            clicked,
            hovered,
            false,
            self.bounds,
            &ui.input,
            Some(widget_id),
        );
        response.changed = false;
        response
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_labeled_slider(
    ui: &mut UiContext,
    id: &str,
    value: &mut f32,
    range_start: f32,
    range_end: f32,
    bounds: Rect2D,
    label: &str,
    label_width: f32,
    show_value: bool,
    precision: usize,
) -> Response {
    let font_size = ui.style.font_size;
    let text_color = ui.style.text_color;
    let label_text_size = ui.measure_text(label, font_size);

    let label_x = bounds.min.x();
    let label_y = bounds.center().y() - label_text_size.y() * 0.5;
    ui.draw_text(label, Vec2::new(label_x, label_y), text_color, font_size);

    let value_text_width = if show_value {
        let value_text = format!("{:.1$}", *value, precision);
        let size = ui.measure_text(&value_text, font_size);
        size.x() + 8.0
    } else {
        0.0
    };

    let slider_x = bounds.min.x() + label_width;
    let slider_width = (bounds.max.x() - value_text_width) - slider_x;
    let slider_bounds = Rect2D::from_origin_size(
        Vec2::new(slider_x, bounds.min.y()),
        Vec2::new(slider_width.max(0.0), bounds.height()),
    );

    let response = ui.slider(
        id,
        value,
        range_start,
        range_end,
        slider_bounds,
        false,
        precision,
    );

    if show_value {
        let value_text = format!("{:.1$}", *value, precision);
        let value_text_size = ui.measure_text(&value_text, font_size);
        let value_x = bounds.max.x() - value_text_size.x();
        let value_y = bounds.center().y() - value_text_size.y() * 0.5;
        ui.draw_text(
            &value_text,
            Vec2::new(value_x, value_y),
            text_color,
            font_size,
        );
    }

    let mut result = Response::new(bounds);
    result.changed = response.changed;
    result.hovered = response.hovered;
    result.active = response.active;
    result.clicked = response.clicked;
    result
}

fn render_color_picker_popup(
    ui: &mut UiContext,
    color: &mut [f32; 3],
    state: &mut ColorPickerState,
    trigger_bounds: Rect2D,
) {
    let popup_width = SV_SIZE + BAR_WIDTH + BAR_SPACING + PICKER_PADDING * 2.0;
    let slider_height = ui.style.slider_default_height;
    let popup_height = PICKER_PADDING * 2.0 + SV_SIZE + 6.0 + slider_height * 3.0 + 6.0 + 24.0;

    let popup_x = trigger_bounds.min.x();
    let popup_y = (trigger_bounds.max.y() + 4.0).min(ui.screen_size().y() - popup_height - 4.0);
    let popup_pos = Vec2::new(popup_x, popup_y);
    let popup_bounds = Rect2D::from_origin_size(popup_pos, Vec2::new(popup_width, popup_height));

    ui.with_z_index(z_index::POPUP, |ui| {
        let shadow_offset = Vec2::new(4.0, 4.0);
        let shadow_bounds = Rect2D::new(
            popup_bounds.min + shadow_offset,
            popup_bounds.max + shadow_offset,
        );
        ui.draw_rect(shadow_bounds, ui.style.popup_shadow);
        ui.draw_rounded_rect(popup_bounds, ui.style.popup_bg, ui.style.popup_rounding);
        ui.draw_rounded_selection_border(
            popup_bounds,
            ui.style.popup_border,
            1.0,
            ui.style.popup_rounding,
        );

        let content_x = popup_pos.x() + PICKER_PADDING;
        let content_y = popup_pos.y() + PICKER_PADDING;

        let sv_origin = Vec2::new(content_x, content_y);
        let sv_bounds = Rect2D::from_origin_size(sv_origin, Vec2::new(SV_SIZE, SV_SIZE));

        let [h, s, v] = state.hsv;
        let [hr, hg, hb] = hsv_to_rgb(h, 1.0, 1.0);
        let hue_color = Color::rgb(hr, hg, hb);
        let white = Color::WHITE;

        ui.draw_gradient_rect(sv_bounds, white, hue_color, Color::BLACK, Color::BLACK);

        let cursor_x = sv_origin.x() + s * SV_SIZE;
        let cursor_y = sv_origin.y() + (1.0 - v) * SV_SIZE;

        let cursor_r = 5.0;
        let cursor_color = Color::rgb(
            hsv_to_rgb(state.hsv[0], state.hsv[1], state.hsv[2])[0],
            hsv_to_rgb(state.hsv[0], state.hsv[1], state.hsv[2])[1],
            hsv_to_rgb(state.hsv[0], state.hsv[1], state.hsv[2])[2],
        );
        ui.draw_circle(Vec2::new(cursor_x, cursor_y), cursor_r + 1.5, Color::BLACK);
        ui.draw_circle(Vec2::new(cursor_x, cursor_y), cursor_r, cursor_color);

        if state.dragging_sv {
            if ui.mouse_down(mouse_button::LEFT) {
                let mx = ui.mouse_pos().x();
                let my = ui.mouse_pos().y();
                let new_s = ((mx - sv_origin.x()) / SV_SIZE).clamp(0.0, 1.0);
                let new_v = 1.0 - ((my - sv_origin.y()) / SV_SIZE).clamp(0.0, 1.0);
                state.hsv[1] = new_s;
                state.hsv[2] = new_v;
            } else {
                state.dragging_sv = false;
            }
        }

        if !state.dragging_sv && !state.dragging_hue {
            let sv_hovered = ui.is_hovered(sv_bounds);
            if sv_hovered && ui.mouse_clicked(mouse_button::LEFT) {
                state.dragging_sv = true;
                let mx = ui.mouse_pos().x();
                let my = ui.mouse_pos().y();
                state.hsv[1] = ((mx - sv_origin.x()) / SV_SIZE).clamp(0.0, 1.0);
                state.hsv[2] = 1.0 - ((my - sv_origin.y()) / SV_SIZE).clamp(0.0, 1.0);
            }
        }

        let hue_bar_x = content_x + SV_SIZE + BAR_SPACING;
        let hue_bar_origin = Vec2::new(hue_bar_x, content_y);
        let hue_bar_bounds =
            Rect2D::from_origin_size(hue_bar_origin, Vec2::new(BAR_WIDTH, SV_SIZE));

        let seg_height = SV_SIZE / HUE_SEGMENTS as f32;
        for i in 0..HUE_SEGMENTS {
            let t0 = i as f32 / HUE_SEGMENTS as f32;
            let t1 = (i + 1) as f32 / HUE_SEGMENTS as f32;
            let [r0, g0, b0] = hsv_to_rgb(t0, 1.0, 1.0);
            let [r1, g1, b1] = hsv_to_rgb(t1, 1.0, 1.0);
            let seg_y = content_y + i as f32 * seg_height;
            let seg_bounds = Rect2D::from_origin_size(
                Vec2::new(hue_bar_x, seg_y),
                Vec2::new(BAR_WIDTH, seg_height),
            );
            ui.draw_gradient_rect(
                seg_bounds,
                Color::rgb(r0, g0, b0),
                Color::rgb(r0, g0, b0),
                Color::rgb(r1, g1, b1),
                Color::rgb(r1, g1, b1),
            );
        }

        let hue_y = content_y + state.hsv[0] * SV_SIZE;
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(hue_bar_x - 2.0, hue_y - 2.0),
                Vec2::new(BAR_WIDTH + 4.0, 4.0),
            ),
            Color::WHITE,
        );
        ui.draw_line(
            Vec2::new(hue_bar_x - 2.0, hue_y),
            Vec2::new(hue_bar_x, hue_y),
            Color::WHITE,
            2.0,
        );
        ui.draw_line(
            Vec2::new(hue_bar_x + BAR_WIDTH, hue_y),
            Vec2::new(hue_bar_x + BAR_WIDTH + 2.0, hue_y),
            Color::WHITE,
            2.0,
        );

        if state.dragging_hue {
            if ui.mouse_down(mouse_button::LEFT) {
                let my = ui.mouse_pos().y();
                state.hsv[0] = ((my - hue_bar_origin.y()) / SV_SIZE).clamp(0.0, 1.0);
            } else {
                state.dragging_hue = false;
            }
        }

        if !state.dragging_sv && !state.dragging_hue {
            let hue_hovered = ui.is_hovered(hue_bar_bounds);
            if hue_hovered && ui.mouse_clicked(mouse_button::LEFT) {
                state.dragging_hue = true;
                let my = ui.mouse_pos().y();
                state.hsv[0] = ((my - hue_bar_origin.y()) / SV_SIZE).clamp(0.0, 1.0);
            }
        }

        let [r, g, b] = hsv_to_rgb(state.hsv[0], state.hsv[1], state.hsv[2]);
        color[0] = r;
        color[1] = g;
        color[2] = b;

        let slider_y = content_y + SV_SIZE + 6.0;
        let slider_width = popup_width - PICKER_PADDING * 2.0;

        let mut r_val = color[0];
        let mut g_val = color[1];
        let mut b_val = color[2];

        let r_bounds = Rect2D::from_origin_size(
            Vec2::new(content_x, slider_y),
            Vec2::new(slider_width, slider_height),
        );
        let g_bounds = Rect2D::from_origin_size(
            Vec2::new(content_x, slider_y + slider_height),
            Vec2::new(slider_width, slider_height),
        );
        let b_bounds = Rect2D::from_origin_size(
            Vec2::new(content_x, slider_y + slider_height * 2.0),
            Vec2::new(slider_width, slider_height),
        );

        let r_resp =
            draw_labeled_slider(ui, "R", &mut r_val, 0.0, 1.0, r_bounds, "R", 16.0, true, 2);
        let g_resp =
            draw_labeled_slider(ui, "G", &mut g_val, 0.0, 1.0, g_bounds, "G", 16.0, true, 2);
        let b_resp =
            draw_labeled_slider(ui, "B", &mut b_val, 0.0, 1.0, b_bounds, "B", 16.0, true, 2);

        if r_resp.changed || g_resp.changed || b_resp.changed {
            color[0] = r_val;
            color[1] = g_val;
            color[2] = b_val;
            state.hsv = rgb_to_hsv(color[0], color[1], color[2]);
            state.hsv_initialized = true;
        }

        let preview_y = slider_y + slider_height * 3.0 + 6.0;
        let preview_bounds = Rect2D::from_origin_size(
            Vec2::new(content_x, preview_y),
            Vec2::new(slider_width, 20.0),
        );
        let preview_color = Color::rgb(color[0], color[1], color[2]);
        ui.draw_rounded_rect(preview_bounds, preview_color, 3.0);
        ui.draw_rounded_selection_border(preview_bounds, ui.style.border, 1.0, 3.0);
    });

    let mouse_in_popup = popup_bounds.contains(ui.mouse_pos());
    let mouse_in_trigger = trigger_bounds.contains(ui.mouse_pos());
    if !state.dragging_sv
        && !state.dragging_hue
        && ui.mouse_clicked(mouse_button::LEFT)
        && !mouse_in_popup
        && !mouse_in_trigger
    {
        state.open = false;
        state.hsv_initialized = false;
    }
}
