use std::any::Any;

use katla_math::{Color, Rect2D, Vec2};
use taffy::Style;

use crate::context::UiContext;

use super::super::animation::AnimationState;
use super::super::descriptor::SeparatorDirection;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{InputContext, InputResult, Widget};

pub(crate) struct Separator {
    pub direction: SeparatorDirection,
    pub color: Option<Color>,
}

impl Widget for Separator {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Separator>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self) -> Style {
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
        ctx: &mut UiContext,
        _state: &StateArena,
        bounds: Rect2D,
        animation: &AnimationState,
        _children: &[ViewId],
    ) {
        let line_color = self.color.unwrap_or(ctx.style().separator);
        match self.direction {
            SeparatorDirection::Horizontal => {
                let y = bounds.center().y();
                ctx.draw_line(
                    Vec2::new(bounds.min.x(), y),
                    Vec2::new(bounds.max.x(), y),
                    animation.apply_to_color(line_color),
                    1.0,
                );
            }
            SeparatorDirection::Vertical => {
                let x = bounds.center().x();
                ctx.draw_line(
                    Vec2::new(x, bounds.min.y()),
                    Vec2::new(x, bounds.max.y()),
                    animation.apply_to_color(line_color),
                    1.0,
                );
            }
        }
    }

    fn focusable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::constructors::text;
    use crate::declarative::widget::DescriptorWidget;

    #[test]
    fn test_separator_diff() {
        let a = Separator {
            direction: SeparatorDirection::Horizontal,
            color: None,
        };
        let b = Separator {
            direction: SeparatorDirection::Vertical,
            color: Some(Color::WHITE),
        };
        assert_eq!(b.diff_against(&a), DiffAction::Update);

        let other = DescriptorWidget::new(text("hello"));
        assert_eq!(a.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_separator_layout_style() {
        let sep = Separator {
            direction: SeparatorDirection::Horizontal,
            color: None,
        };
        let style = sep.layout_style();
        assert_eq!(style, Style::default());
    }
}
