use std::any::Any;

use katla_math::Rect2D;
use taffy::{FlexDirection, LengthPercentage, Size, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::{Alignment, FlexProps, Padding};
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{
    ChildWidgets, DrawInteraction, InputContext, InputResult, MeasureFn, Widget,
};
use crate::context::UiContext;

pub(crate) struct HStack {
    pub spacing: f32,
    pub padding: Padding,
    pub alignment: Alignment,
    pub flex: FlexProps,
    pub child_widgets: Vec<super::super::constructors::KeyedChild>,
    children: Vec<ViewId>,
}

impl HStack {
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

impl Widget for HStack {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<HStack>().is_some() {
            DiffAction::RecurseChildren
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        let mut style = Style {
            flex_direction: FlexDirection::Row,
            gap: Size {
                width: LengthPercentage::Length(self.spacing),
                height: LengthPercentage::Length(0.0),
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
        // HStack has no chrome — children are positioned by taffy layout
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
        let children: Vec<(Option<u64>, Box<dyn Widget>)> = self
            .child_widgets
            .drain(..)
            .map(|kc| (kc.key, kc.widget))
            .collect();
        ChildWidgets::Multi(children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hstack_diff_same_type() {
        let a = HStack::new(
            0.0,
            Padding::zero(),
            Alignment::Leading,
            FlexProps::default(),
            vec![],
        );
        let b = HStack::new(
            8.0,
            Padding::all(4.0),
            Alignment::Center,
            FlexProps::default(),
            vec![],
        );
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_hstack_diff_different_type() {
        let hstack = HStack::new(
            0.0,
            Padding::zero(),
            Alignment::Leading,
            FlexProps::default(),
            vec![],
        );
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(hstack.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_hstack_children() {
        let mut hstack = HStack::new(
            0.0,
            Padding::zero(),
            Alignment::Leading,
            FlexProps::default(),
            vec![],
        );
        assert!(hstack.children().is_empty());

        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        hstack.children_mut().push(view_id);
        assert_eq!(hstack.children().len(), 1);
    }

    #[test]
    fn test_hstack_layout_style_horizontal() {
        let hstack = HStack::new(
            8.0,
            Padding::zero(),
            Alignment::Leading,
            FlexProps::default(),
            vec![],
        );
        let style = hstack.layout_style(&crate::declarative::layout::measure_text_descriptor);
        assert_eq!(style.flex_direction, FlexDirection::Row);
    }

    #[test]
    fn test_hstack_not_focusable() {
        let hstack = HStack::new(
            0.0,
            Padding::zero(),
            Alignment::Leading,
            FlexProps::default(),
            vec![],
        );
        assert!(!hstack.focusable());
    }
}
