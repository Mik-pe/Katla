//! Compact numeric authoring fields with mouse scrubbing and keyboard entry.
use super::super::widget::{DrawInfo, InputContext, InputResult, MeasureFn, Widget};
use super::super::{AnimationState, DiffAction, StateArena, StateId, ViewId};
use crate::input::{KeyCode, mouse_button};
use crate::{FontSize, UiContext};
use katla_math::{Color, Rect2D, Vec2};
use std::any::Any;
use std::ops::RangeInclusive;
use taffy::{Dimension, Size, Style};

/// Value and transient edit state retained independently of widget rebuilds.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberState {
    pub value: f32,
    pub draft: Option<String>,
    origin: Option<(f32, f32)>,
    invalid: bool,
}
impl NumberState {
    /// Create an idle numeric field.
    pub fn new(value: f32) -> Self {
        Self {
            value,
            draft: None,
            origin: None,
            invalid: false,
        }
    }
}

/// A labeled scalar field; drag to scrub, type to replace, Enter to commit.
pub struct NumberInput {
    pub label: String,
    pub state_id: StateId,
    pub step: f32,
    pub range: RangeInclusive<f32>,
    pub prefix: Option<(String, Color)>,
}
impl NumberInput {
    /// Construct a field bound to a NumberState in the state arena.
    pub fn new(
        label: impl Into<String>,
        state_id: StateId,
        range: RangeInclusive<f32>,
        step: f32,
    ) -> Self {
        Self {
            label: label.into(),
            state_id,
            range,
            step,
            prefix: None,
        }
    }
    /// Add an axis marker with a restrained semantic tint.
    pub fn axis(mut self, label: &str, color: Color) -> Self {
        self.prefix = Some((label.into(), color));
        self
    }
    fn commit(&self, state: &mut NumberState) {
        if let Some(draft) = &state.draft {
            match draft.parse::<f32>() {
                Ok(value) if value.is_finite() => {
                    state.value = value.clamp(*self.range.start(), *self.range.end());
                    state.draft = None;
                    state.invalid = false;
                }
                _ => state.invalid = true,
            }
        }
    }
}
impl Widget for NumberInput {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().is::<Self>() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }
    fn layout_style(&self, _: MeasureFn<'_>) -> Style {
        Style {
            size: Size {
                width: Dimension::Length(0.0),
                height: Dimension::Length(crate::tokens::COMPACT_CONTROL_HEIGHT),
            },
            min_size: Size {
                width: Dimension::Length(44.0),
                height: Dimension::Auto,
            },
            flex_grow: 1.0,
            ..Style::default()
        }
    }
    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        arena: &mut StateArena,
        bounds: Rect2D,
        _: &[ViewId],
    ) -> InputResult {
        let Some(mut state) = arena.get::<NumberState>(self.state_id) else {
            return InputResult::Ignore;
        };
        let focused = ctx.focused_id == Some(ctx.view_id)
            || (bounds.contains(ctx.mouse_pos) && ctx.input.mouse_pressed[mouse_button::LEFT]);
        let mut handled = false;
        if bounds.contains(ctx.mouse_pos) && ctx.input.mouse_pressed[mouse_button::LEFT] {
            self.commit(&mut state);
            state.origin = Some((ctx.mouse_pos.x(), state.value));
            ctx.active_id = Some(ctx.view_id);
            handled = true;
        }
        if ctx.active_id == Some(ctx.view_id) && ctx.input.mouse_down[mouse_button::LEFT] {
            if let Some((x, value)) = state.origin {
                let value = value + (ctx.mouse_pos.x() - x) * self.step;
                if value.is_finite() {
                    state.value = value.clamp(*self.range.start(), *self.range.end());
                }
            }
            handled = true;
        }
        if !ctx.input.mouse_down[mouse_button::LEFT] {
            state.origin = None;
        }
        if focused {
            if ctx.input.key_pressed(KeyCode::Backspace) {
                state
                    .draft
                    .get_or_insert_with(|| format!("{:.2}", state.value))
                    .pop();
                handled = true;
            }
            for character in &ctx.input.characters {
                if character.is_ascii_digit() || matches!(character, '-' | '+' | '.' | 'e' | 'E') {
                    if state.invalid {
                        state.draft = None;
                        state.invalid = false;
                    }
                    state.draft.get_or_insert_with(String::new).push(*character);
                    handled = true;
                }
            }
            if ctx.input.key_pressed(KeyCode::Escape) {
                state.draft = None;
                state.invalid = false;
                handled = true;
            } else if ctx.input.key_pressed(KeyCode::Enter) {
                self.commit(&mut state);
                handled = true;
            } else if ctx.input.key_pressed(KeyCode::ArrowUp)
                || ctx.input.key_pressed(KeyCode::ArrowDown)
            {
                let delta = if ctx.input.key_pressed(KeyCode::ArrowUp) {
                    self.step
                } else {
                    -self.step
                };
                state.draft = None;
                state.invalid = false;
                state.value = (state.value + delta).clamp(*self.range.start(), *self.range.end());
                handled = true;
            }
        } else if state.draft.is_some() {
            self.commit(&mut state);
        }
        arena.set(self.state_id, state);
        if handled {
            InputResult::Consumed
        } else {
            InputResult::Ignore
        }
    }
    fn draw(
        &self,
        ctx: &mut UiContext,
        arena: &StateArena,
        bounds: Rect2D,
        animation: &AnimationState,
        _: &[ViewId],
        info: &DrawInfo,
    ) {
        let Some(state) = arena.get::<NumberState>(self.state_id) else {
            return;
        };
        let style = ctx.style().clone();
        let focused = info.interaction.is_focused(info.view_id);
        ctx.draw_rounded_rect(
            bounds,
            animation.apply_to_color(style.input_bg),
            style.input_rounding,
        );
        if focused || state.invalid {
            ctx.draw_rounded_selection_border(
                bounds,
                if state.invalid {
                    style.error
                } else {
                    style.focus_ring_color
                },
                1.0,
                style.input_rounding,
            );
        }
        ctx.push_clip(bounds);
        let size = ctx.scaled_font_size(FontSize::Small);
        if let Some((prefix, color)) = &self.prefix {
            let height = ctx.measure_text(prefix, size).y();
            ctx.draw_text(
                prefix,
                Vec2::new(bounds.min.x() + 4.0, bounds.center().y() - height / 2.0),
                *color,
                size,
            );
        }
        let text = state.draft.unwrap_or_else(|| format!("{:.2}", state.value));
        let measured = ctx.measure_text(&text, size);
        let prefix_width = if self.prefix.is_some() { 14.0 } else { 0.0 };
        let x = (bounds.max.x() - measured.x() - 4.0).max(bounds.min.x() + 4.0 + prefix_width);
        ctx.draw_text(
            &text,
            Vec2::new(x, bounds.center().y() - measured.y() / 2.0),
            if state.invalid {
                style.error
            } else {
                style.input_text
            },
            size,
        );
        ctx.pop_clip();
    }
    fn focusable(&self) -> bool {
        true
    }
    fn wants_global_input(&self, _: &StateArena) -> bool {
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_invalid_numeric_entry_preserves_value_and_valid_entry_is_clamped() {
        let input = NumberInput::new("Scale X", StateId::test_id(), 0.01..=100.0, 0.01);
        for text in ["NaN", "inf", "-", "1e99", ""] {
            let mut state = NumberState::new(2.0);
            state.draft = Some(text.into());
            input.commit(&mut state);
            assert_eq!(state.value, 2.0);
        }
        let mut state = NumberState::new(2.0);
        state.draft = Some("0".into());
        input.commit(&mut state);
        assert_eq!(state.value, 0.01);
    }
}
