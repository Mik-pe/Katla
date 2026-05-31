use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Position, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::ContextMenuEntry;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;
use crate::input::mouse_button;

pub(crate) struct ContextMenu {
    pub items: Vec<ContextMenuEntry>,
    pub open_id: StateId,
    children: Vec<ViewId>,
}

impl ContextMenu {
    pub fn new(items: Vec<ContextMenuEntry>, open_id: StateId) -> Self {
        Self {
            items,
            open_id,
            children: Vec::new(),
        }
    }
}

impl Widget for ContextMenu {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<ContextMenu>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        Style {
            position: Position::Absolute,
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

        if bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
            let item_height = 28.0_f32;
            for (i, entry) in self.items.iter().enumerate() {
                let entry_bounds = Rect2D::from_origin_size(
                    Vec2::new(bounds.min.x(), bounds.min.y() + i as f32 * item_height),
                    Vec2::new(bounds.width(), item_height),
                );
                if entry_bounds.contains(ctx.mouse_pos) && !entry.disabled {
                    if let Some(ref callback) = entry.on_click {
                        ctx.callbacks.invoke(callback, ctx.actions);
                    }
                    state.set(self.open_id, false);
                    return InputResult::Consumed;
                }
            }
        }

        if !bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
            state.set(self.open_id, false);
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
        let is_open: bool = state.get(self.open_id).unwrap_or_default();
        if !is_open {
            return;
        }

        let font_size = ctx.style().font_size;
        let item_height = 28.0_f32;
        let item_spacing = ctx.style().item_inner_spacing;
        let max_label_width: f32 = self
            .items
            .iter()
            .map(|item| ctx.measure_text(&item.label, font_size).x())
            .fold(0.0_f32, f32::max);
        let menu_width = max_label_width + item_spacing * 4.0;
        let menu_height = self.items.len() as f32 * item_height;

        let menu_bounds = Rect2D::from_origin_size(bounds.min, Vec2::new(menu_width, menu_height));

        ctx.draw_rect(menu_bounds, ctx.style().window_bg);
        ctx.draw_rect_border(
            menu_bounds,
            ctx.style().window_bg,
            ctx.style().window_border,
            1.0,
        );

        for (i, entry) in self.items.iter().enumerate() {
            let entry_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), bounds.min.y() + i as f32 * item_height),
                Vec2::new(menu_width, item_height),
            );

            let entry_hovered = entry_bounds.contains(ctx.mouse_pos());
            if entry_hovered && !entry.disabled {
                ctx.draw_rect(entry_bounds, ctx.style().selectable_hovered);
            }

            let text_color = if entry.disabled {
                ctx.style().text_disabled
            } else {
                ctx.style().text_color
            };
            let label_y = entry_bounds.center().y() - font_size * 0.5;
            ctx.draw_text(
                &entry.label,
                Vec2::new(entry_bounds.min.x() + item_spacing * 2.0, label_y),
                text_color,
                font_size,
            );
        }
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

    fn interactive(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::build::CallbackTable;

    fn make_context_menu(open: bool) -> (ContextMenu, StateArena) {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let open_id = arena.get_or_create(view_id, open);
        let menu = ContextMenu::new(
            vec![
                ContextMenuEntry {
                    label: "Copy".into(),
                    on_click: None,
                    disabled: false,
                },
                ContextMenuEntry {
                    label: "Paste".into(),
                    on_click: None,
                    disabled: true,
                },
            ],
            open_id,
        );
        (menu, arena)
    }

    #[test]
    fn test_context_menu_diff_same_type() {
        let (a, _) = make_context_menu(true);
        let (b, _) = make_context_menu(true);
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }

    #[test]
    fn test_context_menu_diff_different_type() {
        let (menu, _) = make_context_menu(true);
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(menu.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_context_menu_children() {
        let (mut menu, _) = make_context_menu(true);
        assert!(menu.children().is_empty());
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        menu.children_mut().push(view_id);
        assert_eq!(menu.children().len(), 1);
    }

    #[test]
    fn test_context_menu_item_selection() {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let open_id = arena.get_or_create(view_id, true);
        let mut callbacks = CallbackTable::new();
        let cb = callbacks.push(|_actions| {});

        let menu = ContextMenu::new(
            vec![ContextMenuEntry {
                label: "Copy".into(),
                on_click: Some(cb),
                disabled: false,
            }],
            open_id,
        );

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(50.0, 10.0));
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(50.0, 10.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
        };

        let bounds = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(200.0, 28.0));
        let result = menu.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let is_open: bool = arena.get(open_id).unwrap_or_default();
        assert!(!is_open, "selecting item should close context menu");
    }

    #[test]
    fn test_context_menu_click_outside_closes() {
        let (menu, mut arena) = make_context_menu(true);

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(500.0, 500.0));
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut callbacks = CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(500.0, 500.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
        };

        let bounds = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(200.0, 100.0));
        let result = menu.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let is_open: bool = arena.get(menu.open_id).unwrap_or_default();
        assert!(!is_open, "clicking outside should close context menu");
    }
}
