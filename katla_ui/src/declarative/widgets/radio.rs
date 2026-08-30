use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{DrawInfo, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;
use crate::input::mouse_button;

pub struct RadioButton {
    pub value_id: StateId,
    pub index: usize,
    pub label: String,
}

impl Widget for RadioButton {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<RadioButton>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, measure: MeasureFn<'_>) -> Style {
        let text_size = measure(&self.label, None);
        Style {
            size: Size {
                width: Dimension::Length(text_size.x() + 28.0),
                height: Dimension::Length(text_size.y() + 16.0),
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
        if bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
            state.set(self.value_id, self.index);
            return InputResult::Consumed;
        }
        InputResult::Ignore
    }

    fn draw(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        animation: &AnimationState,
        _children: &[ViewId],
        _info: &DrawInfo,
    ) {
        let selected: usize = state.get(self.value_id).unwrap_or_default();
        let is_selected = selected == self.index;
        let is_hovered = bounds.contains(ctx.mouse_pos());

        let bg = if is_selected {
            ctx.style().check_mark_color
        } else if is_hovered {
            ctx.style().button_hovered
        } else {
            ctx.style().window_title_bg
        };
        let bg = animation.apply_to_color(bg);
        let radius = animation.apply_to_corner_radius(ctx.style().button_rounding);
        ctx.draw_rounded_rect(bounds, bg, radius);

        let indicator_radius = bounds.height() * 0.15;
        let indicator_center =
            Vec2::new(bounds.min.x() + indicator_radius * 2.0, bounds.center().y());
        ctx.draw_circle(
            indicator_center,
            indicator_radius,
            animation.apply_to_color(ctx.style().window_border),
        );
        if is_selected {
            ctx.draw_circle(
                indicator_center,
                indicator_radius * 0.6,
                animation.apply_to_color(ctx.style().text_color),
            );
        }

        let font_size = ctx.style().font_size;
        let text_size = ctx.measure_text(&self.label, font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + indicator_radius * 4.0,
            bounds.center().y() - text_size.y() * 0.5,
        );
        // Inactive radio labels are secondary, NOT disabled: inactive tools
        // must stay distinguishable from unavailable ones.
        let text_color = if is_selected {
            ctx.style().text_color
        } else {
            ctx.style().text_secondary
        };
        ctx.draw_text(
            &self.label,
            text_pos,
            animation.apply_to_color(text_color),
            font_size,
        );
    }

    fn focusable(&self) -> bool {
        true
    }

    fn interactive(&self) -> bool {
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn make_radio(index: usize, label: &str) -> RadioButton {
        RadioButton {
            value_id: StateId::test_id(),
            index,
            label: label.to_string(),
        }
    }

    #[test]
    fn test_radio_click_sets_group_index() {
        let mut state = StateArena::new();
        let dummy_view = ViewId::from(slotmap::KeyData::from_ffi(0));
        let value_id = state.get_or_create(dummy_view, 0_usize);
        let radio = RadioButton {
            value_id,
            index: 2,
            label: "Option C".to_string(),
        };

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(50.0, 10.0));
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(50.0, 10.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
            focused_id: None,
        };

        let bounds = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(200.0, 20.0));
        let result = radio.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let selected: usize = state.get(value_id).unwrap_or_default();
        assert_eq!(selected, 2);
    }

    #[test]
    fn test_radio_diff() {
        let a = make_radio(0, "A");
        let b = make_radio(1, "B");
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }

    #[test]
    fn test_radio_diff_different_type() {
        use super::super::image::Image;
        use crate::types::TextureId;
        use katla_math::Color;

        let radio = make_radio(0, "A");
        let image = Image {
            texture: TextureId(0),
            uv: None,
            tint: Color::WHITE,
            width: None,
            height: None,
        };
        assert_eq!(radio.diff_against(&image), DiffAction::Replace);
    }

    #[test]
    fn test_radio_focusable() {
        let radio = make_radio(0, "A");
        assert!(radio.focusable());
    }
}
