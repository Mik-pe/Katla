use std::any::Any;
use std::ops::RangeInclusive;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;
use crate::input::mouse_button;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{DrawInfo, InputContext, InputResult, MeasureFn, Widget};

pub struct LabeledSlider {
    pub label: String,
    pub value_id: StateId,
    pub range: RangeInclusive<f32>,
    pub label_width: f32,
    pub show_value: bool,
    pub precision: usize,
    /// Multiplier applied to the raw value for display (100 → percent).
    pub value_multiplier: f32,
    /// Suffix appended to the displayed value ("%", " m/s", …).
    pub value_suffix: String,
}

/// Format the value for display using multiplier/precision/suffix.
fn display_value(slider: &LabeledSlider, value: f32) -> String {
    format!(
        "{:.*}{}",
        slider.precision,
        value * slider.value_multiplier,
        slider.value_suffix
    )
}

impl Widget for LabeledSlider {
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

    fn layout_style(&self, measure: MeasureFn<'_>) -> Style {
        let text_size = measure(&self.label, None);
        Style {
            size: Size {
                width: Dimension::Length((text_size.x() + 120.0).max(200.0)),
                height: Dimension::Length((text_size.y() + 12.0).max(24.0)),
            },
            ..Style::default()
        }
    }

    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        state: &mut StateArena,
        bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        if !bounds.contains(ctx.mouse_pos) {
            return InputResult::Ignore;
        }

        if !ctx.input.mouse_down[mouse_button::LEFT] {
            return InputResult::Ignore;
        }

        let track_x = bounds.min.x() + self.label_width;
        let track_width = bounds.max.x() - track_x;
        let t = if track_width > 0.0 {
            ((ctx.mouse_pos.x() - track_x) / track_width).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let new_val = self.range.start() + t * (*self.range.end() - *self.range.start());
        state.set(self.value_id, new_val);
        ctx.active_id = Some(ctx.view_id);

        InputResult::Consumed
    }

    fn draw(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        animation: &AnimationState,
        _children: &[ViewId],
        info: &DrawInfo,
    ) {
        let value: f32 = state.get(self.value_id).unwrap_or_default();
        let t = if *self.range.end() > *self.range.start() {
            ((value - *self.range.start()) / (*self.range.end() - *self.range.start()))
                .clamp(0.0, 1.0)
        } else {
            0.0
        };

        let font_size = ctx.style().font_size;
        let text_color = animation.apply_to_color(ctx.style().text_color);

        let label_size = ctx.measure_text(&self.label, font_size);
        let label_y = bounds.center().y() - label_size.y() * 0.5;
        ctx.draw_text(
            &self.label,
            Vec2::new(bounds.min.x(), label_y),
            text_color,
            font_size,
        );

        let track_x = bounds.min.x() + self.label_width;

        let value_text_width = if self.show_value {
            let value_text = display_value(self, value);
            let size = ctx.measure_text(&value_text, font_size);
            size.x() + 8.0
        } else {
            0.0
        };

        let track_end = bounds.max.x() - value_text_width;
        let track_width = (track_end - track_x).max(0.0);
        let track_height = ctx.style().slider_track_height;
        let track_center_y = bounds.center().y();
        let track_bounds = Rect2D::from_center_size(
            Vec2::new(track_x + track_width * 0.5, track_center_y),
            Vec2::new(track_width, track_height),
        );

        ctx.draw_rounded_rect(track_bounds, ctx.style().slider_track, track_height * 0.5);

        let fill_width = t * track_width;
        if fill_width > 0.0 {
            let fill_bounds =
                Rect2D::from_origin_size(track_bounds.min, Vec2::new(fill_width, track_height));
            ctx.draw_rounded_rect(
                fill_bounds,
                animation.apply_to_color(ctx.style().slider_grab),
                track_height * 0.5,
            );
        }

        // The grab grows slightly while hovered for a clearer affordance.
        let hovered = bounds.contains(ctx.mouse_pos());
        let grab_color = if hovered {
            ctx.style().slider_grab_hovered
        } else {
            ctx.style().slider_grab
        };
        let grab_center_x = track_x + t * track_width;
        let grab_center = Vec2::new(grab_center_x, track_center_y);
        let grab_radius = ctx.style().slider_grab_size * 0.5 + if hovered { 1.0 } else { 0.0 };
        ctx.draw_circle(
            Vec2::new(grab_center.x(), grab_center.y() + 1.0),
            grab_radius,
            katla_math::Color::new(0.0, 0.0, 0.0, 0.3),
        );
        ctx.draw_circle(
            grab_center,
            grab_radius,
            animation.apply_to_color(grab_color),
        );

        if self.show_value {
            let value_text = display_value(self, value);
            let text_size = ctx.measure_text(&value_text, font_size);
            let value_x = bounds.max.x() - text_size.x();
            let value_y = bounds.center().y() - text_size.y() * 0.5;
            ctx.draw_text(
                &value_text,
                Vec2::new(value_x, value_y),
                text_color,
                font_size,
            );
        }

        if info.interaction.is_focused(info.view_id) {
            ctx.draw_rounded_selection_border(
                bounds,
                ctx.style().focus_ring_color,
                2.0,
                ctx.style().button_rounding,
            );
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn interactive(&self) -> bool {
        true
    }
}
impl LabeledSlider {
    pub fn show_value(mut self, show: bool) -> Self {
        self.show_value = show;
        self
    }
    pub fn precision(mut self, p: usize) -> Self {
        self.precision = p;
        self
    }
    pub fn label_width(mut self, w: f32) -> Self {
        self.label_width = w;
        self
    }
    /// Display the value as `value * multiplier` followed by `suffix`
    /// (e.g. multiplier 100 + suffix "%" renders 0.25 as "25%").
    pub fn value_display(mut self, multiplier: f32, suffix: impl Into<String>) -> Self {
        self.value_multiplier = multiplier;
        self.value_suffix = suffix.into();
        self
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::diff::DiffAction;
    use crate::declarative::layout::measure_text_descriptor;
    use crate::declarative::state::StateArena;
    use crate::declarative::widget::{InputResult, Widget};

    use crate::input::UiInputState;

    fn make_slider() -> LabeledSlider {
        LabeledSlider {
            label: "Volume".into(),
            value_id: StateId::test_id(),
            range: 0.0..=1.0,
            label_width: 80.0,
            show_value: true,
            precision: 2,
            value_multiplier: 1.0,
            value_suffix: String::new(),
        }
    }

    #[test]
    fn test_labeled_slider_diff_same_type() {
        let a = make_slider();
        let b = make_slider();
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }

    #[test]
    fn test_labeled_slider_diff_different_type() {
        use crate::declarative::widgets::progress::Progress;
        let slider = make_slider();
        let progress = Progress {
            value: 0.5,
            range: 0.0..=1.0,
            fill_color: None,
            label: None,
        };
        assert_eq!(slider.diff_against(&progress), DiffAction::Replace);
    }

    #[test]
    fn test_labeled_slider_layout_default() {
        let slider = make_slider();
        let style = slider.layout_style(&measure_text_descriptor);
        let default_size: taffy::Size<taffy::Dimension> = taffy::Size {
            width: taffy::Dimension::Length(0.0),
            height: taffy::Dimension::Length(0.0),
        };
        assert!(style.size.width != default_size.width || style.size.height != default_size.height);
    }

    #[test]
    fn test_labeled_slider_focusable() {
        let slider = make_slider();
        assert!(slider.focusable());
    }

    #[test]
    fn test_labeled_slider_handle_input_consumed() {
        let mut state = StateArena::new();
        let value_id = state.get_or_create(ViewId::default(), 0.0f32);
        let slider = LabeledSlider {
            label: "Volume".into(),
            value_id,
            range: 0.0..=1.0,
            label_width: 80.0,
            show_value: true,
            precision: 2,
            value_multiplier: 1.0,
            value_suffix: String::new(),
        };

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(140.0, 10.0);
        input.mouse_down[mouse_button::LEFT] = true;

        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 20.0));

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: input.mouse_pos,
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
            focused_id: None,
        };

        let result = slider.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let new_val: f32 = state.get(slider.value_id).unwrap();
        assert!(
            new_val > 0.0,
            "slider value should have changed after clicking on track"
        );
    }

    #[test]
    fn test_labeled_slider_handle_input_outside_bounds() {
        let slider = make_slider();
        let mut state = StateArena::new();

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(300.0, 10.0);
        input.mouse_pressed[mouse_button::LEFT] = true;

        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 20.0));

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: input.mouse_pos,
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
            focused_id: None,
        };

        let result = slider.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Ignore);
    }
}
