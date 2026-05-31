use std::any::Any;

use katla_math::Rect2D;
use taffy::Style;

use super::super::animation::AnimationState;
use super::super::descriptor::FlexProps;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{
    ChildWidgets, DrawInteraction, InputContext, InputResult, MeasureFn, Widget,
};
use crate::context::UiContext;

pub struct ScrollView {
    pub scroll_state_id: StateId,
    pub flex: FlexProps,
    pub child_widget: Option<Box<dyn super::super::widget::Widget>>,
    children: Vec<ViewId>,
}

impl ScrollView {
    pub fn new(
        scroll_state_id: StateId,
        flex: FlexProps,
        child_widget: Option<Box<dyn super::super::widget::Widget>>,
    ) -> Self {
        Self {
            scroll_state_id,
            flex,
            child_widget,
            children: Vec::new(),
        }
    }
}

impl Widget for ScrollView {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<ScrollView>().is_some() {
            DiffAction::RecurseChildren
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        let mut style = Style {
            overflow: taffy::Point {
                x: taffy::Overflow::Scroll,
                y: taffy::Overflow::Scroll,
            },
            ..Style::default()
        };
        crate::declarative::layout::apply_flex_props(&mut style, &self.flex);
        style
    }

    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        state: &mut StateArena,
        bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        if ctx.input.scroll_delta.y() != 0.0 && bounds.contains(ctx.mouse_pos) {
            let mut offset: f32 = state.get(self.scroll_state_id).unwrap_or_default();
            offset -= ctx.input.scroll_delta.y() * 30.0;
            offset = offset.max(0.0);
            state.set(self.scroll_state_id, offset);
            return InputResult::Consumed;
        }
        InputResult::Ignore
    }

    fn draw(
        &self,
        ctx: &mut UiContext,
        _state: &StateArena,
        bounds: Rect2D,
        _animation: &AnimationState,
        _children: &[ViewId],
        _interaction: &DrawInteraction,
        _view_id: ViewId,
        _children_bounds: &[Rect2D],
    ) {
        let bg = ctx.style().window_bg;
        ctx.draw_rect(bounds, bg);
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

    fn take_children(&mut self) -> ChildWidgets {
        if let Some(child) = self.child_widget.take() {
            ChildWidgets::Single(child)
        } else {
            ChildWidgets::None
        }
    }

    fn needs_clip_children(&self) -> bool {
        true
    }

    fn scroll_offset(&self, state: &StateArena) -> f32 {
        state.get(self.scroll_state_id).unwrap_or_default()
    }

    fn interactive(&self) -> bool {
        true
    }
}

impl ScrollView {
    pub fn flex_width(mut self, w: f32) -> Self {
        self.flex.width = Some(w);
        self
    }
    pub fn flex_height(mut self, h: f32) -> Self {
        self.flex.height = Some(h);
        self
    }
    pub fn flex_grow(mut self, grow: f32) -> Self {
        self.flex.flex_grow = grow;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scroll() -> ScrollView {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let state_id = arena.get_or_create(view_id, 0.0f32);
        ScrollView::new(state_id, FlexProps::default(), None)
    }

    #[test]
    fn test_scroll_view_diff_same_type() {
        let a = make_scroll();
        let b = make_scroll();
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_scroll_view_diff_different_type() {
        let scroll = make_scroll();
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(scroll.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_scroll_view_children() {
        let mut scroll = make_scroll();
        assert!(scroll.children().is_empty());
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        scroll.children_mut().push(view_id);
        assert_eq!(scroll.children().len(), 1);
    }

    #[test]
    fn test_scroll_view_handle_input_consumes_scroll() {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let state_id = arena.get_or_create(view_id, 0.0f32);

        let scroll = ScrollView::new(state_id, FlexProps::default(), None);

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(katla_math::Vec2::new(50.0, 50.0));
        input.scroll_delta = katla_math::Vec2::new(0.0, -3.0);

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: katla_math::Vec2::new(50.0, 50.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
        };

        let bounds = Rect2D::new(
            katla_math::Vec2::new(0.0, 0.0),
            katla_math::Vec2::new(200.0, 200.0),
        );
        let result = scroll.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let offset: f32 = arena.get(state_id).unwrap_or_default();
        assert!(
            offset > 0.0,
            "scroll offset should be positive after scrolling"
        );
    }
}
