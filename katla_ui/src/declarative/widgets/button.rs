use std::any::Any;

use katla_math::{Color, Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;
use crate::input::mouse_button;

use super::super::animation::AnimationState;
use super::super::descriptor::Callback;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInfo, InputContext, InputResult, MeasureFn, Widget};

pub struct Button {
    pub label: String,
    pub fill_color: Option<Color>,
    pub border_color: Option<Color>,
    pub tooltip: Option<String>,
    pub on_click: Option<Callback>,
}

impl Widget for Button {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Button>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, measure: MeasureFn<'_>) -> Style {
        let text_size = measure(&self.label, None);
        let h_padding = 16.0;
        Style {
            size: Size {
                width: Dimension::Length(text_size.x() + h_padding),
                height: Dimension::Length(crate::tokens::CONTROL_HEIGHT),
            },
            ..Style::default()
        }
    }

    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        _state: &mut StateArena,
        bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        if bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
            if let Some(ref cb) = self.on_click {
                ctx.callbacks.invoke(cb, ctx.actions);
            }
            return InputResult::Consumed;
        }
        InputResult::Ignore
    }

    fn draw(
        &self,
        ctx: &mut UiContext,
        _state: &StateArena,
        bounds: Rect2D,
        animation: &AnimationState,
        _children: &[ViewId],
        info: &DrawInfo,
    ) {
        let hovered = bounds.contains(ctx.mouse_pos());
        if hovered && let Some(ref tooltip) = self.tooltip {
            ctx.defer_tooltip(tooltip);
        }
        let bg = if hovered {
            ctx.style().button_hovered
        } else {
            ctx.style().button_normal
        };
        let bg = self.fill_color.unwrap_or(bg);
        let bg = animation.apply_to_color(bg);
        let radius = animation.apply_to_corner_radius(ctx.style().button_rounding);
        ctx.draw_rounded_rect(bounds, bg, radius);

        if let Some(border) = self.border_color {
            ctx.draw_rounded_selection_border(bounds, border, 1.0, radius);
        }

        if info.interaction.is_focused(info.view_id) {
            ctx.draw_rounded_selection_border(bounds, ctx.style().focus_ring_color, 2.0, radius);
        }

        let font_size = ctx.style().font_size;
        let text_size = ctx.measure_text(&self.label, font_size);
        let text_pos = Vec2::new(
            bounds.center().x() - text_size.x() * 0.5,
            bounds.center().y() - text_size.y() * 0.5,
        );
        ctx.draw_text(
            &self.label,
            text_pos,
            animation.apply_to_color(ctx.style().button_text),
            font_size,
        );
    }

    fn focusable(&self) -> bool {
        self.on_click.is_some()
    }

    fn press_action(&self) -> Option<Callback> {
        self.on_click
    }

    fn interactive(&self) -> bool {
        true
    }
}

impl Button {
    pub fn fill(mut self, color: impl Into<katla_math::Color>) -> Self {
        self.fill_color = Some(color.into());
        self
    }
    pub fn border(mut self, color: impl Into<katla_math::Color>) -> Self {
        self.border_color = Some(color.into());
        self
    }
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
    pub fn on_click(mut self, cb: super::super::descriptor::Callback) -> Self {
        self.on_click = Some(cb);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::actions::ActionStream;
    use crate::declarative::build::CallbackTable;
    use crate::declarative::constructors::text;
    use crate::input::UiInputState;

    #[test]
    fn test_button_click_callback() {
        let mut callbacks = CallbackTable::new();
        let cb = callbacks.push(|_actions| {});
        let button = Button {
            label: "click".into(),
            fill_color: None,
            border_color: None,
            tooltip: None,
            on_click: Some(cb),
        };

        let mut input = UiInputState::new();
        input.set_mouse_pos(Vec2::new(50.0, 15.0));
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut state = StateArena::new();
        let mut actions = ActionStream::new();
        let bounds = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 30.0));

        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(50.0, 15.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
            focused_id: None,
        };

        let result = button.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);
    }

    #[test]
    fn test_button_diff() {
        let a = Button {
            label: "ok".into(),
            fill_color: None,
            border_color: None,
            tooltip: None,
            on_click: None,
        };
        let b = Button {
            label: "cancel".into(),
            fill_color: None,
            border_color: None,
            tooltip: None,
            on_click: None,
        };
        assert_eq!(b.diff_against(&a), DiffAction::Update);

        let other = text("hello");
        assert_eq!(a.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_button_focusable() {
        let mut callbacks = CallbackTable::new();
        let cb = callbacks.push(|_actions| {});
        let with_cb = Button {
            label: "click".into(),
            fill_color: None,
            border_color: None,
            tooltip: None,
            on_click: Some(cb),
        };
        assert!(with_cb.focusable());

        let without_cb = Button {
            label: "label".into(),
            fill_color: None,
            border_color: None,
            tooltip: None,
            on_click: None,
        };
        assert!(!without_cb.focusable());
    }
}
