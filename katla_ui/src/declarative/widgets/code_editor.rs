use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use katla_math::{Color, Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;
use crate::input::{KeyCode, mouse_button};

use super::super::animation::AnimationState;
use super::super::descriptor::Callback;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};

// ---------------------------------------------------------------------------
// Selection state
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct SelectionState {
    anchor_line: usize,
    anchor_col: usize,
    cursor_line: usize,
    cursor_col: usize,
}

impl SelectionState {
    fn sorted(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor_line < self.cursor_line
            || (self.anchor_line == self.cursor_line && self.anchor_col <= self.cursor_col)
        {
            (
                (self.anchor_line, self.anchor_col),
                (self.cursor_line, self.cursor_col),
            )
        } else {
            (
                (self.cursor_line, self.cursor_col),
                (self.anchor_line, self.anchor_col),
            )
        }
    }

    fn text<'a>(&self, lines: &'a [String]) -> String {
        let ((sl, sc), (el, ec)) = self.sorted();
        if sl == el {
            if sc < lines[sl].len() && ec <= lines[sl].len() {
                lines[sl][sc..ec].to_string()
            } else {
                String::new()
            }
        } else {
            let mut result = String::new();
            if sc < lines[sl].len() {
                result.push_str(&lines[sl][sc..]);
            }
            for i in sl + 1..el {
                result.push('\n');
                result.push_str(&lines[i]);
            }
            result.push('\n');
            if ec > 0 && ec <= lines[el].len() {
                result.push_str(&lines[el][..ec]);
            }
            result
        }
    }
}

// ---------------------------------------------------------------------------
// Undo/Redo changes
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum EditorChange {
    Insert {
        line: usize,
        col: usize,
        text: String,
    },
    Delete {
        line: usize,
        col: usize,
        text: String,
    },
}

impl EditorChange {
    fn reverse(&self) -> EditorChange {
        match self {
            EditorChange::Insert { line, col, text } => EditorChange::Delete {
                line: *line,
                col: *col,
                text: text.clone(),
            },
            EditorChange::Delete { line, col, text } => EditorChange::Insert {
                line: *line,
                col: *col,
                text: text.clone(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Editor state (persists across frames)
// ---------------------------------------------------------------------------

struct EditorStateInner {
    lines: Vec<String>,
    cursor_line: usize,
    cursor_col: usize,
    selection: Option<SelectionState>,
    scroll_y: f32,
    undo_stack: Vec<Vec<EditorChange>>,
    redo_stack: Vec<Vec<EditorChange>>,
    preferred_x: usize,
    tab_width: usize,
    font_size: f32,
    line_height: f32,
    clipboard_text: String,
    pending_paste: bool,
    last_click_time: f64,
    last_click_pos: (f32, f32),
    click_count: u32,
    is_dragging: bool,
}

impl EditorStateInner {
    fn new(text: &str) -> Self {
        let lines: Vec<String> = if text.is_empty() {
            vec![String::new()]
        } else {
            text.split('\n').map(|s| s.to_string()).collect()
        };

        Self {
            lines,
            cursor_line: 0,
            cursor_col: 0,
            selection: None,
            scroll_y: 0.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            preferred_x: 0,
            tab_width: 4,
            font_size: 14.0,
            line_height: 14.0 * 1.2,
            clipboard_text: String::new(),
            pending_paste: false,
            last_click_time: 0.0,
            last_click_pos: (0.0, 0.0),
            click_count: 0,
            is_dragging: false,
        }
    }

    fn text(&self) -> String {
        self.lines.join("\n")
    }

    fn line_count(&self) -> usize {
        self.lines.len()
    }

    fn clamp_cursor(&mut self) {
        if self.cursor_line >= self.lines.len() {
            self.cursor_line = self.lines.len().saturating_sub(1);
        }
        let max_col = self.lines[self.cursor_line].len();
        if self.cursor_col > max_col {
            self.cursor_col = max_col;
        }
    }

    // --- Editing ---

    fn insert_char(&mut self, c: char, changes: &mut Vec<EditorChange>) {
        self.delete_selection_into(changes);

        let line = &mut self.lines[self.cursor_line];
        line.insert(self.cursor_col, c);
        changes.push(EditorChange::Insert {
            line: self.cursor_line,
            col: self.cursor_col,
            text: c.to_string(),
        });
        self.cursor_col += c.len_utf8();
        self.preferred_x = self.cursor_col;
    }

    fn insert_newline(&mut self, changes: &mut Vec<EditorChange>) {
        self.delete_selection_into(changes);

        let line = &mut self.lines[self.cursor_line];
        let after: String = line[self.cursor_col..].to_string();
        line.truncate(self.cursor_col);

        changes.push(EditorChange::Insert {
            line: self.cursor_line,
            col: self.cursor_col,
            text: "\n".to_string(),
        });

        self.cursor_line += 1;
        self.cursor_col = 0;
        self.lines.insert(self.cursor_line, after);
        self.preferred_x = 0;
    }

    fn insert_tab(&mut self, changes: &mut Vec<EditorChange>) {
        self.delete_selection_into(changes);

        let spaces: String = " ".repeat(self.tab_width);
        let line = &mut self.lines[self.cursor_line];
        changes.push(EditorChange::Insert {
            line: self.cursor_line,
            col: self.cursor_col,
            text: spaces.clone(),
        });
        line.insert_str(self.cursor_col, &spaces);
        self.cursor_col += self.tab_width;
        self.preferred_x = self.cursor_col;
    }

    fn backspace(&mut self, changes: &mut Vec<EditorChange>) {
        if self.delete_selection_into(changes) {
            return;
        }

        if self.cursor_col > 0 {
            let line = &mut self.lines[self.cursor_line];
            let char_start = line[..self.cursor_col]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            let deleted = line[char_start..self.cursor_col].to_string();
            line.drain(char_start..self.cursor_col);
            changes.push(EditorChange::Delete {
                line: self.cursor_line,
                col: char_start,
                text: deleted,
            });
            self.cursor_col = char_start;
        } else if self.cursor_line > 0 {
            let prev_len = self.lines[self.cursor_line - 1].len();
            let current = self.lines.remove(self.cursor_line);
            self.lines[self.cursor_line - 1].push_str(&current);
            self.cursor_line -= 1;
            self.cursor_col = prev_len;
            changes.push(EditorChange::Delete {
                line: self.cursor_line,
                col: prev_len,
                text: "\n".to_string(),
            });
        }
        self.preferred_x = self.cursor_col;
    }

    fn delete_forward(&mut self, changes: &mut Vec<EditorChange>) {
        if self.delete_selection_into(changes) {
            return;
        }

        let line_len = self.lines[self.cursor_line].len();
        if self.cursor_col < line_len {
            let line = &mut self.lines[self.cursor_line];
            let char_end = line[self.cursor_col..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor_col + i)
                .unwrap_or(line_len);
            let deleted = line[self.cursor_col..char_end].to_string();
            line.drain(self.cursor_col..char_end);
            changes.push(EditorChange::Delete {
                line: self.cursor_line,
                col: self.cursor_col,
                text: deleted,
            });
        } else if self.cursor_line < self.lines.len() - 1 {
            let next = self.lines.remove(self.cursor_line + 1);
            self.lines[self.cursor_line].push_str(&next);
            changes.push(EditorChange::Delete {
                line: self.cursor_line,
                col: self.cursor_col,
                text: "\n".to_string(),
            });
        }
    }

    fn delete_selection_into(&mut self, changes: &mut Vec<EditorChange>) -> bool {
        let Some(sel) = self.selection.take() else {
            return false;
        };
        let ((sl, sc), (el, ec)) = sel.sorted();

        if sl == el {
            let line = &mut self.lines[sl];
            if sc < ec && ec <= line.len() {
                let deleted = line[sc..ec].to_string();
                line.drain(sc..ec);
                changes.push(EditorChange::Delete {
                    line: sl,
                    col: sc,
                    text: deleted,
                });
            }
        } else {
            let mut deleted = String::new();

            // First line: truncate at selection start
            let first_line = &mut self.lines[sl];
            if sc < first_line.len() {
                deleted.push_str(&first_line[sc..]);
            }
            first_line.truncate(sc);

            // Collect middle and end line content
            let mut tail = String::new();
            if el < self.lines.len() && ec <= self.lines[el].len() {
                tail = self.lines[el][ec..].to_string();
                deleted.push('\n');
                if ec > 0 {
                    deleted.push_str(&self.lines[el][..ec]);
                }
            }

            // Remove lines from el down to sl+1
            for idx in (sl + 1..=el).rev() {
                if idx < self.lines.len() {
                    if idx > sl + 1 {
                        deleted.push('\n');
                        deleted.push_str(&self.lines[idx]);
                    }
                    self.lines.remove(idx);
                }
            }

            // Append remaining tail to first line
            self.lines[sl].push_str(&tail);

            changes.push(EditorChange::Delete {
                line: sl,
                col: sc,
                text: deleted,
            });
        }

        self.cursor_line = sl;
        self.cursor_col = sc;
        self.preferred_x = self.cursor_col;
        true
    }

    fn commit_change(&mut self, changes: Vec<EditorChange>) {
        if !changes.is_empty() {
            self.undo_stack.push(changes);
            self.redo_stack.clear();
        }
    }

    // --- Cursor movement ---

    fn move_left(&mut self) {
        self.selection = None;
        if self.cursor_col > 0 {
            self.cursor_col = self.lines[self.cursor_line][..self.cursor_col]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        } else if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.lines[self.cursor_line].len();
        }
        self.preferred_x = self.cursor_col;
    }

    fn move_right(&mut self) {
        self.selection = None;
        let line = &self.lines[self.cursor_line];
        if self.cursor_col < line.len() {
            self.cursor_col = line[self.cursor_col..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor_col + i)
                .unwrap_or(line.len());
        } else if self.cursor_line < self.lines.len() - 1 {
            self.cursor_line += 1;
            self.cursor_col = 0;
        }
        self.preferred_x = self.cursor_col;
    }

    fn move_up(&mut self) {
        self.selection = None;
        if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.preferred_x.min(self.lines[self.cursor_line].len());
        }
    }

    fn move_down(&mut self) {
        self.selection = None;
        if self.cursor_line < self.lines.len() - 1 {
            self.cursor_line += 1;
            self.cursor_col = self.preferred_x.min(self.lines[self.cursor_line].len());
        }
    }

    fn move_home(&mut self) {
        self.selection = None;
        self.cursor_col = 0;
        self.preferred_x = 0;
    }

    fn move_end(&mut self) {
        self.selection = None;
        self.cursor_col = self.lines[self.cursor_line].len();
        self.preferred_x = self.cursor_col;
    }

    fn move_ctrl_home(&mut self) {
        self.selection = None;
        self.cursor_line = 0;
        self.cursor_col = 0;
        self.preferred_x = 0;
    }

    fn move_ctrl_end(&mut self) {
        self.selection = None;
        self.cursor_line = self.lines.len() - 1;
        self.cursor_col = self.lines[self.cursor_line].len();
        self.preferred_x = self.cursor_col;
    }

    // --- Selection operations ---

    fn start_selection(&mut self) {
        if self.selection.is_none() {
            self.selection = Some(SelectionState {
                anchor_line: self.cursor_line,
                anchor_col: self.cursor_col,
                cursor_line: self.cursor_line,
                cursor_col: self.cursor_col,
            });
        }
    }

    fn extend_selection_left(&mut self) {
        self.start_selection();
        if self.cursor_col > 0 {
            self.cursor_col = self.lines[self.cursor_line][..self.cursor_col]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        } else if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.lines[self.cursor_line].len();
        }
        self.update_selection_cursor();
    }

    fn extend_selection_right(&mut self) {
        self.start_selection();
        let line = &self.lines[self.cursor_line];
        if self.cursor_col < line.len() {
            self.cursor_col = line[self.cursor_col..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor_col + i)
                .unwrap_or(line.len());
        } else if self.cursor_line < self.lines.len() - 1 {
            self.cursor_line += 1;
            self.cursor_col = 0;
        }
        self.update_selection_cursor();
    }

    fn extend_selection_up(&mut self) {
        self.start_selection();
        if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.preferred_x.min(self.lines[self.cursor_line].len());
        }
        self.update_selection_cursor();
    }

    fn extend_selection_down(&mut self) {
        self.start_selection();
        if self.cursor_line < self.lines.len() - 1 {
            self.cursor_line += 1;
            self.cursor_col = self.preferred_x.min(self.lines[self.cursor_line].len());
        }
        self.update_selection_cursor();
    }

    fn update_selection_cursor(&mut self) {
        if let Some(ref mut sel) = self.selection {
            sel.cursor_line = self.cursor_line;
            sel.cursor_col = self.cursor_col;
        }
    }

    fn select_all(&mut self) {
        let last_line = self.lines.len() - 1;
        let last_col = self.lines[last_line].len();
        self.selection = Some(SelectionState {
            anchor_line: 0,
            anchor_col: 0,
            cursor_line: last_line,
            cursor_col: last_col,
        });
        self.cursor_line = last_line;
        self.cursor_col = last_col;
    }

    fn select_word(&mut self) {
        let line = &self.lines[self.cursor_line];
        let col = self.cursor_col.min(line.len());

        let start = line[..col]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = line[col..]
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| col + i)
            .unwrap_or(line.len());

        self.selection = Some(SelectionState {
            anchor_line: self.cursor_line,
            anchor_col: start,
            cursor_line: self.cursor_line,
            cursor_col: end,
        });
        self.cursor_col = end;
    }

    fn select_line(&mut self) {
        let line_len = self.lines[self.cursor_line].len();
        self.selection = Some(SelectionState {
            anchor_line: self.cursor_line,
            anchor_col: 0,
            cursor_line: self.cursor_line,
            cursor_col: line_len,
        });
        self.cursor_col = line_len;
    }

    fn clear_selection(&mut self) {
        self.selection = None;
    }

    // --- Clipboard ---

    fn copy(&mut self) -> Option<String> {
        if let Some(ref sel) = self.selection {
            let text = sel.text(&self.lines);
            self.clipboard_text = text.clone();
            Some(text)
        } else {
            None
        }
    }

    fn cut(&mut self) -> Option<String> {
        let copied = self.copy();
        if copied.is_some() {
            let mut changes = Vec::new();
            self.delete_selection_into(&mut changes);
            self.commit_change(changes);
        }
        copied
    }

    fn paste(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut changes = Vec::new();
        self.delete_selection_into(&mut changes);

        let paste_lines: Vec<&str> = text.split('\n').collect();
        if paste_lines.len() == 1 {
            self.lines[self.cursor_line].insert_str(self.cursor_col, paste_lines[0]);
            changes.push(EditorChange::Insert {
                line: self.cursor_line,
                col: self.cursor_col,
                text: text.to_string(),
            });
            self.cursor_col += paste_lines[0].len();
        } else {
            let current_line = &mut self.lines[self.cursor_line];
            let after: String = current_line[self.cursor_col..].to_string();
            current_line.truncate(self.cursor_col);
            current_line.push_str(paste_lines[0]);

            let num_paste = paste_lines.len();
            for (i, (&line_text, insert_at)) in paste_lines[1..]
                .iter()
                .zip(self.cursor_line + 1..)
                .enumerate()
            {
                if i == num_paste - 2 {
                    let mut full = line_text.to_string();
                    full.push_str(&after);
                    self.lines.insert(insert_at, full);
                } else {
                    self.lines.insert(insert_at, line_text.to_string());
                }
            }

            changes.push(EditorChange::Insert {
                line: self.cursor_line,
                col: self.cursor_col,
                text: text.to_string(),
            });

            self.cursor_line += paste_lines.len() - 1;
            self.cursor_col = paste_lines.last().unwrap().len();
        }

        self.preferred_x = self.cursor_col;
        self.commit_change(changes);
    }

    // --- Undo/Redo ---

    fn undo(&mut self) {
        let Some(changes) = self.undo_stack.pop() else {
            return;
        };

        let redo_changes: Vec<EditorChange> = changes
            .iter()
            .rev()
            .map(|c| {
                let reversed = c.reverse();
                self.apply_change_inner(&reversed);
                c.clone()
            })
            .collect();

        self.redo_stack.push(redo_changes);
    }

    fn redo(&mut self) {
        let Some(changes) = self.redo_stack.pop() else {
            return;
        };

        let undo_changes: Vec<EditorChange> = changes
            .iter()
            .rev()
            .map(|c| {
                self.apply_change_inner(c);
                c.reverse()
            })
            .collect();

        self.undo_stack.push(undo_changes);
    }

    fn apply_change_inner(&mut self, change: &EditorChange) {
        match change {
            EditorChange::Insert { line, col, text } => {
                if *line < self.lines.len() {
                    if text.contains('\n') {
                        // Multi-line insert
                        let parts: Vec<&str> = text.split('\n').collect();
                        let current_line = &mut self.lines[*line];
                        let after: String = current_line[*col..].to_string();
                        current_line.truncate(*col);
                        current_line.push_str(parts[0]);

                        let num_parts = parts.len();
                        for (i, (&part, insert_at)) in
                            parts[1..].iter().zip(*line + 1..).enumerate()
                        {
                            if i == num_parts - 2 {
                                let mut full = part.to_string();
                                full.push_str(&after);
                                self.lines.insert(insert_at, full);
                            } else {
                                self.lines.insert(insert_at, part.to_string());
                            }
                        }

                        self.cursor_line = *line + parts.len() - 1;
                        self.cursor_col = parts.last().unwrap().len();
                    } else {
                        self.lines[*line].insert_str(*col, text);
                        self.cursor_line = *line;
                        self.cursor_col = col + text.len();
                    }
                }
            }
            EditorChange::Delete { line, col, text } => {
                if *line < self.lines.len() {
                    if text.contains('\n') {
                        // Multi-line delete
                        let after = self.lines.remove(*line + 1);
                        self.lines[*line].push_str(&after);
                        self.cursor_line = *line;
                        self.cursor_col = *col;
                    } else {
                        let end = (*col + text.len()).min(self.lines[*line].len());
                        self.lines[*line].drain(*col..end);
                        self.cursor_line = *line;
                        self.cursor_col = *col;
                    }
                }
            }
        }
        self.preferred_x = self.cursor_col;
    }

    // --- Click positioning ---

    fn click_to_position(&mut self, x: f32, y: f32, bounds: Rect2D, gutter_width: f32) {
        let text_x = bounds.min.x() + gutter_width;
        let text_y = bounds.min.y();

        let line = ((y - text_y + self.scroll_y) / self.line_height).floor() as usize;
        self.cursor_line = line.min(self.lines.len().saturating_sub(1));

        // Approximate column from x position
        let char_width = self.font_size * 0.6;
        let col =
            (((x - text_x) / char_width).floor() as usize).min(self.lines[self.cursor_line].len());
        self.cursor_col = col;
        self.preferred_x = self.cursor_col;
    }

    // --- Scrolling ---

    fn scroll(&mut self, delta: f32, viewport_height: f32) {
        self.scroll_y += delta;
        self.clamp_scroll(viewport_height);
    }

    fn clamp_scroll(&mut self, viewport_height: f32) {
        let content_height = self.lines.len() as f32 * self.line_height;
        let max_scroll = (content_height - viewport_height).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
    }

    fn scroll_cursor_into_view(&mut self, viewport_height: f32) {
        let cursor_y = self.cursor_line as f32 * self.line_height;
        if cursor_y < self.scroll_y {
            self.scroll_y = cursor_y;
        } else if cursor_y + self.line_height > self.scroll_y + viewport_height {
            self.scroll_y = cursor_y + self.line_height - viewport_height;
        }
        self.clamp_scroll(viewport_height);
    }

    fn gutter_width(&self) -> f32 {
        let line_count = self.lines.len();
        let digits = if line_count < 10 {
            1
        } else if line_count < 100 {
            2
        } else if line_count < 1000 {
            3
        } else if line_count < 10000 {
            4
        } else {
            5
        };
        let char_width = self.font_size * 0.6;
        char_width * (digits as f32 + 2.0)
    }
}

// ---------------------------------------------------------------------------
// Shared state wrapper for StateArena
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct SharedEditorState(pub(crate) Rc<RefCell<EditorStateInner>>);

impl PartialEq for SharedEditorState {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

// ---------------------------------------------------------------------------
// CodeEditor widget
// ---------------------------------------------------------------------------

pub struct CodeEditor {
    pub state_id: StateId,
}

impl Widget for CodeEditor {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Self>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        Style {
            size: Size {
                width: Dimension::Length(400.0),
                height: Dimension::Length(300.0),
            },
            flex_grow: 1.0,
            ..Style::default()
        }
    }

    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        state: &mut StateArena,
        bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        let Some(shared) = state.get::<SharedEditorState>(self.state_id) else {
            return InputResult::Ignore;
        };
        let mut inner = shared.0.borrow_mut();

        let is_focused = ctx.focused_id == Some(ctx.view_id);
        let in_bounds = bounds.contains(ctx.mouse_pos);

        // Click handling
        if ctx.input.mouse_pressed[mouse_button::LEFT] && in_bounds {
            let gutter_width = inner.gutter_width();

            // Detect click count
            let now = ctx.input.last_click_time[mouse_button::LEFT];
            let dx = ctx.mouse_pos.x() - inner.last_click_pos.0;
            let dy = ctx.mouse_pos.y() - inner.last_click_pos.1;
            let dist = (dx * dx + dy * dy).sqrt();

            if now - inner.last_click_time < 0.5 && dist < 5.0 {
                inner.click_count = inner.click_count.saturating_add(1).min(3);
            } else {
                inner.click_count = 1;
            }
            inner.last_click_time = now;
            inner.last_click_pos = (ctx.mouse_pos.x(), ctx.mouse_pos.y());

            match inner.click_count {
                1 => {
                    inner.click_to_position(
                        ctx.mouse_pos.x(),
                        ctx.mouse_pos.y(),
                        bounds,
                        gutter_width,
                    );
                    inner.clear_selection();
                    inner.is_dragging = true;
                }
                2 => {
                    inner.click_to_position(
                        ctx.mouse_pos.x(),
                        ctx.mouse_pos.y(),
                        bounds,
                        gutter_width,
                    );
                    inner.select_word();
                }
                3 => {
                    inner.click_to_position(
                        ctx.mouse_pos.x(),
                        ctx.mouse_pos.y(),
                        bounds,
                        gutter_width,
                    );
                    inner.select_line();
                }
                _ => {}
            }

            return InputResult::Consumed;
        }

        // Drag handling
        if inner.is_dragging && ctx.input.is_mouse_down(mouse_button::LEFT) {
            let gutter_width = inner.gutter_width();
            let old_line = inner.cursor_line;
            let old_col = inner.cursor_col;
            inner.click_to_position(ctx.mouse_pos.x(), ctx.mouse_pos.y(), bounds, gutter_width);
            if old_line != inner.cursor_line || old_col != inner.cursor_col {
                inner.start_selection();
                inner.update_selection_cursor();
            }
            return InputResult::Consumed;
        }

        if ctx.input.mouse_released[mouse_button::LEFT] {
            inner.is_dragging = false;
        }

        // Scroll
        if in_bounds && ctx.input.scroll_delta.y() != 0.0 {
            inner.scroll(ctx.input.scroll_delta.y() * 20.0, bounds.height());
            return InputResult::Consumed;
        }

        if !is_focused {
            return if in_bounds {
                InputResult::Consumed
            } else {
                InputResult::Ignore
            };
        }

        let ctrl = ctx.input.is_key_down(KeyCode::Control);
        let shift = ctx.input.is_key_down(KeyCode::Shift);

        // Keyboard: cursor movement
        if ctx.input.key_pressed(KeyCode::ArrowLeft) {
            if shift {
                inner.extend_selection_left();
            } else {
                inner.move_left();
            }
            return InputResult::Consumed;
        }
        if ctx.input.key_pressed(KeyCode::ArrowRight) {
            if shift {
                inner.extend_selection_right();
            } else {
                inner.move_right();
            }
            return InputResult::Consumed;
        }
        if ctx.input.key_pressed(KeyCode::ArrowUp) {
            if shift {
                inner.extend_selection_up();
            } else {
                inner.move_up();
            }
            return InputResult::Consumed;
        }
        if ctx.input.key_pressed(KeyCode::ArrowDown) {
            if shift {
                inner.extend_selection_down();
            } else {
                inner.move_down();
            }
            return InputResult::Consumed;
        }
        if ctx.input.key_pressed(KeyCode::Home) {
            if ctrl {
                inner.move_ctrl_home();
            } else {
                inner.move_home();
            }
            return InputResult::Consumed;
        }
        if ctx.input.key_pressed(KeyCode::End) {
            if ctrl {
                inner.move_ctrl_end();
            } else {
                inner.move_end();
            }
            return InputResult::Consumed;
        }

        // Keyboard: editing
        if ctx.input.key_pressed(KeyCode::Backspace) {
            let mut changes = Vec::new();
            inner.backspace(&mut changes);
            inner.commit_change(changes);
            return InputResult::Consumed;
        }
        if ctx.input.key_pressed(KeyCode::Delete) {
            let mut changes = Vec::new();
            inner.delete_forward(&mut changes);
            inner.commit_change(changes);
            return InputResult::Consumed;
        }
        if ctx.input.key_pressed(KeyCode::Enter) {
            let mut changes = Vec::new();
            inner.insert_newline(&mut changes);
            inner.commit_change(changes);
            return InputResult::Consumed;
        }
        if ctx.input.key_pressed(KeyCode::Tab) {
            let mut changes = Vec::new();
            inner.insert_tab(&mut changes);
            inner.commit_change(changes);
            return InputResult::Consumed;
        }

        // Ctrl shortcuts
        if ctrl {
            if ctx.input.key_pressed(KeyCode::A) {
                inner.select_all();
                return InputResult::Consumed;
            }
            if ctx.input.key_pressed(KeyCode::C) {
                inner.copy();
                return InputResult::Consumed;
            }
            if ctx.input.key_pressed(KeyCode::X) {
                inner.cut();
                return InputResult::Consumed;
            }
            if ctx.input.key_pressed(KeyCode::V) {
                inner.pending_paste = true;
                return InputResult::Consumed;
            }
            if ctx.input.key_pressed(KeyCode::Z) {
                inner.undo();
                return InputResult::Consumed;
            }
            if ctx.input.key_pressed(KeyCode::Y) {
                inner.redo();
                return InputResult::Consumed;
            }
        }

        // Text input
        for &c in &ctx.input.characters {
            if c >= ' ' {
                let mut changes = Vec::new();
                inner.insert_char(c, &mut changes);
                inner.commit_change(changes);
            }
        }

        InputResult::Consumed
    }

    fn draw(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        _animation: &AnimationState,
        _children: &[ViewId],
        interaction: &DrawInteraction,
        view_id: ViewId,
        _children_bounds: &[Rect2D],
    ) {
        let Some(shared) = state.get::<SharedEditorState>(self.state_id) else {
            return;
        };
        let mut inner = shared.0.borrow_mut();

        // Handle pending paste
        if inner.pending_paste {
            let paste_text = inner.clipboard_text.clone();
            inner.paste(&paste_text);
            inner.pending_paste = false;
        }

        let font_size = inner.font_size;
        let line_height = inner.line_height;
        let gutter_width = inner.gutter_width();
        let scroll_y = inner.scroll_y;

        // Background
        let bg_color = Color::new(0.12, 0.12, 0.14, 1.0);
        ctx.draw_list.set_clip(bounds);
        ctx.draw_rect(bounds, bg_color);

        // Calculate visible lines
        let first_visible = (scroll_y / line_height).floor() as usize;
        let viewport_lines = (bounds.height() / line_height).ceil() as usize + 1;
        let last_visible = (first_visible + viewport_lines).min(inner.lines.len());

        let text_color = Color::new(0.76, 0.77, 0.78, 1.0);
        let gutter_color = Color::new(0.45, 0.47, 0.49, 1.0);
        let gutter_bg = Color::new(0.10, 0.10, 0.12, 1.0);

        // Draw gutter background
        let gutter_rect =
            Rect2D::from_origin_size(bounds.min, Vec2::new(gutter_width, bounds.height()));
        ctx.draw_rect(gutter_rect, gutter_bg);

        // Draw line numbers and text
        for line_i in first_visible..last_visible {
            let y = bounds.min.y() + (line_i as f32) * line_height - scroll_y;

            if y + line_height < bounds.min.y() || y > bounds.max.y() {
                continue;
            }

            // Line number
            let line_num = format!("{}", line_i + 1);
            ctx.draw_text(
                &line_num,
                Vec2::new(bounds.min.x() + 4.0, y),
                gutter_color,
                font_size * 0.85,
            );

            // Text line
            let line_text = &inner.lines[line_i];
            if !line_text.is_empty() {
                let text_x = bounds.min.x() + gutter_width + 4.0;
                ctx.draw_text(line_text, Vec2::new(text_x, y), text_color, font_size);
            }
        }

        // Draw selection highlight
        if let Some(ref sel) = inner.selection {
            let ((sl, sc), (el, ec)) = sel.sorted();
            let sel_color = Color::new(0.2, 0.4, 0.7, 0.4);
            let char_width = font_size * 0.6;
            let text_x = bounds.min.x() + gutter_width + 4.0;

            for line_i in sl..=el {
                let y = bounds.min.y() + (line_i as f32) * line_height - scroll_y;
                if y + line_height < bounds.min.y() || y > bounds.max.y() {
                    continue;
                }

                let (start_col, end_col) = if line_i == sl && line_i == el {
                    (sc, ec)
                } else if line_i == sl {
                    (sc, inner.lines[line_i].len())
                } else if line_i == el {
                    (0, ec)
                } else {
                    (0, inner.lines[line_i].len())
                };

                let sel_x = text_x + start_col as f32 * char_width;
                let sel_w = (end_col - start_col) as f32 * char_width;
                let sel_rect =
                    Rect2D::from_origin_size(Vec2::new(sel_x, y), Vec2::new(sel_w, line_height));
                ctx.draw_rect(sel_rect, sel_color);
            }
        }

        // Draw cursor
        let is_focused = interaction.is_focused(view_id);
        if is_focused {
            let cursor_y = bounds.min.y() + (inner.cursor_line as f32) * line_height - scroll_y;
            let char_width = font_size * 0.6;
            let cursor_x =
                bounds.min.x() + gutter_width + 4.0 + inner.cursor_col as f32 * char_width;

            // Blink cursor
            let blink_visible = (ctx.time * 2.0) as i32 % 2 == 0;
            if blink_visible {
                let cursor_color = Color::new(0.9, 0.9, 0.9, 1.0);
                let cursor_rect = Rect2D::from_origin_size(
                    Vec2::new(cursor_x, cursor_y),
                    Vec2::new(2.0, line_height),
                );
                ctx.draw_rect(cursor_rect, cursor_color);
            }

            // Scroll cursor into view
            inner.scroll_cursor_into_view(bounds.height());
        }

        // Border
        let border_color = if is_focused {
            ctx.style().selectable_selected
        } else {
            ctx.style().input_border
        };
        ctx.draw_selection_border(bounds, border_color, 1.0);
    }

    fn focusable(&self) -> bool {
        true
    }

    fn interactive(&self) -> bool {
        true
    }

    fn needs_clip_children(&self) -> bool {
        true
    }
}

impl CodeEditor {
    pub fn on_change(self, _cb: Callback) -> Self {
        self
    }
}

// ---------------------------------------------------------------------------
// Public helpers for testing
// ---------------------------------------------------------------------------

impl EditorStateInner {
    pub(crate) fn cursor_position(&self) -> (usize, usize) {
        (self.cursor_line, self.cursor_col)
    }

    pub(crate) fn set_cursor(&mut self, line: usize, col: usize) {
        self.cursor_line = line;
        self.cursor_col = col;
        self.clamp_cursor();
        self.preferred_x = self.cursor_col;
    }

    pub(crate) fn get_selection_text(&self) -> Option<String> {
        self.selection.as_ref().map(|sel| sel.text(&self.lines))
    }

    pub(crate) fn has_selection(&self) -> bool {
        self.selection.is_some()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::diff::DiffAction;
    use crate::declarative::state::{StateArena, StateId};
    use crate::declarative::widget::Widget;
    use crate::declarative::widgets::text::Text;

    fn make_editor_state_id(arena: &mut StateArena, text: &str) -> StateId {
        arena.get_or_create(
            ViewId::from(slotmap::KeyData::from_ffi(1)),
            SharedEditorState(Rc::new(RefCell::new(EditorStateInner::new(text)))),
        )
    }

    fn make_state(text: &str) -> EditorStateInner {
        EditorStateInner::new(text)
    }

    // --- VAL-EDITOR-001: Empty file renders with cursor at origin ---

    #[test]
    fn test_empty_file_cursor_at_origin() {
        let state = make_state("");
        assert_eq!(state.cursor_position(), (0, 0));
        assert_eq!(state.line_count(), 1);
    }

    // --- VAL-EDITOR-002: Single-line text renders correctly ---

    #[test]
    fn test_single_line_text() {
        let state = make_state("hello world");
        assert_eq!(state.text(), "hello world");
        assert_eq!(state.line_count(), 1);
    }

    // --- VAL-EDITOR-003: Multi-line text renders with correct line breaks ---

    #[test]
    fn test_multi_line_text() {
        let state = make_state("line1\nline2\nline3");
        assert_eq!(state.line_count(), 3);
        assert_eq!(state.lines[0], "line1");
        assert_eq!(state.lines[1], "line2");
        assert_eq!(state.lines[2], "line3");
    }

    // --- VAL-EDITOR-008: Unicode text renders correctly ---

    #[test]
    fn test_unicode_text() {
        let mut state = make_state("héllo wörld 日本語");
        assert_eq!(state.text(), "héllo wörld 日本語");

        // Unicode cursor positioning - col 6 is the space character
        state.set_cursor(0, 6);
        assert_eq!(state.cursor_col, 6);

        // Insert unicode char at col 6 (before the space)
        let mut changes = Vec::new();
        state.insert_char('界', &mut changes);
        assert_eq!(state.lines[0], "héllo界 wörld 日本語");
    }

    // --- VAL-EDITOR-009: Tab characters render with appropriate width ---

    #[test]
    fn test_tab_insertion() {
        let mut state = make_state("hello");
        state.set_cursor(0, 0);
        let mut changes = Vec::new();
        state.insert_tab(&mut changes);
        assert_eq!(state.lines[0], "    hello");
        assert_eq!(state.cursor_col, 4);
    }

    // --- VAL-EDITOR-010: Click positions cursor at correct character boundary ---

    #[test]
    fn test_click_positions_cursor() {
        let mut state = make_state("hello world");
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(400.0, 300.0));
        let gutter_width = state.gutter_width();

        // Click near the start
        state.click_to_position(
            bounds.min.x() + gutter_width + 2.0,
            0.0,
            bounds,
            gutter_width,
        );
        assert_eq!(state.cursor_col, 0);

        // Click past end of line
        state.click_to_position(800.0, 0.0, bounds, gutter_width);
        assert_eq!(state.cursor_col, "hello world".len());
    }

    // --- VAL-EDITOR-011: Arrow keys move cursor correctly ---

    #[test]
    fn test_arrow_left_right() {
        let mut state = make_state("hello");
        state.set_cursor(0, 3);

        state.move_left();
        assert_eq!(state.cursor_position(), (0, 2));

        state.move_right();
        assert_eq!(state.cursor_position(), (0, 3));
    }

    #[test]
    fn test_arrow_up_down() {
        let mut state = make_state("line1\nline2\nline3");
        state.set_cursor(1, 3);

        state.move_up();
        assert_eq!(state.cursor_position(), (0, 3));

        state.move_down();
        assert_eq!(state.cursor_position(), (1, 3));

        state.move_down();
        assert_eq!(state.cursor_position(), (2, 3));
    }

    #[test]
    fn test_home_end() {
        let mut state = make_state("hello");
        state.set_cursor(0, 3);

        state.move_home();
        assert_eq!(state.cursor_col, 0);

        state.move_end();
        assert_eq!(state.cursor_col, 5);
    }

    // --- VAL-EDITOR-012: Ctrl+Home/End move to buffer start/end ---

    #[test]
    fn test_ctrl_home_end() {
        let mut state = make_state("line1\nline2\nline3");
        state.set_cursor(2, 3);

        state.move_ctrl_home();
        assert_eq!(state.cursor_position(), (0, 0));

        state.move_ctrl_end();
        assert_eq!(state.cursor_position(), (2, 5));
    }

    // --- VAL-EDITOR-013: Cursor does not move past buffer boundaries ---

    #[test]
    fn test_cursor_buffer_boundaries() {
        let mut state = make_state("hello");

        state.move_left();
        assert_eq!(state.cursor_position(), (0, 0));

        state.set_cursor(0, 5);
        state.move_right();
        assert_eq!(state.cursor_position(), (0, 5));
    }

    // --- VAL-EDITOR-014: Cursor maintains horizontal position across different-length lines ---

    #[test]
    fn test_cursor_horizontal_preservation() {
        let mut state = make_state("long line here\nshort\nlong line here");
        state.set_cursor(0, 10);

        state.move_down(); // to "short" (len 5)
        assert_eq!(state.cursor_position(), (1, 5));

        state.move_down(); // back to long line
        assert_eq!(state.cursor_position(), (2, 10));
    }

    // --- VAL-EDITOR-015: Cursor wraps across line boundaries ---

    #[test]
    fn test_cursor_line_wrapping() {
        let mut state = make_state("hello\nworld");
        state.set_cursor(0, 5);

        state.move_right();
        assert_eq!(state.cursor_position(), (1, 0));

        state.move_left();
        assert_eq!(state.cursor_position(), (0, 5));
    }

    // --- VAL-EDITOR-016: Typing inserts at cursor position ---

    #[test]
    fn test_insert_at_cursor() {
        let mut state = make_state("hello");
        state.set_cursor(0, 3);
        let mut changes = Vec::new();
        state.insert_char('X', &mut changes);
        assert_eq!(state.lines[0], "helXlo");
        assert_eq!(state.cursor_col, 4);
    }

    // --- VAL-EDITOR-017: Enter splits the current line ---

    #[test]
    fn test_enter_splits_line() {
        let mut state = make_state("hello");
        state.set_cursor(0, 3);
        let mut changes = Vec::new();
        state.insert_newline(&mut changes);
        assert_eq!(state.lines[0], "hel");
        assert_eq!(state.lines[1], "lo");
        assert_eq!(state.cursor_position(), (1, 0));
    }

    // --- VAL-EDITOR-018: Tab inserts tab character ---

    #[test]
    fn test_tab_inserts_spaces() {
        let mut state = make_state("hello");
        state.set_cursor(0, 3);
        let mut changes = Vec::new();
        state.insert_tab(&mut changes);
        assert_eq!(state.lines[0], "hel    lo");
        assert_eq!(state.cursor_col, 7);
    }

    // --- VAL-EDITOR-019: Typing with selection replaces selection ---

    #[test]
    fn test_type_replaces_selection() {
        let mut state = make_state("hello");
        state.set_cursor(0, 2);
        state.start_selection();
        state.cursor_col = 4;
        state.update_selection_cursor();
        assert!(state.has_selection());

        let mut changes = Vec::new();
        state.insert_char('r', &mut changes);
        assert_eq!(state.lines[0], "hero");
        assert!(!state.has_selection());
    }

    // --- VAL-EDITOR-020: Backspace deletes character before cursor ---

    #[test]
    fn test_backspace() {
        let mut state = make_state("hello");
        state.set_cursor(0, 3);
        let mut changes = Vec::new();
        state.backspace(&mut changes);
        assert_eq!(state.lines[0], "helo");
        assert_eq!(state.cursor_col, 2);
    }

    // --- VAL-EDITOR-021: Backspace at column 0 joins with previous line ---

    #[test]
    fn test_backspace_join_lines() {
        let mut state = make_state("hel\nlo");
        state.set_cursor(1, 0);
        let mut changes = Vec::new();
        state.backspace(&mut changes);
        assert_eq!(state.lines[0], "hello");
        assert_eq!(state.cursor_position(), (0, 3));
    }

    // --- VAL-EDITOR-022: Backspace at buffer start does nothing ---

    #[test]
    fn test_backspace_at_start() {
        let mut state = make_state("hello");
        let mut changes = Vec::new();
        state.backspace(&mut changes);
        assert_eq!(state.lines[0], "hello");
        assert_eq!(state.cursor_position(), (0, 0));
    }

    // --- VAL-EDITOR-023: Delete key deletes character after cursor ---

    #[test]
    fn test_delete_forward() {
        let mut state = make_state("hello");
        state.set_cursor(0, 2);
        let mut changes = Vec::new();
        state.delete_forward(&mut changes);
        assert_eq!(state.lines[0], "helo");
    }

    // --- VAL-EDITOR-024: Click + drag creates selection ---

    #[test]
    fn test_drag_selection() {
        let mut state = make_state("hello world");
        state.set_cursor(0, 2);
        state.start_selection();
        state.cursor_col = 5;
        state.update_selection_cursor();

        let sel_text = state.get_selection_text().unwrap();
        assert_eq!(sel_text, "llo");
    }

    // --- VAL-EDITOR-025: Double-click selects word ---

    #[test]
    fn test_select_word() {
        let mut state = make_state("hello world");
        state.set_cursor(0, 7); // 'w' in 'world'
        state.select_word();
        let sel_text = state.get_selection_text().unwrap();
        assert_eq!(sel_text, "world");
    }

    // --- VAL-EDITOR-026: Triple-click selects line ---

    #[test]
    fn test_select_line() {
        let mut state = make_state("hello world");
        state.set_cursor(0, 5);
        state.select_line();
        let sel_text = state.get_selection_text().unwrap();
        assert_eq!(sel_text, "hello world");
    }

    // --- VAL-EDITOR-027: Shift+Arrow extends selection ---

    #[test]
    fn test_shift_arrow_selection() {
        let mut state = make_state("hello");
        state.set_cursor(0, 2);

        state.extend_selection_right();
        assert!(state.has_selection());
        assert_eq!(state.get_selection_text().unwrap(), "l");

        state.extend_selection_right();
        assert_eq!(state.get_selection_text().unwrap(), "ll");
    }

    #[test]
    fn test_shift_arrow_left_selection() {
        let mut state = make_state("hello");
        state.set_cursor(0, 3);

        state.extend_selection_left();
        assert_eq!(state.get_selection_text().unwrap(), "l");
    }

    // --- VAL-EDITOR-028: Ctrl+A selects all ---

    #[test]
    fn test_select_all() {
        let mut state = make_state("hello\nworld");
        state.select_all();
        let sel_text = state.get_selection_text().unwrap();
        assert_eq!(sel_text, "hello\nworld");
    }

    // --- VAL-EDITOR-029: Clicking without dragging clears selection ---

    #[test]
    fn test_click_clears_selection() {
        let mut state = make_state("hello world");
        state.set_cursor(0, 2);
        state.start_selection();
        state.cursor_col = 5;
        state.update_selection_cursor();
        assert!(state.has_selection());

        // Simulate click clearing selection
        state.clear_selection();
        assert!(!state.has_selection());
    }

    // --- VAL-EDITOR-030: Ctrl+C copies selected text ---

    #[test]
    fn test_copy_selection() {
        let mut state = make_state("hello world");
        state.set_cursor(0, 2);
        state.start_selection();
        state.cursor_col = 5;
        state.update_selection_cursor();

        let copied = state.copy().unwrap();
        assert_eq!(copied, "llo");
        assert_eq!(state.text(), "hello world"); // Buffer unchanged
    }

    // --- VAL-EDITOR-031: Ctrl+V pastes at cursor ---

    #[test]
    fn test_paste_at_cursor() {
        let mut state = make_state("hello");
        state.set_cursor(0, 2);
        state.paste("xyz");
        assert_eq!(state.lines[0], "hexyzllo");
    }

    // --- VAL-EDITOR-032: Ctrl+V with selection replaces selection ---

    #[test]
    fn test_paste_with_selection() {
        let mut state = make_state("hello");
        // Select "ll" (col 2..4)
        state.set_cursor(0, 2);
        state.start_selection();
        state.cursor_col = 4;
        state.update_selection_cursor();

        state.paste("y");
        assert_eq!(state.lines[0], "heyo");
    }

    // --- VAL-EDITOR-033: Ctrl+X cuts selected text ---

    #[test]
    fn test_cut_selection() {
        let mut state = make_state("hello");
        state.set_cursor(0, 2);
        state.start_selection();
        state.cursor_col = 4;
        state.update_selection_cursor();

        let cut = state.cut().unwrap();
        assert_eq!(cut, "ll");
        assert_eq!(state.text(), "heo");
    }

    // --- VAL-EDITOR-034: Undo reverses last typed character ---

    #[test]
    fn test_undo_insert() {
        let mut state = make_state("");
        let mut changes = Vec::new();
        state.insert_char('a', &mut changes);
        state.commit_change(changes);
        assert_eq!(state.text(), "a");

        state.undo();
        assert_eq!(state.text(), "");
    }

    // --- VAL-EDITOR-035: Redo re-applies undone action ---

    #[test]
    fn test_redo() {
        let mut state = make_state("");
        let mut changes = Vec::new();
        state.insert_char('a', &mut changes);
        state.commit_change(changes);

        state.undo();
        assert_eq!(state.text(), "");

        state.redo();
        assert_eq!(state.text(), "a");
    }

    // --- VAL-EDITOR-036: New edit after undo clears redo stack ---

    #[test]
    fn test_new_edit_clears_redo() {
        let mut state = make_state("");
        let mut changes = Vec::new();
        state.insert_char('a', &mut changes);
        state.commit_change(changes);

        state.undo();
        assert_eq!(state.text(), "");

        // New edit
        let mut changes = Vec::new();
        state.insert_char('b', &mut changes);
        state.commit_change(changes);

        // Redo should do nothing
        state.redo();
        assert_eq!(state.text(), "b");
    }

    // --- VAL-EDITOR-037: Undo reverses Enter (line split) ---

    #[test]
    fn test_undo_enter() {
        let mut state = make_state("hello");
        state.set_cursor(0, 3);
        let mut changes = Vec::new();
        state.insert_newline(&mut changes);
        state.commit_change(changes);
        assert_eq!(state.text(), "hel\nlo");

        state.undo();
        assert_eq!(state.text(), "hello");
    }

    // --- VAL-EDITOR-038: Mouse wheel scrolls viewport ---

    #[test]
    fn test_scroll_viewport() {
        let mut state = make_state("line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8");
        assert_eq!(state.scroll_y, 0.0);

        state.scroll(40.0, 100.0);
        assert!(state.scroll_y > 0.0);
    }

    // --- VAL-EDITOR-040: Scroll offset clamped to valid range ---

    #[test]
    fn test_scroll_clamped() {
        let mut state = make_state("line1\nline2\nline3");
        state.scroll(1000.0, 100.0);
        // scroll_y should be clamped
        let content_height = 3.0 * state.line_height;
        assert!(state.scroll_y <= (content_height - 100.0).max(0.0));
    }

    // --- VAL-EDITOR-041: Line numbers display for each visible line ---

    #[test]
    fn test_line_numbers() {
        let state = make_state("line1\nline2\nline3");
        assert_eq!(state.line_count(), 3);
    }

    // --- VAL-EDITOR-042: Line numbers update when lines are added ---

    #[test]
    fn test_line_numbers_update_on_enter() {
        let mut state = make_state("hello");
        assert_eq!(state.line_count(), 1);

        state.set_cursor(0, 3);
        let mut changes = Vec::new();
        state.insert_newline(&mut changes);
        assert_eq!(state.line_count(), 2);
    }

    // --- VAL-EDITOR-043: Line number gutter width adjusts ---

    #[test]
    fn test_gutter_width_adjusts() {
        let state_9 = make_state("line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9");
        let state_10 = make_state("a\nb\nc\nd\ne\nf\ng\nh\ni\nj");

        let w9 = state_9.gutter_width();
        let w10 = state_10.gutter_width();
        assert!(w10 > w9, "10-line gutter should be wider than 9-line");
    }

    // --- VAL-EDITOR-044: CodeEditor implements Build trait ---

    #[test]
    fn test_code_editor_widget_trait() {
        let mut arena = StateArena::new();
        let sid = make_editor_state_id(&mut arena, "hello");
        let editor = CodeEditor { state_id: sid };
        assert!(editor.focusable());
        assert!(editor.interactive());
        assert!(editor.needs_clip_children());
    }

    // --- VAL-EDITOR-046: Editor state survives across frames ---

    #[test]
    fn test_state_persistence() {
        let shared = SharedEditorState(Rc::new(RefCell::new(EditorStateInner::new("hello"))));
        let shared2 = shared.clone();

        {
            let mut inner = shared.0.borrow_mut();
            let mut changes = Vec::new();
            inner.insert_char('!', &mut changes);
            inner.commit_change(changes);
        }

        let inner = shared2.0.borrow();
        assert_eq!(inner.text(), "!hello");
    }

    // --- VAL-EDITOR-047: CodeEditor is focusable and interactive ---

    #[test]
    fn test_focusable_interactive() {
        let mut arena = StateArena::new();
        let sid = make_editor_state_id(&mut arena, "hello");
        let editor = CodeEditor { state_id: sid };
        assert!(editor.focusable());
        assert!(editor.interactive());
    }

    // --- VAL-EDITOR-049: Select all + delete clears buffer ---

    #[test]
    fn test_select_all_delete() {
        let mut state = make_state("hello world");
        state.select_all();
        let mut changes = Vec::new();
        state.delete_selection_into(&mut changes);
        assert_eq!(state.text(), "");
        assert_eq!(state.cursor_position(), (0, 0));
    }

    // --- Widget diff test ---

    #[test]
    fn test_editor_diff() {
        let mut arena = StateArena::new();
        let sid1 = make_editor_state_id(&mut arena, "hello");
        let sid2 = make_editor_state_id(&mut arena, "world");

        let a = CodeEditor { state_id: sid1 };
        let b = CodeEditor { state_id: sid2 };
        assert_eq!(b.diff_against(&a), DiffAction::Update);

        let text = Text {
            content: "hello".into(),
            color: None,
            font_size: None,
        };
        assert_eq!(a.diff_against(&text), DiffAction::Replace);
    }

    // --- Multi-line selection text ---

    #[test]
    fn test_multiline_selection_text() {
        let mut state = make_state("hello\nworld\nfoo");
        state.set_cursor(0, 2);
        state.start_selection();
        state.cursor_line = 2;
        state.cursor_col = 3;
        state.update_selection_cursor();

        let sel_text = state.get_selection_text().unwrap();
        assert_eq!(sel_text, "llo\nworld\nfoo");
    }

    // --- Shift+up/down selection ---

    #[test]
    fn test_shift_up_down_selection() {
        let mut state = make_state("line1\nline2\nline3");
        state.set_cursor(1, 3);
        state.preferred_x = 3;

        state.extend_selection_up();
        assert!(state.has_selection());
        // anchor=(1,3), cursor=(0,3): selection is lines[0][3..] + "\n" + lines[1][..3]
        let sel = state.get_selection_text().unwrap();
        assert_eq!(sel, "e1\nlin");

        state.extend_selection_down();
        state.extend_selection_down();
        // anchor=(1,3), cursor=(2,3): selection is lines[1][3..] + "\n" + lines[2][..3]
        let sel = state.get_selection_text().unwrap();
        assert_eq!(sel, "e2\nlin");
    }

    // --- Undo backspace ---

    #[test]
    fn test_undo_backspace() {
        let mut state = make_state("hello");
        state.set_cursor(0, 5);
        let mut changes = Vec::new();
        state.backspace(&mut changes);
        state.commit_change(changes);
        assert_eq!(state.text(), "hell");

        state.undo();
        assert_eq!(state.text(), "hello");
    }

    // --- Undo delete forward ---

    #[test]
    fn test_undo_delete_forward() {
        let mut state = make_state("hello");
        state.set_cursor(0, 2);
        let mut changes = Vec::new();
        state.delete_forward(&mut changes);
        state.commit_change(changes);
        assert_eq!(state.text(), "helo");

        state.undo();
        assert_eq!(state.text(), "hello");
    }

    // --- Multi-line paste ---

    #[test]
    fn test_multiline_paste() {
        let mut state = make_state("hello");
        state.set_cursor(0, 2);
        state.paste("a\nb\nc");
        assert_eq!(state.text(), "hea\nb\ncllo");
        assert_eq!(state.cursor_position(), (2, 1));
    }

    // --- Scroll cursor into view ---

    #[test]
    fn test_scroll_cursor_into_view() {
        let mut state = make_state("line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8");
        state.set_cursor(7, 0); // last line
        state.scroll_cursor_into_view(50.0);
        let cursor_y = 7.0 * state.line_height;
        assert!(cursor_y >= state.scroll_y);
        assert!(cursor_y <= state.scroll_y + 50.0);
    }

    // --- Selection text edge cases ---

    #[test]
    fn test_empty_selection() {
        let state = make_state("hello");
        assert!(state.get_selection_text().is_none());
    }

    // --- Undo join lines (backspace at col 0) ---

    #[test]
    fn test_undo_join_lines() {
        let mut state = make_state("hel\nlo");
        state.set_cursor(1, 0);
        let mut changes = Vec::new();
        state.backspace(&mut changes);
        state.commit_change(changes);
        assert_eq!(state.text(), "hello");

        state.undo();
        assert_eq!(state.text(), "hel\nlo");
    }

    // --- Cut clears selection and text ---

    #[test]
    fn test_cut_clears() {
        let mut state = make_state("hello");
        state.set_cursor(0, 0);
        state.select_all();
        let result = state.cut().unwrap();
        assert_eq!(result, "hello");
        assert_eq!(state.text(), "");
    }

    // --- Copy without selection returns None ---

    #[test]
    fn test_copy_no_selection() {
        let mut state = make_state("hello");
        assert!(state.copy().is_none());
    }
}
