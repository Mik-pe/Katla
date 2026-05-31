use std::any::Any;

use katla_math::{Color, Rect2D};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;
use crate::style::FontSize;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};

pub(crate) struct Text {
    pub content: String,
    pub color: Option<Color>,
    pub font_size: Option<FontSize>,
}

impl Widget for Text {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Text>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, measure: MeasureFn<'_>) -> Style {
        let size = measure(&self.content, self.font_size);
        Style {
            size: Size {
                width: Dimension::Length(size.x()),
                height: Dimension::Length(size.y()),
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
        animation: &AnimationState,
        _children: &[ViewId],
        _interaction: &DrawInteraction,
        _view_id: ViewId,
        _children_bounds: &[Rect2D],
    ) {
        let text_color = self.color.unwrap_or(ctx.style().text_color);
        let size = self
            .font_size
            .map(|fs| ctx.scaled_font_size(fs))
            .unwrap_or(ctx.style().font_size);
        ctx.draw_text(
            &self.content,
            bounds.min,
            animation.apply_to_color(text_color),
            size,
        );
    }

    fn focusable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::constructors::text;

    #[test]
    fn test_text_diff_same_type() {
        let a = Text {
            content: "hello".into(),
            color: None,
            font_size: None,
        };
        let b = Text {
            content: "world".into(),
            color: None,
            font_size: None,
        };
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }

    #[test]
    fn test_text_diff_different_type() {
        let widget = Text {
            content: "hello".into(),
            color: None,
            font_size: None,
        };
        let other = text("other");
        assert_eq!(widget.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_text_draw() {
        let mut ctx = UiContext::new();
        let state = StateArena::new();
        let anim = AnimationState::default();
        let widget = Text {
            content: "hello".into(),
            color: Some(Color::WHITE),
            font_size: Some(FontSize::Medium),
        };
        let bounds = Rect2D::new(
            katla_math::Vec2::new(0.0, 0.0),
            katla_math::Vec2::new(100.0, 20.0),
        );
        widget.draw(
            &mut ctx,
            &state,
            bounds,
            &anim,
            &[],
            &DrawInteraction {
                hovered_id: None,
                active_id: None,
                focused_id: None,
            },
            ViewId::default(),
            &[],
        );
    }
}
