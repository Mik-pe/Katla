use std::any::Any;

use katla_math::{Color, Rect2D, Vec2};
use taffy::{Dimension, Position, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::Callback;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{ChildWidgets, DrawInfo, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;
use crate::input::{KeyCode, mouse_button};
use crate::tokens;

/// Scrim dimming behind an open modal. Strong enough to separate the dialog
/// from the editor, weak enough to keep context visible.
const SCRIM_ALPHA: f32 = 0.6;

pub struct Modal {
    pub title: String,
    pub width: f32,
    pub height: f32,
    pub open_id: StateId,
    pub on_close: Option<Callback>,
    pub child_widget: Option<Box<dyn super::super::widget::Widget>>,
    children: Vec<ViewId>,
}

/// Close-button rect inside the window bounds (shared by input + draw).
fn close_button_bounds(bounds: Rect2D) -> Rect2D {
    Rect2D::from_origin_size(
        Vec2::new(
            bounds.max.x() - tokens::MODAL_CLOSE_SIZE - 6.0,
            bounds.min.y() + (tokens::MODAL_TITLE_HEIGHT - tokens::MODAL_CLOSE_SIZE) * 0.5,
        ),
        Vec2::new(tokens::MODAL_CLOSE_SIZE, tokens::MODAL_CLOSE_SIZE),
    )
}

impl Modal {
    pub fn new(
        width: f32,
        height: f32,
        open_id: StateId,
        on_close: Option<Callback>,
        child_widget: Option<Box<dyn super::super::widget::Widget>>,
    ) -> Self {
        Self {
            title: String::new(),
            width,
            height,
            open_id,
            on_close,
            child_widget,
            children: Vec::new(),
        }
    }

    /// Set the window title shown in the modal title bar.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
}

impl Widget for Modal {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Modal>().is_some() {
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
            // Reserve the title bar strip so content starts below it.
            padding: taffy::Rect {
                top: taffy::LengthPercentage::Length(tokens::MODAL_TITLE_HEIGHT),
                right: taffy::LengthPercentage::Length(0.0),
                bottom: taffy::LengthPercentage::Length(0.0),
                left: taffy::LengthPercentage::Length(0.0),
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
        let is_open: bool = state.get(self.open_id).unwrap_or_default();
        if !is_open {
            return InputResult::Ignore;
        }

        let close = |modal: &Self, state: &mut StateArena, ctx: &mut InputContext| {
            state.set(modal.open_id, false);
            if let Some(ref cb) = modal.on_close {
                ctx.callbacks.invoke(cb, ctx.actions);
            }
        };

        // Escape closes it
        if ctx.input.key_pressed(KeyCode::Escape) {
            close(self, state, ctx);
            return InputResult::Consumed;
        }

        // Click outside the dialog closes it
        if !bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
            close(self, state, ctx);
            return InputResult::Consumed;
        }

        // Close button
        let close_bounds = close_button_bounds(bounds);
        if close_bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
            close(self, state, ctx);
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
        _info: &DrawInfo,
    ) {
        let is_open: bool = state.get(self.open_id).unwrap_or_default();
        if !is_open {
            return;
        }

        // Scrim: dims the editor behind the dialog.
        let screen_bounds = Rect2D::new(Vec2::new(0.0, 0.0), ctx.screen_size());
        ctx.draw_rect(screen_bounds, Color::new(0.0, 0.0, 0.0, SCRIM_ALPHA));

        // Elevation: two soft shadow layers — no hard 0.7-alpha halo.
        let far = bounds.translate(Vec2::new(0.0, 8.0)).inflate(12.0);
        ctx.draw_rounded_rect(
            far,
            Color::new(0.0, 0.0, 0.0, 0.12),
            tokens::RADIUS_WINDOW + 8.0,
        );
        let near = bounds.translate(Vec2::new(0.0, 3.0)).inflate(3.0);
        ctx.draw_rounded_rect(
            near,
            Color::new(0.0, 0.0, 0.0, 0.28),
            tokens::RADIUS_WINDOW + 2.0,
        );

        ctx.draw_rounded_rect(bounds, ctx.style().window_bg, tokens::RADIUS_WINDOW);
        ctx.draw_rounded_selection_border(
            bounds,
            ctx.style().window_border,
            1.0,
            tokens::RADIUS_WINDOW,
        );

        // Title bar.
        let title_bounds = Rect2D::new(
            bounds.min,
            Vec2::new(bounds.max.x(), bounds.min.y() + tokens::MODAL_TITLE_HEIGHT),
        );
        let title_size = crate::style::FontSize::Large.to_pixels();
        let title_pos = Vec2::new(
            bounds.min.x() + tokens::TAB_LABEL_LEADING,
            bounds.min.y() + (tokens::MODAL_TITLE_HEIGHT - title_size) * 0.5,
        );
        ctx.draw_text(
            &self.title,
            title_pos,
            ctx.style().window_title_text,
            title_size,
        );

        // Hairline under the title.
        let divider = Rect2D::from_origin_size(
            Vec2::new(bounds.min.x(), title_bounds.max.y() - 1.0),
            Vec2::new(bounds.width(), 1.0),
        );
        ctx.draw_rect(divider, ctx.style().separator);

        // Close button: a real hit target with hover feedback.
        let close_bounds = close_button_bounds(bounds);
        let close_hovered = close_bounds.contains(ctx.mouse_pos());
        if close_hovered {
            ctx.draw_rounded_rect(
                close_bounds,
                ctx.style().button_hovered,
                tokens::RADIUS_CONTROL,
            );
        }
        let icon_size = tokens::ICON_SIZE_MEDIUM + 2.0;
        let glyph = ctx.measure_icon(crate::ForkAwesome::TIMES, icon_size);
        let glyph_color = if close_hovered {
            ctx.style().text_color
        } else {
            ctx.style().text_hint
        };
        ctx.draw_icon(
            crate::ForkAwesome::TIMES,
            Vec2::new(
                close_bounds.center().x() - glyph.x() * 0.5,
                close_bounds.center().y() - glyph.y() * 0.5,
            ),
            icon_size,
            glyph_color,
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

    fn take_children(&mut self) -> ChildWidgets {
        if let Some(child) = self.child_widget.take() {
            ChildWidgets::Single(child)
        } else {
            ChildWidgets::None
        }
    }

    fn resolve_position_delta(
        &self,
        bounds: Rect2D,
        parent_bounds: Rect2D,
        _zstack_alignment: Option<super::super::descriptor::Alignment>,
        state: &StateArena,
    ) -> Vec2 {
        let is_open: bool = state.get(self.open_id).unwrap_or_default();
        if is_open {
            // Centered, but never clipped above/left of the window: in a
            // window smaller than the dialog the title bar stays reachable.
            let cx = ((parent_bounds.width() - self.width) * 0.5).max(8.0);
            let cy = ((parent_bounds.height() - self.height) * 0.5).max(8.0);
            let centered = parent_bounds.min + Vec2::new(cx, cy);
            centered - bounds.min
        } else {
            Vec2::ZERO
        }
    }

    fn should_draw_children(&self, state: &StateArena) -> bool {
        state.get(self.open_id).unwrap_or_default()
    }

    fn interactive(&self) -> bool {
        true
    }

    /// An open modal receives input globally so Escape closes it and an
    /// outside click dismisses it regardless of what the mouse hit lands on.
    fn wants_global_input(&self, state: &StateArena) -> bool {
        state.get(self.open_id).unwrap_or_default()
    }

    fn is_focus_scope(&self) -> bool {
        true
    }

    fn focus_scope_trap(&self, state: &StateArena) -> bool {
        state.get(self.open_id).unwrap_or_default()
    }
}

impl Modal {
    pub fn on_close(mut self, cb: super::super::descriptor::Callback) -> Self {
        self.on_close = Some(cb);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::build::CallbackTable;

    fn make_modal(open: bool) -> (Modal, StateArena) {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let open_id = arena.get_or_create(view_id, open);
        let modal = Modal::new(400.0, 300.0, open_id, None, None);
        (modal, arena)
    }

    fn make_modal_with_close() -> (Modal, StateArena, CallbackTable) {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let open_id = arena.get_or_create(view_id, true);
        let mut callbacks = CallbackTable::new();
        let cb = callbacks.push(|_actions| {});
        let modal = Modal::new(400.0, 300.0, open_id, Some(cb), None);
        (modal, arena, callbacks)
    }

    #[test]
    fn test_modal_diff_same_type() {
        let (a, _) = make_modal(true);
        let (b, _) = make_modal(true);
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_modal_diff_different_type() {
        let (modal, _) = make_modal(true);
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(modal.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_modal_children() {
        let (mut modal, _) = make_modal(true);
        assert!(modal.children().is_empty());
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        modal.children_mut().push(view_id);
        assert_eq!(modal.children().len(), 1);
    }

    #[test]
    fn test_modal_escape_closes() {
        let (modal, mut arena, mut callbacks) = make_modal_with_close();

        let mut input = crate::input::UiInputState::default();
        input.keys_pressed.push(KeyCode::Escape);

        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(500.0, 500.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
            focused_id: None,
        };

        let bounds = Rect2D::new(Vec2::new(200.0, 150.0), Vec2::new(600.0, 450.0));
        let result = modal.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let is_open: bool = arena.get(modal.open_id).unwrap_or_default();
        assert!(!is_open, "Escape should close modal");
    }

    #[test]
    fn test_modal_click_outside_closes() {
        let (modal, mut arena, mut callbacks) = make_modal_with_close();

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(10.0, 10.0));
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(10.0, 10.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
            focused_id: None,
        };

        let bounds = Rect2D::new(Vec2::new(200.0, 150.0), Vec2::new(600.0, 450.0));
        let result = modal.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let is_open: bool = arena.get(modal.open_id).unwrap_or_default();
        assert!(!is_open, "clicking outside should close modal");
    }

    #[test]
    fn test_modal_closed_ignores_input() {
        let (modal, mut arena) = make_modal(false);

        let mut input = crate::input::UiInputState::default();
        input.keys_pressed.push(KeyCode::Escape);

        let mut callbacks = CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(500.0, 500.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
            focused_id: None,
        };

        let bounds = Rect2D::new(Vec2::new(200.0, 150.0), Vec2::new(600.0, 450.0));
        let result = modal.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Ignore);
    }
}
