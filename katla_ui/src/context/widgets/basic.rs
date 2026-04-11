//! Internal widget implementations.
//!
//! This module contains the actual rendering and interaction logic for basic widgets.
//! These are private implementation details called by the public builder widgets
//! in `crate::widgets`.

use katla_math::{Color, Rect2D, Vec2};

use crate::icons::ForkAwesome;
use crate::input::{KeyCode, mouse_button};
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
        self.draw_rect(bounds, bg_color);

        // Draw border if specified
        if let Some(border_color) = border_color {
            self.draw_selection_border(bounds, border_color, 1.0);
        }

        // Draw button text
        let text_size = self.measure_text(text, self.style.font_size);
        let text_pos = center_in_bounds(bounds, text_size);
        self.draw_text(text, text_pos, self.style.text_color, self.style.font_size);

        Response::interactive(clicked, hovered, active, bounds, &self.input)
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
        self.draw_rect(bounds, bg_color);

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

        let mut response = Response::interactive(clicked, hovered, active, bounds, &self.input);
        if !enabled {
            response.clicked = false;
            response.changed = false;
            response.double_clicked = false;
        }
        response
    }

    /// Draw a checkbox (internal - use `widgets::Checkbox` instead).
    pub(crate) fn checkbox(
        &mut self,
        id: &str,
        label: &str,
        checked: &mut bool,
        bounds: Rect2D,
    ) -> Response {
        let widget_id = self.generate_id(id);
        self.register_focusable(widget_id, bounds);

        let hovered = self.update_hover(widget_id, bounds);
        let click_result = self.click_interaction(
            widget_id,
            hovered,
            bounds,
            super::super::interaction::ClickConfig::POPUP_AWARE,
        );
        let clicked = click_result.is_clicked();
        let active = click_result.is_active();

        if clicked {
            *checked = !*checked;
        }

        // Draw checkbox background
        let check_size = bounds.height().min(20.0);
        let check_bounds = Rect2D::from_origin_size(
            Vec2::new(bounds.min.x(), bounds.center().y() - check_size * 0.5),
            Vec2::new(check_size, check_size),
        );

        let bg_color = if *checked {
            self.style.checkbox_check
        } else {
            self.style.checkbox_bg
        };
        self.draw_rect_border(check_bounds, bg_color, self.style.checkbox_border, 1.0);

        // Draw check icon if checked
        if *checked {
            let icon_size = check_size * 0.7;
            self.draw_icon_centered(ForkAwesome::CHECK, check_bounds, icon_size, Color::WHITE);
        }

        // Draw label
        let text_size = self.measure_text(label, self.style.font_size);
        let label_pos = Vec2::new(
            check_bounds.max.x() + self.style.item_inner_spacing,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(
            label,
            label_pos,
            self.style.text_color,
            self.style.font_size,
        );

        Response::interactive(clicked, hovered, active, bounds, &self.input)
    }

    /// Draw a slider (internal - use `widgets::Slider` instead).
    pub(crate) fn slider(
        &mut self,
        id: &str,
        value: &mut f32,
        min: f32,
        max: f32,
        bounds: Rect2D,
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
        let track_height = 4.0;
        let track_bounds = Rect2D::from_center_size(
            Vec2::new(bounds.center().x(), bounds.center().y()),
            Vec2::new(bounds.width(), track_height),
        );
        self.draw_rect(track_bounds, self.style.slider_track);

        // Draw grab
        let t = (*value - min) / (max - min);
        let grab_size = 12.0;
        let grab_pos = bounds.min.x() + t * (bounds.width() - grab_size);
        let grab_bounds = Rect2D::from_origin_size(
            Vec2::new(grab_pos, bounds.center().y() - grab_size * 0.5),
            Vec2::new(grab_size, grab_size),
        );
        let grab_color = if active {
            self.style.slider_grab_active
        } else if hovered {
            self.style.slider_grab_hovered
        } else {
            self.style.slider_grab
        };
        self.draw_rect(grab_bounds, grab_color);

        let mut response = Response::interactive(false, hovered, active, bounds, &self.input);
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

        // Snapshot input flags before any mutable borrows
        let mouse_pressed_left = self.input.mouse_pressed[mouse_button::LEFT];
        let mouse_pos_x = self.input.mouse_pos.x();
        let key_backspace = self.input.key_pressed(KeyCode::Backspace);
        let key_delete = self.input.key_pressed(KeyCode::Delete);
        let key_home = self.input.key_pressed(KeyCode::Home);
        let key_end = self.input.key_pressed(KeyCode::End);
        let key_left = self.input.key_pressed(KeyCode::ArrowLeft);
        let key_right = self.input.key_pressed(KeyCode::ArrowRight);
        let key_a = self.input.key_pressed(KeyCode::A);
        let key_c = self.input.key_pressed(KeyCode::C);
        let key_x = self.input.key_pressed(KeyCode::X);
        let key_v = self.input.key_pressed(KeyCode::V);
        let key_escape = self.input.key_pressed(KeyCode::Escape);
        let key_enter = self.input.key_pressed(KeyCode::Enter);
        let ctrl = self.input.is_key_down(KeyCode::Control);
        let shift = self.input.is_key_down(KeyCode::Shift);
        let characters: Vec<char> = self.input.characters.clone();
        let max_len = self.style.text_input_max_length;
        let font_size = self.style.font_size;

        // Focus on click (but not on clear button)
        if hovered && !clear_hovered && mouse_pressed_left {
            self.focused_id = Some(widget_id);
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
                state.cursor = state.cursor.min(len);
                state.selection_anchor = state.selection_anchor.min(len);
            }

            // Handle click-to-position cursor
            if hovered && !clear_hovered && mouse_pressed_left {
                let text_x = bounds.min.x() + padding;
                let rel_x = mouse_pos_x - text_x;
                let click_pos = if rel_x <= 0.0 {
                    0
                } else {
                    let mut best_offset = text.len();
                    let mut best_dist = f32::MAX;
                    for (i, _) in text.char_indices() {
                        let prefix_width = self.measure_text(&text[..i], font_size).x();
                        let dist = (prefix_width - rel_x).abs();
                        if dist < best_dist {
                            best_dist = dist;
                            best_offset = i;
                        }
                    }
                    let full_width = self.measure_text(text, font_size).x();
                    if (full_width - rel_x).abs() < best_dist {
                        best_offset = text.len();
                    }
                    best_offset
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

                // Helper: delete selection
                let delete_selection =
                    |text: &mut String, state: &mut super::super::TextInputState| -> bool {
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

                if key_backspace {
                    if !delete_selection(text, state) && state.cursor > 0 {
                        let prev = if ctrl {
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

                if key_delete {
                    if !delete_selection(text, state) && state.cursor < text.len() {
                        let next = if ctrl {
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

                if key_home {
                    if shift {
                        state.cursor = 0;
                    } else {
                        state.cursor = 0;
                        state.selection_anchor = 0;
                    }
                }

                if key_end {
                    let len = text.len();
                    if shift {
                        state.cursor = len;
                    } else {
                        state.cursor = len;
                        state.selection_anchor = len;
                    }
                }

                if key_left {
                    if ctrl {
                        let new_pos = prev_word_boundary(text, state.cursor);
                        if shift {
                            state.cursor = new_pos;
                        } else {
                            state.cursor = new_pos;
                            state.selection_anchor = new_pos;
                        }
                    } else if shift {
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

                if key_right {
                    if ctrl {
                        let new_pos = next_word_boundary(text, state.cursor);
                        if shift {
                            state.cursor = new_pos;
                        } else {
                            state.cursor = new_pos;
                            state.selection_anchor = new_pos;
                        }
                    } else if shift {
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

                // Ctrl+A: select all
                if ctrl && key_a {
                    state.cursor = text.len();
                    state.selection_anchor = 0;
                }

                // Ctrl+C: copy
                if ctrl && key_c && state.has_selection() {
                    let (start, end) = state.selection_range();
                    let copied = text[start..end].to_string();
                    if let Some(ref mut cb) = self.clipboard {
                        cb.set(&copied);
                    }
                }

                // Ctrl+X: cut
                if ctrl && key_x && state.has_selection() {
                    let (start, end) = state.selection_range();
                    let cut = text[start..end].to_string();
                    text.drain(start..end);
                    state.cursor = start;
                    state.selection_anchor = start;
                    changed = true;
                    if let Some(ref mut cb) = self.clipboard {
                        cb.set(&cut);
                    }
                }

                // Ctrl+V: paste
                if ctrl && key_v {
                    if let Some(ref mut cb) = self.clipboard {
                        if let Some(pasted) = cb.get() {
                            let available = max_len.saturating_sub(text.len());
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
                    }
                }

                // Character input (only when Ctrl is NOT held)
                if !ctrl {
                    for c in characters {
                        if c >= ' ' && text.len() < max_len {
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

                    // Multiline: Shift+Enter inserts a newline
                    if multiline && shift && key_enter && text.len() < max_len {
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

                if changed {
                    self.last_input_time = self.time;
                }

                if key_escape {
                    self.focused_id = None;
                }
            }
        }

        let enter_pressed = if multiline {
            focused && key_enter && !shift
        } else {
            focused && key_enter
        };

        // Draw background
        self.draw_rect(bounds, self.style.input_bg);

        let border_color = if focused {
            self.style.input_border_focused
        } else {
            self.style.input_border
        };
        self.draw_rect_border(bounds, Color::TRANSPARENT, border_color, 1.0);

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

        let text_size = self.measure_text(text, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + padding,
            bounds.center().y() - text_size.y() * 0.5,
        );

        // Draw selection highlight when focused
        if focused {
            if let Some(state) = self.text_input_states.get(&widget_id) {
                if state.has_selection() {
                    let (sel_start, sel_end) = state.selection_range();
                    let before_sel = self
                        .measure_text(&text[..sel_start], self.style.font_size)
                        .x();
                    let sel_width = self
                        .measure_text(&text[sel_start..sel_end], self.style.font_size)
                        .x();
                    let sel_rect = Rect2D::from_origin_size(
                        Vec2::new(text_pos.x() + before_sel, text_pos.y()),
                        Vec2::new(sel_width.max(1.0), text_size.y()),
                    );
                    self.draw_rect(sel_rect, self.style.input_selection);
                }
            }
        }

        // Draw placeholder or text
        if text.is_empty() && !focused {
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

        self.pop_clip();

        // Draw cursor when focused (with blink and grace period after typing)
        if focused {
            let grace_period = 0.8;
            let time_since_input = self.time - self.last_input_time;
            let blink_on = self.time == 0.0
                || time_since_input < grace_period
                || ((self.time * 2.0 * std::f64::consts::PI).sin() > 0.0);
            if blink_on {
                let cursor_byte = self
                    .text_input_states
                    .get(&widget_id)
                    .map(|s| s.cursor)
                    .unwrap_or(text.len());
                let before_cursor = self
                    .measure_text(&text[..cursor_byte], self.style.font_size)
                    .x();
                let cursor_x = text_pos.x() + before_cursor;
                self.draw_line(
                    Vec2::new(cursor_x, text_pos.y()),
                    Vec2::new(cursor_x, text_pos.y() + text_size.y()),
                    self.style.input_cursor,
                    self.style.text_input_cursor_width,
                );
            }
        }

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

        let mut response = Response::interactive(false, hovered, focused, bounds, &self.input);
        response.changed = changed;
        response.enter_pressed = enter_pressed;
        response
    }

    /// Draw a radio button (internal - use `widgets::RadioButton` instead).
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
            .add_circle_auto(center, radius - 1.0, Color::TRANSPARENT);

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

        let mut response = Response::interactive(clicked, hovered, active, bounds, &self.input);
        response.changed = clicked && !is_selected;

        if response.changed {
            *value = index;
        }

        response
    }

    /// Draw a combo box (internal - use `widgets::ComboBox` instead).
    ///
    /// Shows the currently selected option. When clicked, opens a dropdown popup
    /// listing all options. Clicking an option selects it and closes the popup.
    /// Clicking outside closes without changing the selection.
    pub(crate) fn combo_box(
        &mut self,
        id: &str,
        selected: &mut usize,
        options: &[&str],
        bounds: Rect2D,
        open: &mut bool,
    ) -> Response {
        let widget_id = self.generate_id(id);
        self.register_focusable(widget_id, bounds);

        let hovered = self.update_hover(widget_id, bounds);
        let clicked = self
            .click_interaction(
                widget_id,
                hovered,
                bounds,
                super::super::interaction::ClickConfig::POPUP_BYPASS,
            )
            .is_clicked();

        if clicked && !*open {
            *open = true;
        }

        // Draw trigger button
        let bg_color = if *open || hovered {
            self.style.combo_hovered
        } else {
            self.style.combo_bg
        };
        self.draw_rect(bounds, bg_color);
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.combo_border, 1.0);

        // Draw selected text
        let padding = self.style.text_input_padding;
        let text = options.get(*selected).copied().unwrap_or("");
        let text_size = self.measure_text(text, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + padding,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(text, text_pos, self.style.combo_text, self.style.font_size);

        // Draw dropdown arrow on the right side
        let arrow_size = self.style.font_size * 0.6;
        let arrow_bounds = Rect2D::from_origin_size(
            Vec2::new(bounds.max.x() - arrow_size - padding, bounds.min.y()),
            Vec2::new(arrow_size + padding, bounds.height()),
        );
        let arrow_char = if *open { '▲' } else { '▼' };
        let arrow_text_size = self.measure_text(&arrow_char.to_string(), arrow_size);
        let arrow_pos = center_in_bounds(arrow_bounds, arrow_text_size);
        self.draw_text(
            &arrow_char.to_string(),
            arrow_pos,
            self.style.combo_text,
            arrow_size,
        );

        let mut response = Response::interactive(clicked, hovered, false, bounds, &self.input);

        // Draw dropdown popup when open
        if *open {
            let item_height = self.style.combo_default_height;
            let popup_height = (options.len() as f32) * item_height;
            let popup_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), bounds.max.y()),
                Vec2::new(bounds.width(), popup_height),
            );

            self.dropdown(id, bounds, open, |ui, open_state| {
                for (i, option) in options.iter().enumerate() {
                    let item_bounds = Rect2D::from_origin_size(
                        ui.popup_cursor,
                        Vec2::new(popup_bounds.width(), item_height),
                    );
                    let item_hovered = ui.is_hovered(item_bounds);
                    let is_selected = *selected == i;

                    let bg = if item_hovered {
                        ui.style.combo_hovered
                    } else if is_selected {
                        ui.style.selectable_selected
                    } else {
                        ui.style.combo_bg
                    };
                    ui.draw_rect(item_bounds, bg);

                    let opt_text_size = ui.measure_text(option, ui.style.font_size);
                    let opt_text_pos = Vec2::new(
                        item_bounds.min.x() + padding,
                        item_bounds.center().y() - opt_text_size.y() * 0.5,
                    );
                    ui.draw_text(
                        option,
                        opt_text_pos,
                        ui.style.combo_text,
                        ui.style.font_size,
                    );

                    ui.track_popup_item(item_bounds);

                    if item_hovered && ui.input.mouse_pressed[mouse_button::LEFT] {
                        *selected = i;
                        response.changed = true;
                        *open_state = false;
                        ui.input.want_capture_mouse = true;
                    }

                    ui.popup_cursor =
                        Vec2::new(ui.popup_cursor.x(), ui.popup_cursor.y() + item_height);
                }
            });

            if !*open {
                response.clicked = true;
            }
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
