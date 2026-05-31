use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, Position, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::Callback;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;
use crate::input::{KeyCode, mouse_button};

pub(crate) struct Modal {
    pub width: f32,
    pub height: f32,
    pub open_id: StateId,
    pub on_close: Option<Callback>,
    children: Vec<ViewId>,
}

impl Modal {
    pub fn new(width: f32, height: f32, open_id: StateId, on_close: Option<Callback>) -> Self {
        Self {
            width,
            height,
            open_id,
            on_close,
            children: Vec::new(),
        }
    }
}

impl Widget for Modal {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Modal>().is_some() {
            DiffAction::RecurseChildren
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        Style {
            position: Position::Absolute,
            size: taffy::Size {
                width: Dimension::Length(self.width),
                height: Dimension::Length(self.height),
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
        let is_open: bool = state.get(self.open_id).unwrap_or_default();
        if !is_open {
            return InputResult::Ignore;
        }

        // Click outside modal closes it
        if !bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
            state.set(self.open_id, false);
            if let Some(ref cb) = self.on_close {
                ctx.callbacks.invoke(cb, ctx.actions);
            }
            return InputResult::Consumed;
        }

        // Escape closes it
        if ctx.input.key_pressed(KeyCode::Escape) {
            state.set(self.open_id, false);
            if let Some(ref cb) = self.on_close {
                ctx.callbacks.invoke(cb, ctx.actions);
            }
            return InputResult::Consumed;
        }

        InputResult::Ignore
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
        let is_open: bool = state.get(self.open_id).unwrap_or_default();
        if !is_open {
            return;
        }

        let screen_size = ctx.screen_size();
        let screen_bounds = Rect2D::new(Vec2::new(0.0, 0.0), screen_size);
        ctx.draw_rect(screen_bounds, ctx.style().popup_shadow);

        ctx.draw_rect_border(
            bounds,
            ctx.style().window_bg,
            ctx.style().window_border,
            1.0,
        );
    }

    fn focusable(&self) -> bool {
        false
    }

    fn children(&self) -> &[ViewId] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<ViewId> {
        &mut self.children
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::build::CallbackTable;

    fn make_modal(open: bool) -> (Modal, StateArena) {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let open_id = arena.get_or_create(view_id, open);
        let modal = Modal::new(400.0, 300.0, open_id, None);
        (modal, arena)
    }

    fn make_modal_with_close() -> (Modal, StateArena, CallbackTable) {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let open_id = arena.get_or_create(view_id, true);
        let mut callbacks = CallbackTable::new();
        let cb = callbacks.push(|_actions| {});
        let modal = Modal::new(400.0, 300.0, open_id, Some(cb));
        (modal, arena, callbacks)
    }

    #[test]
    fn test_modal_diff_same_type() {
        let (a, _) = make_modal(true);
        let (b, _) = make_modal(true);
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_modal_diff_different_type() {
        let (modal, _) = make_modal(true);
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(modal.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_modal_children() {
        let (mut modal, _) = make_modal(true);
        assert!(modal.children().is_empty());
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        modal.children_mut().push(view_id);
        assert_eq!(modal.children().len(), 1);
    }

    #[test]
    fn test_modal_escape_closes() {
        let (modal, mut arena, mut callbacks) = make_modal_with_close();

        let mut input = crate::input::UiInputState::default();
        input.keys_pressed.push(KeyCode::Escape);

        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(500.0, 500.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
        };

        let bounds = Rect2D::new(Vec2::new(200.0, 150.0), Vec2::new(600.0, 450.0));
        let result = modal.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let is_open: bool = arena.get(modal.open_id).unwrap_or_default();
        assert!(!is_open, "Escape should close modal");
    }

    #[test]
    fn test_modal_click_outside_closes() {
        let (modal, mut arena, mut callbacks) = make_modal_with_close();

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(10.0, 10.0));
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(10.0, 10.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
        };

        let bounds = Rect2D::new(Vec2::new(200.0, 150.0), Vec2::new(600.0, 450.0));
        let result = modal.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let is_open: bool = arena.get(modal.open_id).unwrap_or_default();
        assert!(!is_open, "clicking outside should close modal");
    }

    #[test]
    fn test_modal_closed_ignores_input() {
        let (modal, mut arena) = make_modal(false);

        let mut input = crate::input::UiInputState::default();
        input.keys_pressed.push(KeyCode::Escape);

        let mut callbacks = CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(500.0, 500.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
        };

        let bounds = Rect2D::new(Vec2::new(200.0, 150.0), Vec2::new(600.0, 450.0));
        let result = modal.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Ignore);
    }
}
