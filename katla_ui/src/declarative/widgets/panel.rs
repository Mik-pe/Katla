use std::any::Any;

use katla_math::Rect2D;
use taffy::{FlexDirection, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::FlexProps;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{ChildWidgets, DrawInfo, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;

pub struct Panel {
    pub title: String,
    pub header_height: f32,
    pub flex: FlexProps,
    pub child_widget: Option<Box<dyn super::super::widget::Widget>>,
    children: Vec<ViewId>,
}

impl Panel {
    pub fn new(
        title: String,
        header_height: f32,
        flex: FlexProps,
        child_widget: Option<Box<dyn super::super::widget::Widget>>,
    ) -> Self {
        Self {
            title,
            header_height,
            flex,
            child_widget,
            children: Vec::new(),
        }
    }
}

impl Widget for Panel {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Panel>().is_some() {
            DiffAction::RecurseChildren
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        let mut style = Style {
            flex_direction: FlexDirection::Column,
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
        ctx: &mut UiContext,
        _state: &StateArena,
        bounds: Rect2D,
        _animation: &AnimationState,
        _children: &[ViewId],
        _info: &DrawInfo,
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

    fn is_focus_scope(&self) -> bool {
        true
    }
}

impl Panel {
    pub fn header_height(mut self, h: f32) -> Self {
        self.header_height = h;
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
    fn test_panel_diff_same_type() {
        let a = Panel::new("A".into(), 24.0, FlexProps::default(), None);
        let b = Panel::new("B".into(), 32.0, FlexProps::default(), None);
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_panel_diff_different_type() {
        let panel = Panel::new("Title".into(), 24.0, FlexProps::default(), None);
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(panel.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_panel_children() {
        let mut panel = Panel::new("Title".into(), 24.0, FlexProps::default(), None);
        assert!(panel.children().is_empty());
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        panel.children_mut().push(view_id);
        assert_eq!(panel.children().len(), 1);
    }

    #[test]
    fn test_panel_layout_style_column() {
        let panel = Panel::new("Title".into(), 24.0, FlexProps::default(), None);
        let style = panel.layout_style(&crate::declarative::layout::measure_text_descriptor);
        assert_eq!(style.flex_direction, FlexDirection::Column);
    }
}
