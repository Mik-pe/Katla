use std::any::Any;

use katla_math::{Color, Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;

pub(crate) struct VuMeter {
    pub peak_db: f32,
    pub rms_db: f32,
}

impl Widget for VuMeter {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<VuMeter>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        Style {
            size: Size {
                width: Dimension::Length(12.0),
                height: Dimension::Length(120.0),
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
        let track_color = ctx.style().slider_track;

        let db_to_t = |db: f32| (db + 60.0).clamp(0.0, 60.0) / 60.0;

        let rms_t = db_to_t(self.rms_db);
        let peak_t = db_to_t(self.peak_db);

        ctx.draw_rounded_rect(bounds, track_color, 2.0);

        let fill_height = rms_t * bounds.height();
        if fill_height > 0.0 {
            let fill_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), bounds.max.y() - fill_height),
                Vec2::new(bounds.width(), fill_height),
            );

            let bar_color = if self.rms_db >= -3.0 {
                Color::new(0.9, 0.15, 0.15, 1.0)
            } else if self.rms_db >= -12.0 {
                Color::new(0.9, 0.75, 0.1, 1.0)
            } else {
                Color::new(0.2, 0.8, 0.2, 1.0)
            };

            ctx.draw_rounded_rect(fill_bounds, bar_color, 2.0);
        }

        if peak_t > 0.0 {
            let peak_y = bounds.max.y() - peak_t * bounds.height();
            let peak_color = if self.peak_db >= -3.0 {
                Color::new(1.0, 0.3, 0.3, 1.0)
            } else if self.peak_db >= -12.0 {
                Color::new(1.0, 0.9, 0.3, 1.0)
            } else {
                Color::new(0.5, 1.0, 0.5, 1.0)
            };
            ctx.draw_line(
                Vec2::new(bounds.min.x(), peak_y),
                Vec2::new(bounds.max.x(), peak_y),
                peak_color,
                2.0,
            );
        }
    }

    fn focusable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vu(peak_db: f32, rms_db: f32) -> VuMeter {
        VuMeter { peak_db, rms_db }
    }

    #[test]
    fn test_vu_meter_diff_same_type() {
        let a = make_vu(-3.0, -6.0);
        let b = make_vu(-1.0, -4.0);
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }

    #[test]
    fn test_vu_meter_diff_different_type() {
        use super::super::radio::RadioButton;
        use crate::declarative::StateId;

        let vu = make_vu(-3.0, -6.0);
        let radio = RadioButton {
            value_id: StateId::test_id(),
            index: 0,
            label: "A".to_string(),
        };
        assert_eq!(vu.diff_against(&radio), DiffAction::Replace);
    }

    #[test]
    fn test_vu_meter_not_focusable() {
        let vu = make_vu(-3.0, -6.0);
        assert!(!vu.focusable());
    }
}
