use std::any::Any;
use std::ops::RangeInclusive;

use katla_math::{Color, Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInfo, InputContext, InputResult, MeasureFn, Widget};

pub struct Progress {
    pub value: f32,
    pub range: RangeInclusive<f32>,
    pub fill_color: Option<Color>,
    pub label: Option<String>,
}

impl Widget for Progress {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Self>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        Style {
            size: Size {
                width: Dimension::Length(100.0),
                height: Dimension::Length(8.0),
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
        let t = if *self.range.end() > *self.range.start() {
            (self.value - *self.range.start()) / (*self.range.end() - *self.range.start())
        } else {
            0.0
        };

        let track_color = ctx.style().slider_track;
        let bar_color = self.fill_color.unwrap_or(ctx.style().slider_grab);

        ctx.draw_rounded_rect(bounds, track_color, bounds.height() * 0.5);

        let fill_width = t * bounds.width();
        if fill_width > 0.0 {
            let fill_bounds =
                Rect2D::from_origin_size(bounds.min, Vec2::new(fill_width, bounds.height()));
            ctx.draw_rounded_rect(
                fill_bounds,
                animation.apply_to_color(bar_color),
                bounds.height() * 0.5,
            );
        }

        if let Some(ref label_text) = self.label {
            let font_size = ctx.style().font_size;
            let text_size = ctx.measure_text(label_text, font_size);
            let text_pos = Vec2::new(
                bounds.center().x() - text_size.x() * 0.5,
                bounds.center().y() - text_size.y() * 0.5,
            );
            ctx.draw_text(label_text, text_pos, ctx.style().button_text, font_size);
        }
    }

    fn focusable(&self) -> bool {
        false
    }
}

impl Progress {
    pub fn fill(mut self, color: impl Into<katla_math::Color>) -> Self {
        self.fill_color = Some(color.into());
        self
    }
    pub fn progress_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::diff::DiffAction;
    use crate::declarative::state::StateId;
    use crate::declarative::widget::Widget;
    use crate::declarative::widgets::labeled_slider::LabeledSlider;

    fn make_progress(value: f32) -> Progress {
        Progress {
            value,
            range: 0.0..=100.0,
            fill_color: None,
            label: None,
        }
    }

    #[test]
    fn test_progress_fill_proportion() {
        let p = make_progress(50.0);
        let range_start = *p.range.start();
        let range_end = *p.range.end();
        let t = (p.value - range_start) / (range_end - range_start);
        assert!((t - 0.5).abs() < 1e-4, "50/100 should be 0.5");
    }

    #[test]
    fn test_progress_fill_zero() {
        let p = make_progress(0.0);
        let t = (p.value - *p.range.start()) / (*p.range.end() - *p.range.start());
        assert!((t - 0.0).abs() < 1e-4);
    }

    #[test]
    fn test_progress_fill_full() {
        let p = make_progress(100.0);
        let t = (p.value - *p.range.start()) / (*p.range.end() - *p.range.start());
        assert!((t - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_progress_diff() {
        let a = make_progress(25.0);
        let b = make_progress(75.0);
        assert_eq!(b.diff_against(&a), DiffAction::Update);

        let slider = LabeledSlider {
            label: "test".into(),
            value_id: StateId::test_id(),
            range: 0.0..=1.0,
            label_width: 80.0,
            show_value: false,
            precision: 2,
            value_multiplier: 1.0,
            value_suffix: String::new(),
        };
        assert_eq!(a.diff_against(&slider), DiffAction::Replace);
    }

    #[test]
    fn test_progress_not_focusable() {
        let p = make_progress(50.0);
        assert!(!p.focusable());
    }

    #[test]
    fn test_progress_ignore_input() {
        let p = make_progress(50.0);
        let mut state = StateArena::new();

        let input = crate::input::UiInputState::new();
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 20.0));

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = crate::declarative::widget::InputContext {
            input: &input,
            mouse_pos: Vec2::new(100.0, 10.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
            focused_id: None,
        };

        let result = p.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, crate::declarative::widget::InputResult::Ignore);
    }
}
