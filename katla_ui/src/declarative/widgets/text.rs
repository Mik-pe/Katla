use std::any::Any;

use katla_math::{Color, Rect2D};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;
use crate::style::FontSize;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInfo, InputContext, InputResult, MeasureFn, Widget};

pub struct Text {
    pub content: String,
    pub color: Option<Color>,
    pub font_size: Option<FontSize>,
    /// Fixed wrapping width in logical pixels; explicit newlines are preserved.
    pub wrap_width: Option<f32>,
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
        let size = measure(&self.content, self.font_size, self.wrap_width);
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
        _info: &DrawInfo,
    ) {
        let text_color = self.color.unwrap_or(ctx.style().text_color);
        let size = self
            .font_size
            .map(|fs| ctx.scaled_font_size(fs))
            .unwrap_or(ctx.style().font_size);
        ctx.draw_text_with_width(
            &self.content,
            bounds.min,
            animation.apply_to_color(text_color),
            size,
            self.wrap_width,
        );
    }

    fn focusable(&self) -> bool {
        false
    }
}

impl Text {
    /// Wrap words and long tokens at a fixed width using the active font metrics.
    pub fn wrap(mut self, width: f32) -> Self {
        self.wrap_width = Some(width.max(1.0));
        self
    }

    pub fn color(mut self, color: impl Into<katla_math::Color>) -> Self {
        self.color = Some(color.into());
        self
    }
    pub fn font_size(mut self, fs: crate::style::FontSize) -> Self {
        self.font_size = Some(fs);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::widget::DrawInteraction;

    #[test]
    fn test_wrapped_layout_reuses_unicode_shaping_and_preserves_message() {
        let mut fonts = crate::text::FontSystem::new();
        fonts
            .add_font(include_bytes!(
                "../../../../resources/fonts/roboto-regular.ttf"
            ))
            .unwrap();
        let widget =
            crate::declarative::constructors::text("The left door stays clear.\n\nRoom: äö界 🪑")
                .wrap(80.0);
        let fonts = std::cell::RefCell::new(fonts);
        let measure = |content: &str, _: Option<FontSize>, width: Option<f32>| {
            fonts.borrow_mut().measure_text_shaped(
                crate::FontId::DEFAULT,
                content,
                12.0,
                1.0,
                width,
            )
        };
        let style = widget.layout_style(&measure);
        let shaped = fonts
            .borrow_mut()
            .shape_text(
                crate::FontId::DEFAULT,
                &widget.content,
                12.0,
                1.0,
                Some(80.0),
            )
            .unwrap();
        let (width, height) = shaped.dimensions();
        assert_eq!(style.size.width, Dimension::Length(width));
        assert_eq!(style.size.height, Dimension::Length(height));
        assert!(height > 40.0);
        assert!(width <= 80.01);
        assert_eq!(
            widget.content,
            "The left door stays clear.\n\nRoom: äö界 🪑"
        );
    }

    #[test]
    fn test_text_diff_same_type() {
        let a = Text {
            content: "hello".into(),
            color: None,
            font_size: None,
            wrap_width: None,
        };
        let b = Text {
            content: "world".into(),
            color: None,
            font_size: None,
            wrap_width: None,
        };
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }

    #[test]
    fn test_text_diff_different_type() {
        let widget = Text {
            content: "hello".into(),
            color: None,
            font_size: None,
            wrap_width: None,
        };
        // Use a different widget type (Button) to test Replace
        let other = crate::declarative::constructors::button("other");
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
            wrap_width: None,
        };
        let bounds = Rect2D::new(
            katla_math::Vec2::new(0.0, 0.0),
            katla_math::Vec2::new(100.0, 20.0),
        );
        let info = DrawInfo {
            interaction: &DrawInteraction {
                hovered_id: None,
                active_id: None,
                focused_id: None,
            },
            view_id: ViewId::default(),
            children_bounds: &[],
        };
        widget.draw(&mut ctx, &state, bounds, &anim, &[], &info);
    }
}
