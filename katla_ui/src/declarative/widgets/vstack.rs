use std::any::Any;

use katla_math::Rect2D;
use taffy::{FlexDirection, LengthPercentage, Size, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::{Alignment, FlexProps, Padding};
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;

pub(crate) struct VStack {
    pub spacing: f32,
    pub padding: Padding,
    pub alignment: Alignment,
    pub flex: FlexProps,
    pub child_widgets: Vec<super::super::constructors::KeyedChild>,
    children: Vec<ViewId>,
}

impl VStack {
    pub fn new(
        spacing: f32,
        padding: Padding,
        alignment: Alignment,
        flex: FlexProps,
        child_widgets: Vec<super::super::constructors::KeyedChild>,
    ) -> Self {
        Self {
            spacing,
            padding,
            alignment,
            flex,
            child_widgets,
            children: Vec::new(),
        }
    }
}

impl Widget for VStack {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<VStack>().is_some() {
            DiffAction::RecurseChildren
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        let mut style = Style {
            flex_direction: FlexDirection::Column,
            gap: Size {
                width: LengthPercentage::Length(0.0),
                height: LengthPercentage::Length(self.spacing),
            },
            padding: crate::declarative::layout::padding_to_taffy(&self.padding),
            ..Style::default()
        };
        crate::declarative::layout::apply_alignment_to_style(&mut style, self.alignment);
        crate::declarative::layout::apply_flex_props(&mut style, &self.flex);
        style
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

    #[test]
    fn test_vstack_diff_same_type() {
        let a = VStack::new(
            0.0,
            Padding::zero(),
            Alignment::Leading,
            FlexProps::default(),
            vec![],
        );
        let b = VStack::new(
            8.0,
            Padding::all(4.0),
            Alignment::Center,
            FlexProps::default(),
            vec![],
        );
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_vstack_diff_different_type() {
        let vstack = VStack::new(
            0.0,
            Padding::zero(),
            Alignment::Leading,
            FlexProps::default(),
            vec![],
        );
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(vstack.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_vstack_children() {
        let mut vstack = VStack::new(
            0.0,
            Padding::zero(),
            Alignment::Leading,
            FlexProps::default(),
            vec![],
        );
        assert!(vstack.children().is_empty());

        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        vstack.children_mut().push(view_id);
        assert_eq!(vstack.children().len(), 1);
    }

    #[test]
    fn test_vstack_layout_style_vertical() {
        let vstack = VStack::new(
            8.0,
            Padding::zero(),
            Alignment::Leading,
            FlexProps::default(),
            vec![],
        );
        let style = vstack.layout_style(&crate::declarative::layout::measure_text_descriptor);
        assert_eq!(style.flex_direction, FlexDirection::Column);
    }

    #[test]
    fn test_vstack_not_focusable() {
        let vstack = VStack::new(
            0.0,
            Padding::zero(),
            Alignment::Leading,
            FlexProps::default(),
            vec![],
        );
        assert!(!vstack.focusable());
    }
}
