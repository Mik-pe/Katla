use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;
use crate::input::KeyCode;
use crate::input::mouse_button;

use super::super::animation::AnimationState;
use super::super::descriptor::Callback;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};

pub struct TextField {
    pub placeholder: String,
    pub value_id: StateId,
    pub on_submit: Option<Callback>,
}

impl Widget for TextField {
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

    fn layout_style(&self, measure: MeasureFn<'_>) -> Style {
        let text_size = measure(&self.placeholder, None);
        Style {
            size: Size {
                width: Dimension::Length(text_size.x() + 16.0),
                height: Dimension::Length(text_size.y() + 12.0),
            },
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
        if bounds.contains(ctx.mouse_pos) && ctx.input.mouse_pressed[mouse_button::LEFT] {
            return InputResult::Consumed;
        }

        let mut text: String = state.get(self.value_id).unwrap_or_default();
        let mut changed = false;

        if ctx.input.key_pressed(KeyCode::Backspace) && !text.is_empty() {
            let prev = text[..]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            text.truncate(prev);
            changed = true;
        }

        if ctx.input.key_pressed(KeyCode::Enter)
            && let Some(ref callback) = self.on_submit
        {
            ctx.callbacks.invoke(callback, ctx.actions);
        }

        if ctx.input.key_pressed(KeyCode::Escape) {
            return InputResult::Ignore;
        }

        for &c in &ctx.input.characters {
            if c >= ' ' {
                text.push(c);
                changed = true;
            }
        }

        if changed {
            state.set(self.value_id, text);
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
        _interaction: &DrawInteraction,
        _view_id: ViewId,
        _children_bounds: &[Rect2D],
    ) {
        let text: String = state.get(self.value_id).unwrap_or_default();

        ctx.draw_rounded_rect(bounds, ctx.style().input_bg, ctx.style().input_rounding);
        ctx.draw_rounded_selection_border(
            bounds,
            ctx.style().input_border,
            1.0,
            ctx.style().input_rounding,
        );

        let padding = 4.0;
        let font_size = ctx.style().font_size;
        let text_size = ctx.measure_text(&text, font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + padding,
            bounds.center().y() - text_size.y() * 0.5,
        );

        if text.is_empty() {
            ctx.draw_text(
                &self.placeholder,
                text_pos,
                ctx.style().text_hint,
                font_size,
            );
        } else {
            ctx.draw_text(&text, text_pos, ctx.style().input_text, font_size);
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn interactive(&self) -> bool {
        true
    }
}
impl TextField {
    pub fn on_submit(mut self, cb: super::super::descriptor::Callback) -> Self {
        self.on_submit = Some(cb);
        self
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::diff::DiffAction;
    use crate::declarative::widget::{InputResult, Widget};
    use crate::declarative::widgets::progress::Progress;
    use crate::input::UiInputState;

    fn make_textfield() -> TextField {
        TextField {
            placeholder: "Enter text...".into(),
            value_id: StateId::test_id(),
            on_submit: None,
        }
    }

    #[test]
    fn test_textfield_diff() {
        let a = make_textfield();
        let b = make_textfield();
        assert_eq!(b.diff_against(&a), DiffAction::Update);

        let progress = Progress {
            value: 0.5,
            range: 0.0..=1.0,
            fill_color: None,
            label: None,
        };
        assert_eq!(a.diff_against(&progress), DiffAction::Replace);
    }

    #[test]
    fn test_textfield_focusable() {
        let tf = make_textfield();
        assert!(tf.focusable());
    }

    #[test]
    fn test_textfield_layout_default() {
        let tf = make_textfield();
        let style = tf.layout_style(&crate::declarative::layout::measure_text_descriptor);
        let default_width = taffy::Dimension::Length(0.0);
        assert!(style.size.width != default_width);
    }

    #[test]
    fn test_textfield_keyboard_input() {
        let mut state = StateArena::new();
        let value_id = state.get_or_create(ViewId::default(), String::new());
        let tf = TextField {
            placeholder: "Enter text...".into(),
            value_id,
            on_submit: None,
        };

        let mut input = UiInputState::new();
        input.characters = vec!['H', 'i'];

        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 24.0));

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(50.0, 12.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
        };

        let result = tf.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let text: String = state.get(tf.value_id).unwrap();
        assert_eq!(text, "Hi");
    }

    #[test]
    fn test_textfield_click_focus() {
        let tf = make_textfield();
        let mut state = StateArena::new();

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(50.0, 12.0);
        input.mouse_pressed[mouse_button::LEFT] = true;

        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 24.0));

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: input.mouse_pos,
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
        };

        let result = tf.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);
    }

    #[test]
    fn test_textfield_backspace() {
        let mut state = StateArena::new();
        let value_id = state.get_or_create(ViewId::default(), "Hello".to_string());
        let tf = TextField {
            placeholder: "Enter text...".into(),
            value_id,
            on_submit: None,
        };

        let mut input = UiInputState::new();
        input.keys_pressed.push(KeyCode::Backspace);

        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 24.0));

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(50.0, 12.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
        };

        let _ = tf.handle_input(&mut ctx, &mut state, bounds, &[]);
        let text: String = state.get(tf.value_id).unwrap();
        assert_eq!(text, "Hell");
    }
}
