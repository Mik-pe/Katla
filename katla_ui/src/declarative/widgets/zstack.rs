use std::any::Any;

use katla_math::Rect2D;
use taffy::{Dimension, Size, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::{Alignment, FlexProps, Padding};
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{
    ChildWidgets, DrawInfo, InputContext, InputResult, MeasureFn, Widget,
};
use crate::context::UiContext;

pub struct ZStack {
    pub padding: Padding,
    pub flex: FlexProps,
    pub child_widgets: Vec<(Alignment, super::super::constructors::KeyedChild)>,
    children: Vec<ViewId>,
}

impl ZStack {
    pub fn new(
        padding: Padding,
        flex: FlexProps,
        child_widgets: Vec<(Alignment, super::super::constructors::KeyedChild)>,
    ) -> Self {
        Self {
            padding,
            flex,
            child_widgets,
            children: Vec::new(),
        }
    }
}

impl Widget for ZStack {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<ZStack>().is_some() {
            DiffAction::RecurseChildren
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        let mut style = Style {
            size: Size {
                width: Dimension::Percent(1.0),
                height: Dimension::Percent(1.0),
            },
            padding: crate::declarative::layout::padding_to_taffy(&self.padding),
            ..Style::default()
        };
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
        let children: Vec<(
            super::super::descriptor::Alignment,
            Option<u64>,
            Box<dyn Widget>,
        )> = self
            .child_widgets
            .drain(..)
            .map(|(alignment, kc)| (alignment, kc.key, kc.widget))
            .collect();
        ChildWidgets::ZStack(children)
    }
}

impl ZStack {
    pub fn padding(mut self, p: super::super::descriptor::Padding) -> Self {
        self.padding = p;
        self
    }
    pub fn padding_all(mut self, v: f32) -> Self {
        self.padding = super::super::descriptor::Padding::all(v);
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
    fn test_zstack_diff_same_type() {
        let a = ZStack::new(Padding::zero(), FlexProps::default(), vec![]);
        let b = ZStack::new(Padding::all(4.0), FlexProps::default(), vec![]);
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_zstack_diff_different_type() {
        let zstack = ZStack::new(Padding::zero(), FlexProps::default(), vec![]);
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(zstack.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_zstack_children() {
        let mut zstack = ZStack::new(Padding::zero(), FlexProps::default(), vec![]);
        assert!(zstack.children().is_empty());

        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        zstack.children_mut().push(view_id);
        assert_eq!(zstack.children().len(), 1);
    }

    #[test]
    fn test_zstack_layout_style_fills_parent() {
        let zstack = ZStack::new(Padding::zero(), FlexProps::default(), vec![]);
        let style = zstack.layout_style(&crate::declarative::layout::measure_text_descriptor);
        assert!(matches!(style.size.width, Dimension::Percent(1.0)));
        assert!(matches!(style.size.height, Dimension::Percent(1.0)));
    }
}
