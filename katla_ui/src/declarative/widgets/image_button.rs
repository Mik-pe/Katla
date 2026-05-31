use std::any::Any;

use katla_math::{Color, Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;
use crate::input::mouse_button;

use super::super::animation::AnimationState;
use super::super::descriptor::Callback;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};

pub struct ImageButton {
    pub icon: char,
    pub enabled: bool,
    pub fill_color: Option<Color>,
    pub on_click: Option<Callback>,
}

impl Widget for ImageButton {
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
                width: Dimension::Length(28.0),
                height: Dimension::Length(28.0),
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
        if !self.enabled {
            return InputResult::Ignore;
        }

        if bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
            if let Some(ref callback) = self.on_click {
                ctx.callbacks.invoke(callback, ctx.actions);
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
        _interaction: &DrawInteraction,
        _view_id: ViewId,
        _children_bounds: &[Rect2D],
    ) {
        let bg = self.fill_color.unwrap_or(ctx.style().button_normal);
        let bg = animation.apply_to_color(bg);
        let radius = animation.apply_to_corner_radius(ctx.style().button_rounding);
        ctx.draw_rounded_rect(bounds, bg, radius);

        let font_size = ctx.style().icon_button_size * 0.6;
        let text_size = ctx.measure_icon(self.icon, font_size);
        let text_pos = Vec2::new(
            bounds.center().x() - text_size.x() * 0.5,
            bounds.center().y() - text_size.y() * 0.5,
        );
        let icon_color = if self.enabled {
            ctx.style().button_text
        } else {
            ctx.style().text_hint
        };
        ctx.draw_icon(
            self.icon,
            text_pos,
            font_size,
            animation.apply_to_color(icon_color),
        );
    }

    fn focusable(&self) -> bool {
        self.on_click.is_some() && self.enabled
    }

    fn interactive(&self) -> bool {
        true
    }
}
impl ImageButton {
    pub fn fill(mut self, color: impl Into<katla_math::Color>) -> Self {
        self.fill_color = Some(color.into());
        self
    }
    pub fn on_click(mut self, cb: super::super::descriptor::Callback) -> Self {
        self.on_click = Some(cb);
        self
    }
    pub fn enabled(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::diff::DiffAction;
    use crate::declarative::widget::{InputResult, Widget};
    use crate::declarative::widgets::progress::Progress;
    use crate::input::UiInputState;

    fn make_image_button(on_click: Option<Callback>) -> ImageButton {
        ImageButton {
            icon: '+',
            enabled: true,
            fill_color: None,
            on_click,
        }
    }

    static mut CALLBACK_INVOKED: bool = false;

    fn make_callback() -> Callback {
        let mut table = crate::declarative::build::CallbackTable::new();
        table.push(|_actions| unsafe { CALLBACK_INVOKED = true })
    }

    #[test]
    fn test_image_button_click_fires_callback() {
        let callback = make_callback();
        let _btn = make_image_button(Some(callback));
        let mut state = StateArena::new();

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(15.0, 15.0);
        input.set_mouse_button_with_time(mouse_button::LEFT, true, 1.0);

        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(30.0, 30.0));

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let callback = callbacks.push(|_actions| {});
        let btn = ImageButton {
            icon: '+',
            enabled: true,
            fill_color: None,
            on_click: Some(callback),
        };

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

        let result = btn.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);
    }

    #[test]
    fn test_image_button_disabled_no_callback() {
        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let callback = callbacks.push(|_actions| {});
        let btn = ImageButton {
            icon: '+',
            enabled: false,
            fill_color: None,
            on_click: Some(callback),
        };
        let mut state = StateArena::new();

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(15.0, 15.0);
        input.set_mouse_button_with_time(mouse_button::LEFT, true, 1.0);

        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(30.0, 30.0));

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

        let result = btn.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Ignore);
    }

    #[test]
    fn test_image_button_diff() {
        let btn_a = make_image_button(None);
        let btn_b = make_image_button(None);
        assert_eq!(btn_b.diff_against(&btn_a), DiffAction::Update);

        let progress = Progress {
            value: 0.5,
            range: 0.0..=1.0,
            fill_color: None,
            label: None,
        };
        assert_eq!(btn_a.diff_against(&progress), DiffAction::Replace);
    }

    #[test]
    fn test_image_button_focusable_with_callback() {
        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let callback = callbacks.push(|_actions| {});
        let btn = ImageButton {
            icon: '+',
            enabled: true,
            fill_color: None,
            on_click: Some(callback),
        };
        assert!(btn.focusable());
    }

    #[test]
    fn test_image_button_not_focusable_without_callback() {
        let btn = make_image_button(None);
        assert!(!btn.focusable());
    }

    #[test]
    fn test_image_button_not_focusable_disabled() {
        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let callback = callbacks.push(|_actions| {});
        let btn = ImageButton {
            icon: '+',
            enabled: false,
            fill_color: None,
            on_click: Some(callback),
        };
        assert!(!btn.focusable());
    }

    #[test]
    fn test_image_button_click_outside_ignored() {
        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let callback = callbacks.push(|_actions| {});
        let btn = ImageButton {
            icon: '+',
            enabled: true,
            fill_color: None,
            on_click: Some(callback),
        };
        let mut state = StateArena::new();

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(50.0, 50.0);
        input.set_mouse_button_with_time(mouse_button::LEFT, true, 1.0);

        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(30.0, 30.0));

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

        let result = btn.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Ignore);
    }
}
