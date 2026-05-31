use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, Position, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::{DraggablePanelState, DraggablePanelVisibility};
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;
use crate::input::mouse_button;

pub(crate) struct DraggablePanel {
    pub title: String,
    pub width: f32,
    pub height: f32,
    pub state_id: StateId,
    pub close_on_outside_click: bool,
    children: Vec<ViewId>,
}

impl DraggablePanel {
    pub fn new(
        title: String,
        width: f32,
        height: f32,
        state_id: StateId,
        close_on_outside_click: bool,
    ) -> Self {
        Self {
            title,
            width,
            height,
            state_id,
            close_on_outside_click,
            children: Vec::new(),
        }
    }
}

impl Widget for DraggablePanel {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<DraggablePanel>().is_some() {
            DiffAction::RecurseChildren
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        Style {
            position: Position::Absolute,
            size: taffy::Size {
                width: Dimension::Length(self.width),
                height: Dimension::Length(self.height),
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
        let mut panel_state: DraggablePanelState = state.get(self.state_id).unwrap_or_default();

        if !panel_state.visibility.is_visible() {
            return InputResult::Ignore;
        }

        let title_bar_height = 25.0_f32;
        let close_size = 24.0;
        let close_bounds = Rect2D::from_origin_size(
            Vec2::new(bounds.max.x() - close_size - 6.0, bounds.min.y() + 4.0),
            Vec2::new(close_size, close_size),
        );

        // Close button click
        if close_bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
            panel_state.visibility = DraggablePanelVisibility::Hidden;
            state.set(self.state_id, panel_state);
            return InputResult::Consumed;
        }

        let title_bounds = Rect2D::new(
            bounds.min,
            Vec2::new(bounds.max.x(), bounds.min.y() + title_bar_height),
        );
        let in_title = title_bounds.contains(ctx.mouse_pos);
        let in_close = close_bounds.contains(ctx.mouse_pos);

        // Continue drag
        if panel_state.dragging {
            if ctx.input.mouse_down[mouse_button::LEFT] {
                let new_pos = ctx.mouse_pos - panel_state.drag_offset;
                panel_state.position = Some(new_pos);
                state.set(self.state_id, panel_state);
            } else {
                panel_state.dragging = false;
                state.set(self.state_id, panel_state);
            }
            return InputResult::Consumed;
        }

        // Start drag on title bar
        if in_title && !in_close && ctx.input.mouse_pressed[mouse_button::LEFT] {
            panel_state.dragging = true;
            panel_state.drag_offset = ctx.mouse_pos - bounds.min;
            state.set(self.state_id, panel_state);
            return InputResult::Consumed;
        }

        InputResult::Ignore
    }

    fn draw(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        _animation: &AnimationState,
        _children: &[ViewId],
        _interaction: &DrawInteraction,
        _view_id: ViewId,
        _children_bounds: &[Rect2D],
    ) {
        let panel_state: DraggablePanelState = state.get(self.state_id).unwrap_or_default();

        if !panel_state.visibility.is_visible() {
            return;
        }

        let title_bar_height = 25.0_f32;

        let shadow_offset = Vec2::new(6.0, 6.0);
        let shadow_bounds = Rect2D::new(bounds.min + shadow_offset, bounds.max + shadow_offset);
        ctx.draw_rect(shadow_bounds, ctx.style().popup_shadow);

        ctx.draw_rect(bounds, ctx.style().window_bg);
        ctx.draw_rect_border(
            bounds,
            ctx.style().window_bg,
            ctx.style().window_border,
            1.0,
        );

        let title_bounds = Rect2D::new(
            bounds.min,
            Vec2::new(bounds.max.x(), bounds.min.y() + title_bar_height),
        );

        let can_drag = title_bounds.contains(ctx.mouse_pos());
        let title_color = if can_drag {
            ctx.style().window_title_bg_active
        } else {
            ctx.style().window_title_bg
        };
        ctx.draw_rect(title_bounds, title_color);

        let handle_x = bounds.min.x() + self.width * 0.5 - 20.0;
        let handle_y = bounds.min.y() + 6.0;
        for i in 0..3 {
            let line_y = handle_y + i as f32 * 3.0;
            ctx.draw_line(
                Vec2::new(handle_x, line_y),
                Vec2::new(handle_x + 40.0, line_y),
                ctx.style().text_disabled,
                1.0,
            );
        }

        let font_size = ctx.style().font_size;
        let title_pos = Vec2::new(bounds.min.x() + font_size, bounds.min.y() + font_size);
        ctx.draw_text(&self.title, title_pos, ctx.style().text_color, font_size);

        let close_size = 24.0;
        let close_bounds = Rect2D::from_origin_size(
            Vec2::new(bounds.max.x() - close_size - 6.0, bounds.min.y() + 4.0),
            Vec2::new(close_size, close_size),
        );
        let close_hovered = close_bounds.contains(ctx.mouse_pos());
        let close_bg = if close_hovered {
            ctx.style().button_hovered
        } else {
            title_color
        };
        ctx.draw_rect(close_bounds, close_bg);
        ctx.draw_text(
            "\u{00d7}",
            Vec2::new(close_bounds.min.x() + 6.0, close_bounds.min.y() + 2.0),
            ctx.style().text_color,
            font_size,
        );
    }

    fn focusable(&self) -> bool {
        false
    }

    fn children(&self) -> &[ViewId] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<ViewId> {
        &mut self.children
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_panel() -> DraggablePanel {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let state_id = arena.get_or_create(view_id, DraggablePanelState::default());
        DraggablePanel::new("Panel".into(), 200.0, 300.0, state_id, false)
    }

    fn make_visible_panel() -> (DraggablePanel, StateArena) {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let state_id = arena.get_or_create(view_id, DraggablePanelState::default());
        let mut panel_state: DraggablePanelState = arena.get(state_id).unwrap_or_default();
        panel_state.visibility = DraggablePanelVisibility::Visible;
        panel_state.position = Some(Vec2::new(100.0, 100.0));
        arena.set(state_id, panel_state);
        let panel = DraggablePanel::new("Panel".into(), 200.0, 300.0, state_id, false);
        (panel, arena)
    }

    #[test]
    fn test_draggable_panel_diff_same_type() {
        let a = make_panel();
        let b = make_panel();
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_draggable_panel_diff_different_type() {
        let panel = make_panel();
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(panel.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_draggable_panel_children() {
        let mut panel = make_panel();
        assert!(panel.children().is_empty());
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        panel.children_mut().push(view_id);
        assert_eq!(panel.children().len(), 1);
    }

    #[test]
    fn test_draggable_panel_close_button() {
        let (panel, mut arena) = make_visible_panel();
        let bounds = Rect2D::new(Vec2::new(100.0, 100.0), Vec2::new(300.0, 400.0));

        let close_x = bounds.max.x() - 24.0 - 6.0 + 5.0;
        let close_y = bounds.min.y() + 4.0 + 5.0;

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(close_x, close_y));
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(close_x, close_y),
            callbacks: &mut callbacks,
            actions: &mut actions,
        };

        let result = panel.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let state: DraggablePanelState = arena.get(panel.state_id).unwrap_or_default();
        assert_eq!(state.visibility, DraggablePanelVisibility::Hidden);
    }

    #[test]
    fn test_draggable_panel_drag() {
        let (panel, mut arena) = make_visible_panel();
        let bounds = Rect2D::new(Vec2::new(100.0, 100.0), Vec2::new(300.0, 400.0));

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(150.0, 108.0));
        input.set_mouse_button(mouse_button::LEFT, true);
        input.mouse_down[mouse_button::LEFT] = true;

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(150.0, 108.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
        };

        let result = panel.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let state: DraggablePanelState = arena.get(panel.state_id).unwrap_or_default();
        assert!(state.dragging, "should start dragging on title bar press");
    }
}
