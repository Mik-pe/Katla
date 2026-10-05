use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Position, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::Anchor;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{ChildWidgets, DrawInfo, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;

pub struct Overlay {
    pub anchor: Anchor,
    pub offset: Vec2,
    pub child_widget: Option<Box<dyn super::super::widget::Widget>>,
    children: Vec<ViewId>,
}

impl Overlay {
    pub fn new(
        anchor: Anchor,
        offset: Vec2,
        child_widget: Option<Box<dyn super::super::widget::Widget>>,
    ) -> Self {
        Self {
            anchor,
            offset,
            child_widget,
            children: Vec::new(),
        }
    }

    pub fn resolve_position(
        anchor: Anchor,
        offset: Vec2,
        parent_bounds: Rect2D,
        content_bounds: Rect2D,
    ) -> Rect2D {
        let pw = parent_bounds.width();
        let ph = parent_bounds.height();
        let cw = content_bounds.width();
        let ch = content_bounds.height();

        let pos = match anchor {
            Anchor::TopLeft => parent_bounds.min,
            Anchor::TopRight => Vec2::new(parent_bounds.max.x() - cw, parent_bounds.min.y()),
            Anchor::BottomLeft => Vec2::new(parent_bounds.min.x(), parent_bounds.max.y() - ch),
            Anchor::BottomRight => {
                Vec2::new(parent_bounds.max.x() - cw, parent_bounds.max.y() - ch)
            }
            Anchor::TopCenter => Vec2::new(
                parent_bounds.min.x() + (pw - cw) * 0.5,
                parent_bounds.min.y(),
            ),
            Anchor::BottomCenter => Vec2::new(
                parent_bounds.min.x() + (pw - cw) * 0.5,
                parent_bounds.max.y() - ch,
            ),
            Anchor::Center => Vec2::new(
                parent_bounds.min.x() + (pw - cw) * 0.5,
                parent_bounds.min.y() + (ph - ch) * 0.5,
            ),
        };

        let origin = pos + offset;
        Rect2D::new(origin, Vec2::new(origin.x() + cw, origin.y() + ch))
    }
}

impl Widget for Overlay {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Overlay>().is_some() {
            DiffAction::RecurseChildren
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        Style {
            position: Position::Absolute,
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
        if let Some(child) = self.child_widget.take() {
            ChildWidgets::Single(child)
        } else {
            ChildWidgets::None
        }
    }

    fn resolve_position_delta(
        &self,
        bounds: Rect2D,
        parent_bounds: Rect2D,
        _zstack_alignment: Option<super::super::descriptor::Alignment>,
        _state: &StateArena,
    ) -> Vec2 {
        let resolved = Self::resolve_position(self.anchor, self.offset, parent_bounds, bounds);
        resolved.min - bounds.min
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_diff_same_type() {
        let a = Overlay::new(Anchor::TopLeft, Vec2::ZERO, None);
        let b = Overlay::new(Anchor::TopRight, Vec2::new(10.0, 5.0), None);
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_overlay_diff_different_type() {
        let overlay = Overlay::new(Anchor::Center, Vec2::ZERO, None);
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(overlay.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_overlay_children() {
        let mut overlay = Overlay::new(Anchor::TopLeft, Vec2::ZERO, None);
        assert!(overlay.children().is_empty());
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        overlay.children_mut().push(view_id);
        assert_eq!(overlay.children().len(), 1);
    }

    #[test]
    fn test_overlay_anchor_positioning() {
        let parent = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        let content = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0));

        let top_right =
            Overlay::resolve_position(Anchor::TopRight, Vec2::new(10.0, 5.0), parent, content);
        assert_eq!(top_right.min.x(), 800.0 - 100.0 + 10.0);
        assert_eq!(top_right.min.y(), 5.0);

        let bottom_left =
            Overlay::resolve_position(Anchor::BottomLeft, Vec2::ZERO, parent, content);
        assert_eq!(bottom_left.min.x(), 0.0);
        assert_eq!(bottom_left.min.y(), 600.0 - 50.0);

        let center = Overlay::resolve_position(Anchor::Center, Vec2::ZERO, parent, content);
        assert_eq!(center.min.x(), (800.0 - 100.0) * 0.5);
        assert_eq!(center.min.y(), (600.0 - 50.0) * 0.5);
    }

    #[test]
    fn test_overlay_layout_style_absolute() {
        let overlay = Overlay::new(Anchor::TopLeft, Vec2::ZERO, None);
        let style = overlay.layout_style(&crate::declarative::layout::measure_text_descriptor);
        assert_eq!(style.position, Position::Absolute);
    }
}
