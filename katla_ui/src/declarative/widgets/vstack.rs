use std::any::Any;

use katla_math::Rect2D;
use taffy::{FlexDirection, LengthPercentage, Size, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::{Alignment, FlexProps, Padding};
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{
    ChildWidgets, DrawInfo, InputContext, InputResult, MeasureFn, Widget,
};
use crate::context::UiContext;

pub struct VStack {
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
        _info: &DrawInfo,
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

    fn take_children(&mut self) -> ChildWidgets {
        let children: Vec<(Option<u64>, Box<dyn Widget>)> = self
            .child_widgets
            .drain(..)
            .map(|kc| (kc.key, kc.widget))
            .collect();
        ChildWidgets::Multi(children)
    }
}

impl VStack {
    pub fn spacing(mut self, s: f32) -> Self {
        self.spacing = s;
        self
    }
    pub fn padding(mut self, p: super::super::descriptor::Padding) -> Self {
        self.padding = p;
        self
    }
    pub fn padding_all(mut self, v: f32) -> Self {
        self.padding = super::super::descriptor::Padding::all(v);
        self
    }
    pub fn align(mut self, a: super::super::descriptor::Alignment) -> Self {
        self.alignment = a;
        self
    }
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
