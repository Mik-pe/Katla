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

/// Square icon-only tool button.
pub struct ToolButton {
    pub icon: char,
    pub enabled: bool,
    pub selected: bool,
    pub tooltip: Option<String>,
    pub on_click: Option<Callback>,
}

impl ToolButton {
    fn background(&self, ctx: &UiContext, hovered: bool) -> Color {
        if !self.enabled {
            Color::TRANSPARENT
        } else if self.selected {
            ctx.style().accent
        } else if hovered {
            ctx.style().button_hovered
        } else {
            ctx.style().button_normal
        }
    }

    fn foreground(&self, ctx: &UiContext) -> Color {
        if !self.enabled {
            ctx.style().text_hint
        } else if self.selected {
            // Accent fills need a dark foreground to stay legible across
            // themes whose accent is light (amber, green).
            Color::BLACK
        } else {
            ctx.style().button_text
        }
    }
}

impl Widget for ToolButton {
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
                width: Dimension::Length(crate::tokens::CONTROL_HEIGHT),
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
        info: &DrawInfo,
    ) {
        let hovered = self.enabled && bounds.contains(ctx.mouse_pos());
        if hovered && let Some(ref tooltip) = self.tooltip {
            ctx.defer_tooltip(tooltip);
        }
        let bg = animation.apply_to_color(self.background(ctx, hovered));
        let radius = animation.apply_to_corner_radius(ctx.style().input_rounding);
        ctx.draw_rounded_rect(bounds, bg, radius);

        if info.interaction.is_focused(info.view_id) {
            ctx.draw_rounded_selection_border(bounds, ctx.style().focus_ring_color, 2.0, radius);
        }

        let font_size = crate::tokens::ICON_SIZE;
        let text_size = ctx.measure_icon(self.icon, font_size);
        let text_pos = Vec2::new(
            bounds.center().x() - text_size.x() * 0.5,
            bounds.center().y() - text_size.y() * 0.5,
        );
        ctx.draw_icon(
            self.icon,
            text_pos,
            font_size,
            animation.apply_to_color(self.foreground(ctx)),
        );
    }

    fn focusable(&self) -> bool {
        self.on_click.is_some() && self.enabled
    }

    fn press_action(&self) -> Option<Callback> {
        if self.enabled { self.on_click } else { None }
    }

    fn interactive(&self) -> bool {
        true
    }
}

impl ToolButton {
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn on_click(mut self, cb: Callback) -> Self {
        self.on_click = Some(cb);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::diff::DiffAction;

    #[test]
    fn test_tool_button_layout_is_control_height_square() {
        let button = ToolButton {
            icon: 'X',
            enabled: true,
            selected: false,
            tooltip: None,
            on_click: None,
        };
        let style = button.layout_style(&crate::declarative::layout::measure_text_descriptor);
        assert_eq!(
            style.size.width,
            Dimension::Length(crate::tokens::CONTROL_HEIGHT)
        );
        assert_eq!(
            style.size.height,
            Dimension::Length(crate::tokens::CONTROL_HEIGHT)
        );
    }

    #[test]
    fn test_tool_button_diff_same_type_updates() {
        let a = ToolButton {
            icon: 'X',
            enabled: true,
            selected: false,
            tooltip: None,
            on_click: None,
        };
        let b = ToolButton {
            icon: 'X',
            enabled: true,
            selected: true,
            tooltip: None,
            on_click: None,
        };
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }
}
