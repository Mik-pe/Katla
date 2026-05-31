use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, FlexDirection, LengthPercentage, Size, Style};

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{
    ChildWidgets, DrawInteraction, InputContext, InputResult, MeasureFn, Widget,
};
use crate::context::UiContext;

pub struct StatusBar {
    pub height: f32,
    pub child_widget: Option<Box<dyn super::super::widget::Widget>>,
    children: Vec<ViewId>,
}

impl StatusBar {
    pub fn new(height: f32, child_widget: Option<Box<dyn super::super::widget::Widget>>) -> Self {
        Self {
            height,
            child_widget,
            children: Vec::new(),
        }
    }
}

impl Widget for StatusBar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<StatusBar>().is_some() {
            DiffAction::RecurseChildren
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        Style {
            size: Size {
                width: Dimension::Percent(1.0),
                height: Dimension::Length(self.height),
            },
            flex_direction: FlexDirection::Column,
            padding: taffy::Rect {
                top: LengthPercentage::Length(0.0),
                right: LengthPercentage::Length(0.0),
                bottom: LengthPercentage::Length(0.0),
                left: LengthPercentage::Length(0.0),
            },
            ..Style::default()
        }
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
        ctx: &mut UiContext,
        _state: &StateArena,
        bounds: Rect2D,
        _animation: &AnimationState,
        _children: &[ViewId],
        _interaction: &DrawInteraction,
        _view_id: ViewId,
        _children_bounds: &[Rect2D],
    ) {
        ctx.draw_line(
            Vec2::new(bounds.min.x(), bounds.min.y()),
            Vec2::new(bounds.max.x(), bounds.min.y()),
            ctx.style().separator,
            1.0,
        );
        ctx.draw_rect(bounds, ctx.style().window_bg);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statusbar_diff_same_type() {
        let a = StatusBar::new(24.0, None);
        let b = StatusBar::new(32.0, None);
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_statusbar_diff_different_type() {
        let sb = StatusBar::new(24.0, None);
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(sb.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_statusbar_children() {
        let mut sb = StatusBar::new(24.0, None);
        assert!(sb.children().is_empty());
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        sb.children_mut().push(view_id);
        assert_eq!(sb.children().len(), 1);
    }

    #[test]
    fn test_statusbar_layout_style() {
        let sb = StatusBar::new(24.0, None);
        let style = sb.layout_style(&crate::declarative::layout::measure_text_descriptor);
        assert!(matches!(style.size.width, Dimension::Percent(1.0)));
        assert!(matches!(style.size.height, Dimension::Length(24.0)));
    }
}
