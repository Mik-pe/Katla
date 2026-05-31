use std::any::Any;

use katla_math::Rect2D;
use taffy::Style;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;

pub struct Empty;

impl Widget for Empty {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Empty>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        Style::default()
    }

    fn handle_input(
        &self,
        _ctx: &mut InputContext<'_>,
        _state: &mut StateArena,
        _bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        InputResult::Ignore
    }

    fn draw(
        &self,
        _ctx: &mut UiContext,
        _state: &StateArena,
        _bounds: Rect2D,
        _animation: &AnimationState,
        _children: &[ViewId],
        _interaction: &DrawInteraction,
        _view_id: ViewId,
        _children_bounds: &[Rect2D],
    ) {
    }

    fn focusable(&self) -> bool {
        false
    }
}
