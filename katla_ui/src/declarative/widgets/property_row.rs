use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInfo, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;

pub struct PropertyRow {
    pub label: String,
    pub value: String,
}

impl Widget for PropertyRow {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<PropertyRow>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, measure: MeasureFn<'_>) -> Style {
        let label_size = measure(&self.label, None);
        let value_size = measure(&self.value, None);
        Style {
            size: Size {
                width: Dimension::Length(label_size.x() + value_size.x() + 16.0),
                height: Dimension::Length(30.0),
            },
            padding: taffy::Rect {
                top: taffy::LengthPercentage::Length(6.0),
                right: taffy::LengthPercentage::Length(0.0),
                bottom: taffy::LengthPercentage::Length(6.0),
                left: taffy::LengthPercentage::Length(0.0),
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
        let font_size = ctx.style().font_size;

        let label_size = ctx.measure_text(&self.label, font_size);
        let label_y = bounds.center().y() - label_size.y() * 0.5;
        ctx.draw_text(
            &self.label,
            Vec2::new(bounds.min.x(), label_y),
            animation.apply_to_color(ctx.style().tab_text),
            font_size,
        );

        let value_size = ctx.measure_text(&self.value, font_size);
        let value_x = bounds.max.x() - value_size.x();
        let value_y = bounds.center().y() - value_size.y() * 0.5;
        ctx.draw_text(
            &self.value,
            Vec2::new(value_x, value_y),
            animation.apply_to_color(ctx.style().text_color),
            font_size,
        );
    }

    fn focusable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_row(label: &str, value: &str) -> PropertyRow {
        PropertyRow {
            label: label.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn test_property_row_diff_same_type() {
        let a = make_row("Name", "Cube");
        let b = make_row("Type", "Mesh");
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }

    #[test]
    fn test_property_row_diff_different_type() {
        use super::super::radio::RadioButton;
        use crate::declarative::StateId;

        let row = make_row("Name", "Cube");
        let radio = RadioButton {
            value_id: StateId::test_id(),
            index: 0,
            label: "A".to_string(),
        };
        assert_eq!(row.diff_against(&radio), DiffAction::Replace);
    }

    #[test]
    fn test_property_row_not_focusable() {
        let row = make_row("Name", "Cube");
        assert!(!row.focusable());
    }
}
