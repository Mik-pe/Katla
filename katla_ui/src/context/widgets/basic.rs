//! Internal widget implementations.
//!
//! This module contains the actual rendering and interaction logic for basic widgets.
//! These are private implementation details called by the public builder widgets
//! in `crate::widgets`.

use katla_math::{Color, Rect2D, Vec2};

use crate::input::{KeyCode, mouse_button};
use crate::response::Response;

use super::super::UiContext;
use super::super::drawing::center_in_bounds;

struct TextInputInput {
    mouse_pressed_left: bool,
    mouse_pos_x: f32,
    key_backspace: bool,
    key_delete: bool,
    key_home: bool,
    key_end: bool,
    key_left: bool,
    key_right: bool,
    key_a: bool,
    key_c: bool,
    key_x: bool,
    key_v: bool,
    key_escape: bool,
    key_enter: bool,
    ctrl: bool,
    shift: bool,
    characters: Vec<char>,
    max_len: usize,
    font_size: f32,
}

fn snapshot_text_input_input(
    input: &crate::input::UiInputState,
    style: &crate::style::UiStyle,
) -> TextInputInput {
    TextInputInput {
        mouse_pressed_left: input.mouse_pressed[mouse_button::LEFT],
        mouse_pos_x: input.mouse_pos.x(),
        key_backspace: input.key_pressed(KeyCode::Backspace),
        key_delete: input.key_pressed(KeyCode::Delete),
        key_home: input.key_pressed(KeyCode::Home),
        key_end: input.key_pressed(KeyCode::End),
        key_left: input.key_pressed(KeyCode::ArrowLeft),
        key_right: input.key_pressed(KeyCode::ArrowRight),
        key_a: input.key_pressed(KeyCode::A),
        key_c: input.key_pressed(KeyCode::C),
        key_x: input.key_pressed(KeyCode::X),
        key_v: input.key_pressed(KeyCode::V),
        key_escape: input.key_pressed(KeyCode::Escape),
        key_enter: input.key_pressed(KeyCode::Enter),
        ctrl: input.is_key_down(KeyCode::Control),
        shift: input.is_key_down(KeyCode::Shift),
        characters: input.characters.clone(),
        max_len: style.text_input_max_length,
        font_size: style.font_size,
    }
}

fn apply_text_edits(
    text: &mut String,
    state: &mut crate::context::TextInputState,
    inp: &TextInputInput,
    clipboard: &mut Option<Box<dyn crate::widget::ClipboardProvider>>,
    multiline: bool,
) -> (bool, bool) {
    let mut changed = false;

    let delete_selection =
        |text: &mut String, state: &mut crate::context::TextInputState| -> bool {
            if state.has_selection() {
                let (start, end) = state.selection_range();
                text.drain(start..end);
                state.cursor = start;
                state.selection_anchor = start;
                true
            } else {
                false
            }
        };

    if inp.key_backspace {
        if !delete_selection(text, state) && state.cursor > 0 {
            let prev = if inp.ctrl {
                prev_word_boundary(text, state.cursor)
            } else {
                text[..state.cursor]
                    .char_indices()
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            };
            text.drain(prev..state.cursor);
            state.cursor = prev;
            state.selection_anchor = prev;
        }
        changed = true;
    }

    if inp.key_delete {
        if !delete_selection(text, state) && state.cursor < text.len() {
            let next = if inp.ctrl {
                next_word_boundary(text, state.cursor)
            } else {
                text[state.cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| state.cursor + i)
                    .unwrap_or(text.len())
            };
            text.drain(state.cursor..next);
        }
        changed = true;
    }

    if inp.key_home {
        if inp.shift {
            state.cursor = 0;
        } else {
            state.cursor = 0;
            state.selection_anchor = 0;
        }
    }

    if inp.key_end {
        let len = text.len();
        if inp.shift {
            state.cursor = len;
        } else {
            state.cursor = len;
            state.selection_anchor = len;
        }
    }

    if inp.key_left {
        if inp.ctrl {
            let new_pos = prev_word_boundary(text, state.cursor);
            if inp.shift {
                state.cursor = new_pos;
            } else {
                state.cursor = new_pos;
                state.selection_anchor = new_pos;
            }
        } else if inp.shift {
            if state.cursor > 0 {
                let prev = text[..state.cursor]
                    .char_indices()
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                state.cursor = prev;
            }
        } else if state.has_selection() {
            let (start, _) = state.selection_range();
            state.cursor = start;
            state.selection_anchor = start;
        } else if state.cursor > 0 {
            let prev = text[..state.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            state.cursor = prev;
            state.selection_anchor = prev;
        }
    }

    if inp.key_right {
        if inp.ctrl {
            let new_pos = next_word_boundary(text, state.cursor);
            if inp.shift {
                state.cursor = new_pos;
            } else {
                state.cursor = new_pos;
                state.selection_anchor = new_pos;
            }
        } else if inp.shift {
            if state.cursor < text.len() {
                let next = text[state.cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| state.cursor + i)
                    .unwrap_or(text.len());
                state.cursor = next;
            }
        } else if state.has_selection() {
            let (_, end) = state.selection_range();
            state.cursor = end;
            state.selection_anchor = end;
        } else if state.cursor < text.len() {
            let next = text[state.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| state.cursor + i)
                .unwrap_or(text.len());
            state.cursor = next;
            state.selection_anchor = next;
        }
    }

    if inp.ctrl && inp.key_a {
        state.cursor = text.len();
        state.selection_anchor = 0;
    }

    if inp.ctrl && inp.key_c && state.has_selection() {
        let (start, end) = state.selection_range();
        let copied = text[start..end].to_string();
        if let Some(cb) = clipboard {
            cb.set(&copied);
        }
    }

    if inp.ctrl && inp.key_x && state.has_selection() {
        let (start, end) = state.selection_range();
        let cut = text[start..end].to_string();
        text.drain(start..end);
        state.cursor = start;
        state.selection_anchor = start;
        changed = true;
        if let Some(cb) = clipboard {
            cb.set(&cut);
        }
    }

    if inp.ctrl
        && inp.key_v
        && let Some(cb) = clipboard
        && let Some(pasted) = cb.get()
    {
        let available = inp.max_len.saturating_sub(text.len());
        if available > 0 {
            if state.has_selection() {
                let (start, end) = state.selection_range();
                text.drain(start..end);
                state.cursor = start;
                state.selection_anchor = start;
            }
            let pasted_chars: String = pasted
                .chars()
                .filter(|c| *c >= ' ')
                .take(available)
                .collect();
            let insert_len = pasted_chars.len();
            text.insert_str(state.cursor, &pasted_chars);
            state.cursor += insert_len;
            state.selection_anchor = state.cursor;
            changed = true;
        }
    }

    if !inp.ctrl {
        for c in inp.characters.iter().copied() {
            if c >= ' ' && text.len() < inp.max_len {
                if state.has_selection() {
                    let (start, end) = state.selection_range();
                    text.drain(start..end);
                    state.cursor = start;
                    state.selection_anchor = start;
                }
                text.insert(state.cursor, c);
                let c_len = c.len_utf8();
                state.cursor += c_len;
                state.selection_anchor = state.cursor;
                changed = true;
            }
        }

        if multiline && inp.shift && inp.key_enter && text.len() < inp.max_len {
            if state.has_selection() {
                let (start, end) = state.selection_range();
                text.drain(start..end);
                state.cursor = start;
                state.selection_anchor = start;
            }
            text.insert(state.cursor, '\n');
            state.cursor += 1;
            state.selection_anchor = state.cursor;
            changed = true;
        }
    }

    (changed, inp.key_escape)
}

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
    pub(crate) fn image_button(
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

    /// Draw a slider (internal - used by declarative draw pipeline and Vec3Slider).
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

    /// Draw a text input field (internal - use `widgets::TextInput` instead).
    pub(crate) fn text_input(
        &mut self,
        id: &str,
        text: &mut String,
        bounds: Rect2D,
        placeholder: Option<&str>,
        show_clear: bool,
        multiline: bool,
    ) -> Response {
        let widget_id = self.generate_id(id);
        let hovered = self.update_hover(widget_id, bounds);
        self.register_focusable(widget_id, bounds);

        let clear_size = bounds.height();
        let clear_bounds = Rect2D::from_origin_size(
            Vec2::new(bounds.max.x() - clear_size, bounds.min.y()),
            Vec2::new(clear_size, clear_size),
        );
        let clear_hovered = show_clear && !text.is_empty() && self.input.is_hovered(clear_bounds);
        let clear_clicked = clear_hovered && self.input.mouse_pressed[mouse_button::LEFT];

        let inp = snapshot_text_input_input(&self.input, &self.style);

        // Focus on click (but not on clear button)
        if hovered && !clear_hovered && inp.mouse_pressed_left {
            self.focused_id = Some(widget_id);
        }

        // Auto-focus if requested via request_focus()
        if self.pending_focus_label.as_deref() == Some(id) {
            self.focused_id = Some(widget_id);
            self.pending_focus_label = None;
        }

        let focused = self.focused_id == Some(widget_id);
        let mut changed = false;

        // Handle clear button
        if clear_clicked {
            text.clear();
            changed = true;
            self.focused_id = Some(widget_id);
        }

        // Initialize or retrieve text input state
        if focused || clear_clicked {
            let padding = 4.0;

            // Ensure state exists and clamp to current text length
            // (text may have been modified externally since last frame)
            {
                let state = self
                    .text_input_states
                    .entry(widget_id)
                    .or_insert_with(|| super::super::TextInputState::at_end(text));
                let len = text.len();
                state.cursor = text.floor_char_boundary(state.cursor.min(len));
                state.selection_anchor = text.floor_char_boundary(state.selection_anchor.min(len));
            }

            // Handle click-to-position cursor
            if hovered && !clear_hovered && inp.mouse_pressed_left {
                let text_x = bounds.min.x() + padding;
                let scroll = self
                    .text_input_states
                    .get(&widget_id)
                    .map(|s| s.scroll_offset)
                    .unwrap_or(0.0);
                let rel_x = inp.mouse_pos_x - text_x + scroll;
                let click_pos = if rel_x <= 0.0 {
                    0
                } else {
                    let widths =
                        measure_char_widths(text, &mut |s| self.measure_text(s, inp.font_size).x());

                    let full_width = widths.last().copied().unwrap_or(0.0);
                    if (full_width - rel_x).abs() < rel_x {
                        text.len()
                    } else {
                        let chars: Vec<(usize, char)> = text.char_indices().collect();
                        let n = chars.len();
                        let mut best_offset = 0usize;
                        let mut best_dist = f32::MAX;

                        let zero_dist = rel_x.abs();
                        if zero_dist < best_dist {
                            best_dist = zero_dist;
                        }

                        let mut lo = 0usize;
                        let mut hi = n;
                        while lo < hi {
                            let mid = lo + (hi - lo) / 2;
                            let (_, _) = chars[mid];
                            let width = widths[mid + 1];
                            if width < rel_x {
                                lo = mid + 1;
                            } else {
                                hi = mid;
                            }
                        }

                        for &check in &[lo.saturating_sub(1), lo] {
                            if check < n {
                                let width = widths[check + 1];
                                let dist = (width - rel_x).abs();
                                if dist < best_dist {
                                    best_dist = dist;
                                    best_offset = chars[check].0;
                                }
                            }
                        }

                        best_offset
                    }
                };
                let state = self
                    .text_input_states
                    .get_mut(&widget_id)
                    .expect("text input state must exist after insertion");
                state.cursor = click_pos;
                state.selection_anchor = click_pos;
            }

            if clear_clicked {
                let state = self
                    .text_input_states
                    .get_mut(&widget_id)
                    .expect("text input state must exist after insertion");
                state.clear();
            }

            // Handle keyboard input when focused
            if focused {
                self.input.want_capture_keyboard = true;
                let state = self
                    .text_input_states
                    .get_mut(&widget_id)
                    .expect("text input state must exist after insertion");

                let (edit_changed, escape_pressed) =
                    apply_text_edits(text, state, &inp, &mut self.clipboard, multiline);
                if edit_changed {
                    changed = true;
                    self.last_input_time = self.time;
                }
                if escape_pressed {
                    self.focused_id = None;
                }
            }

            // Adjust scroll offset to keep cursor visible
            {
                let cursor = self
                    .text_input_states
                    .get(&widget_id)
                    .expect("text input state must exist")
                    .cursor;
                let cursor_x = self.measure_text(&text[..cursor], inp.font_size).x();
                let text_area_w = bounds.width() - padding * 2.0;
                let state = self
                    .text_input_states
                    .get_mut(&widget_id)
                    .expect("text input state must exist");
                if cursor_x - state.scroll_offset > text_area_w {
                    state.scroll_offset = cursor_x - text_area_w + padding;
                }
                if cursor_x - state.scroll_offset < 0.0 {
                    state.scroll_offset = (cursor_x - padding).max(0.0);
                }
            }
        }

        let enter_pressed = if multiline {
            focused && inp.key_enter && !inp.shift
        } else {
            focused && inp.key_enter
        };

        // Draw background
        self.draw_rounded_rect(bounds, self.style.input_bg, self.style.input_rounding);

        let border_color = if focused {
            self.style.input_border_focused
        } else if hovered {
            Color::new(
                (self.style.input_border.r + 0.1).min(1.0),
                (self.style.input_border.g + 0.1).min(1.0),
                (self.style.input_border.b + 0.1).min(1.0),
                self.style.input_border.a,
            )
        } else {
            self.style.input_border
        };
        self.draw_rounded_selection_border(bounds, border_color, 1.0, self.style.input_rounding);

        if focused {
            let focus_ring_width = self.style.focus_ring_width;
            let focus_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    bounds.min.x() - focus_ring_width,
                    bounds.min.y() - focus_ring_width,
                ),
                Vec2::new(
                    bounds.width() + focus_ring_width * 2.0,
                    bounds.height() + focus_ring_width * 2.0,
                ),
            );
            self.draw_rounded_selection_border(
                focus_bounds,
                self.style.focus_ring_color,
                focus_ring_width,
                self.style.input_rounding,
            );
        }

        let padding = 4.0;
        let text_area_width = if show_clear && !text.is_empty() {
            bounds.width() - clear_size - padding
        } else {
            bounds.width() - padding
        };
        let text_bounds =
            Rect2D::from_origin_size(bounds.min, Vec2::new(text_area_width, bounds.height()))
                .contract(padding);
        self.push_clip(text_bounds);

        let scroll_offset = self
            .text_input_states
            .get(&widget_id)
            .map(|s| s.scroll_offset)
            .unwrap_or(0.0);

        let text_size = self.measure_text(text, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + padding - scroll_offset,
            bounds.center().y() - text_size.y() * 0.5,
        );

        // Precompute widths for all char boundaries when focused
        if focused {
            let (has_selection, sel_range, cursor_byte) = {
                let state = self
                    .text_input_states
                    .get(&widget_id)
                    .expect("text input state must exist");
                (state.has_selection(), state.selection_range(), state.cursor)
            };

            let widths = measure_char_widths(text, &mut |s| {
                self.measure_text(s, self.style.font_size).x()
            });

            // Draw selection highlight
            if has_selection {
                let (sel_start, sel_end) = sel_range;
                let start_idx = char_index_for_byte(text, sel_start);
                let end_idx = char_index_for_byte(text, sel_end);
                let before_sel = widths[start_idx];
                let sel_end_x = widths[end_idx];
                let sel_width = sel_end_x - before_sel;
                let sel_rect = Rect2D::from_origin_size(
                    Vec2::new(text_pos.x() + before_sel, text_pos.y()),
                    Vec2::new(sel_width.max(1.0), text_size.y()),
                );
                self.draw_rect(sel_rect, self.style.input_selection);
            }

            // Draw text
            self.draw_text(text, text_pos, self.style.input_text, self.style.font_size);

            // Draw cursor (with blink and grace period after typing)
            let grace_period = 0.8;
            let time_since_input = self.time - self.last_input_time;
            let blink_on = self.time == 0.0
                || time_since_input < grace_period
                || ((self.time * 2.0 * std::f64::consts::PI).sin() > 0.0);
            if blink_on {
                let cursor_idx = char_index_for_byte(text, cursor_byte);
                let before_cursor = widths[cursor_idx];
                let cursor_x = text_pos.x() + before_cursor;
                self.draw_line(
                    Vec2::new(cursor_x, text_pos.y()),
                    Vec2::new(cursor_x, text_pos.y() + text_size.y()),
                    self.style.input_cursor,
                    self.style.text_input_cursor_width,
                );
            }
        } else {
            // Draw placeholder or text
            if text.is_empty() {
                if let Some(placeholder_text) = placeholder {
                    self.draw_text(
                        placeholder_text,
                        text_pos,
                        self.style.text_hint,
                        self.style.font_size,
                    );
                }
            } else {
                self.draw_text(text, text_pos, self.style.input_text, self.style.font_size);
            }
        }

        self.pop_clip();

        // Draw clear button
        if show_clear && !text.is_empty() {
            let icon_color = if clear_hovered {
                self.style.text_color
            } else {
                self.style.text_disabled
            };
            self.draw_icon_centered(
                crate::icons::ForkAwesome::TIMES,
                clear_bounds,
                clear_size * 0.6,
                icon_color,
            );
        }

        // Set text cursor when hovered
        if hovered {
            self.input.set_cursor(crate::input::MouseCursor::Text);
        }

        let mut response = Response::interactive(
            false,
            hovered,
            focused,
            bounds,
            &self.input,
            Some(widget_id),
        );
        response.changed = changed;
        response.enter_pressed = enter_pressed;
        response
    }

    /// Draw a radio button (internal - used by declarative draw pipeline).
    pub(crate) fn radio_button(
        &mut self,
        id: &str,
        value: &mut usize,
        index: usize,
        label: &str,
        bounds: Rect2D,
    ) -> Response {
        let is_selected = *value == index;
        let widget_id = self.generate_id(id);
        self.register_focusable(widget_id, bounds);
        let hovered = self.update_hover(widget_id, bounds);
        let active = self.active_id == Some(widget_id);

        let center = Vec2::new(bounds.min.x() + 10.0, bounds.center().y());
        let radius = 8.0;

        let border_color = if is_selected {
            self.style.checkbox_check
        } else if hovered {
            self.style.text_color
        } else {
            self.style.checkbox_border
        };

        // Outer ring (filled border circle + inner fill)
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_circle_auto(center, radius, border_color);
        self.draw_list
            .add_circle_auto(center, radius - 1.0, self.style.checkbox_bg);

        // Inner dot when selected
        if is_selected {
            self.draw_list.set_clip(self.clip_rect());
            self.draw_list
                .add_circle_auto(center, radius * 0.5, self.style.checkbox_check);
        }

        // Label
        let label_pos = Vec2::new(center.x() + radius + 8.0, bounds.min.y());
        self.draw_text(
            label,
            label_pos,
            self.style.text_color,
            self.style.font_size,
        );

        let clicked = self
            .click_interaction(
                widget_id,
                hovered,
                bounds,
                super::super::interaction::ClickConfig::POPUP_AWARE,
            )
            .is_clicked();

        let mut response = Response::interactive(
            clicked,
            hovered,
            active,
            bounds,
            &self.input,
            Some(widget_id),
        );
        response.changed = clicked && !is_selected;

        if response.changed {
            *value = index;
        }

        if hovered {
            self.input.set_cursor(crate::input::MouseCursor::Hand);
        }

        response
    }
}

/// Find the previous word boundary before `pos` in `text`.
fn prev_word_boundary(text: &str, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let slice = &text[..pos];
    let mut chars = slice.char_indices().rev();
    // Skip trailing whitespace
    while let Some((i, c)) = chars.next() {
        if !c.is_whitespace() {
            // Now skip the word
            let mut last = i;
            for (idx, ch) in chars.by_ref() {
                if ch.is_whitespace() {
                    return idx + ch.len_utf8();
                }
                last = idx;
            }
            return last;
        }
    }
    0
}

/// Find the next word boundary after `pos` in `text`.
fn next_word_boundary(text: &str, pos: usize) -> usize {
    if pos >= text.len() {
        return text.len();
    }
    let slice = &text[pos..];
    let mut chars = slice.char_indices();
    // Skip current whitespace
    while let Some((_i, c)) = chars.next() {
        if !c.is_whitespace() {
            // Now skip the word
            for (idx, ch) in chars.by_ref() {
                if ch.is_whitespace() {
                    return pos + idx;
                }
            }
            return text.len();
        }
    }
    text.len()
}

/// Measure cumulative widths at each char boundary in `text`.
///
/// Returns a Vec of length `chars.count() + 1` where `widths[i]` is the width
/// of `text` up to (but not including) the i-th character. `widths[0] = 0.0`
/// and `widths[n]` is the full text width.
fn measure_char_widths(text: &str, measure: &mut impl FnMut(&str) -> f32) -> Vec<f32> {
    let n = text.chars().count();
    let mut widths = Vec::with_capacity(n + 1);
    widths.push(0.0);
    let mut byte_pos = 0;
    for ch in text.chars() {
        byte_pos += ch.len_utf8();
        widths.push(measure(&text[..byte_pos]));
    }
    widths
}

/// Convert a byte offset in `text` to a char index.
/// Clamps to valid char boundaries before slicing.
fn char_index_for_byte(text: &str, byte_offset: usize) -> usize {
    let offset = byte_offset.min(text.len());
    let clamped = match text.is_char_boundary(offset) {
        true => offset,
        false => text.floor_char_boundary(offset),
    };
    text[..clamped].chars().count()
}
