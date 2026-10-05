use std::any::Any;

use katla_math::{Color, Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;
use crate::style::FontSize;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInfo, InputContext, InputResult, MeasureFn, Widget};

pub struct Icon {
    pub icon: char,
    pub size: Option<FontSize>,
    pub color: Option<Color>,
}

impl Widget for Icon {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Icon>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        let font_size = self.size.unwrap_or(FontSize::Medium);
        let h = font_size.to_pixels();
        let w = h;
        Style {
            size: Size {
                width: Dimension::Length(w),
                height: Dimension::Length(h),
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
        _info: &DrawInfo,
    ) {
        let font_size = self
            .size
            .map(|fs| ctx.scaled_font_size(fs))
            .unwrap_or(ctx.style().font_size);
        let icon_color = self.color.unwrap_or(ctx.style().text_color);
        let text_size = ctx.measure_icon(self.icon, font_size);
        let text_pos = Vec2::new(
            bounds.center().x() - text_size.x() * 0.5,
            bounds.center().y() - text_size.y() * 0.5,
        );
        ctx.draw_icon(
            self.icon,
            text_pos,
            font_size,
            animation.apply_to_color(icon_color),
        );
    }

    fn focusable(&self) -> bool {
        false
    }
}

impl Icon {
    pub fn color(mut self, color: impl Into<katla_math::Color>) -> Self {
        self.color = Some(color.into());
        self
    }
    pub fn icon_size(mut self, size: crate::style::FontSize) -> Self {
        self.size = Some(size);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::constructors::text;

    #[test]
    fn test_icon_diff() {
        let a = Icon {
            icon: 'A',
            size: None,
            color: None,
        };
        let b = Icon {
            icon: 'B',
            size: Some(FontSize::Large),
            color: Some(Color::WHITE),
        };
        assert_eq!(b.diff_against(&a), DiffAction::Update);

        let other = text("hello");
        assert_eq!(a.diff_against(&other), DiffAction::Replace);
    }
}
