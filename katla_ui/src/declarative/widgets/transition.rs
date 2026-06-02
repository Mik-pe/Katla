use std::any::Any;

use katla_math::Rect2D;
use taffy::Style;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::transition::Transition;
use super::super::widget::{
    ChildWidgets, DrawInfo, InputContext, InputResult, MeasureFn, Widget,
};
use crate::context::UiContext;

pub struct TransitionContainer {
    pub transition: Transition,
    pub child_widget: Option<Box<dyn super::super::widget::Widget>>,
    children: Vec<ViewId>,
}

impl TransitionContainer {
    pub fn new(
        transition: Transition,
        child_widget: Option<Box<dyn super::super::widget::Widget>>,
    ) -> Self {
        Self {
            transition,
            child_widget,
            children: Vec::new(),
        }
    }
}

impl Widget for TransitionContainer {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev
            .as_any()
            .downcast_ref::<TransitionContainer>()
            .is_some()
        {
            DiffAction::RecurseChildren
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
        _info: &DrawInfo,
    ) {
        // TransitionContainer has no visual chrome — animations are applied
        // via the AnimationState on the child node during the draw pipeline
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
            ChildWidgets::Transition {
                child,
                transition: self.transition.clone(),
            }
        } else {
            ChildWidgets::None
        }
    }

    fn as_transition(&self) -> Option<&Transition> {
        Some(&self.transition)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_transition() -> TransitionContainer {
        TransitionContainer::new(Transition::fade(0.3), None)
    }

    #[test]
    fn test_transition_diff_same_type() {
        let a = make_transition();
        let b = make_transition();
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_transition_diff_different_type() {
        let tc = make_transition();
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(tc.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_transition_children() {
        let mut tc = make_transition();
        assert!(tc.children().is_empty());
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        tc.children_mut().push(view_id);
        assert_eq!(tc.children().len(), 1);
    }

    #[test]
    fn test_transition_has_transition_config() {
        let tc = make_transition();
        assert!(tc.transition.insert.is_some() || tc.transition.remove.is_some());
    }
}
