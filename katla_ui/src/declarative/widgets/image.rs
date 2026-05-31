use std::any::Any;

use katla_math::{Color, Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;
use crate::types::TextureId;

pub struct Image {
    pub texture: TextureId,
    pub uv: Option<Rect2D>,
    pub tint: Color,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

impl Widget for Image {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Image>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        let w = self.width.unwrap_or(64.0);
        let h = self.height.unwrap_or(64.0);
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
        _animation: &AnimationState,
        _children: &[ViewId],
        _interaction: &DrawInteraction,
        _view_id: ViewId,
        _children_bounds: &[Rect2D],
    ) {
        let uv_rect = self
            .uv
            .unwrap_or_else(|| Rect2D::new(Vec2::ZERO, Vec2::new(1.0, 1.0)));
        ctx.draw_image(bounds, uv_rect.min, uv_rect.max, self.tint, self.texture);
    }

    fn focusable(&self) -> bool {
        false
    }
}

impl Image {
    pub fn uv(mut self, rect: katla_math::Rect2D) -> Self {
        self.uv = Some(rect);
        self
    }
    pub fn image_size(mut self, width: f32, height: f32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_image(texture: u64) -> Image {
        Image {
            texture: TextureId(texture),
            uv: None,
            tint: Color::WHITE,
            width: None,
            height: None,
        }
    }

    #[test]
    fn test_image_diff_same_type() {
        let a = make_image(1);
        let b = make_image(2);
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }

    #[test]
    fn test_image_diff_different_type() {
        use super::super::radio::RadioButton;
        use crate::declarative::StateId;

        let image = make_image(1);
        let radio = RadioButton {
            value_id: StateId::test_id(),
            index: 0,
            label: "A".to_string(),
        };
        assert_eq!(image.diff_against(&radio), DiffAction::Replace);
    }

    #[test]
    fn test_image_not_focusable() {
        let image = make_image(1);
        assert!(!image.focusable());
    }

    #[test]
    fn test_image_layout_style_default() {
        let image = make_image(1);
        let style = image.layout_style(&crate::declarative::layout::measure_text_descriptor);
        assert!(matches!(style.size.width, Dimension::Length(64.0)));
        assert!(matches!(style.size.height, Dimension::Length(64.0)));
    }
}
