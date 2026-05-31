use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::MenuGroup;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;
use crate::input::mouse_button;

pub(crate) struct MenuBar {
    pub groups: Vec<MenuGroup>,
    pub right_content: Option<Box<dyn super::super::widget::Widget>>,
    pub height: f32,
    children: Vec<ViewId>,
}

impl MenuBar {
    pub fn new(
        groups: Vec<MenuGroup>,
        right_content: Option<Box<dyn super::super::widget::Widget>>,
        height: f32,
    ) -> Self {
        Self {
            groups,
            right_content,
            height,
            children: Vec::new(),
        }
    }
}

impl Widget for MenuBar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<MenuBar>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        Style {
            size: Size {
                width: Dimension::Percent(1.0),
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
        let item_spacing = 8.0_f32;
        let mut x = bounds.min.x() + item_spacing;
        let bar_height = self.height;

        for group in &self.groups {
            let label_size = measure_menu_label(&group.label, 14.0);
            let group_bounds = Rect2D::from_origin_size(
                Vec2::new(x, bounds.min.y()),
                Vec2::new(label_size + item_spacing * 2.0, bar_height),
            );

            if group_bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
                let is_open: bool = state.get(group.open_id).unwrap_or_default();
                state.set(group.open_id, !is_open);
                return InputResult::Consumed;
            }

            let is_open: bool = state.get(group.open_id).unwrap_or_default();
            if is_open {
                let dropdown_y = group_bounds.max.y();
                let dropdown_width = 180.0_f32;
                let entry_height = 28.0_f32;
                let dropdown_bounds = Rect2D::from_origin_size(
                    Vec2::new(group_bounds.min.x(), dropdown_y),
                    Vec2::new(dropdown_width, group.items.len() as f32 * entry_height),
                );

                if dropdown_bounds.contains(ctx.mouse_pos)
                    && ctx.input.mouse_clicked(mouse_button::LEFT)
                {
                    for (i, entry) in group.items.iter().enumerate() {
                        let entry_bounds = Rect2D::from_origin_size(
                            Vec2::new(
                                dropdown_bounds.min.x(),
                                dropdown_y + i as f32 * entry_height,
                            ),
                            Vec2::new(dropdown_width, entry_height),
                        );
                        if entry_bounds.contains(ctx.mouse_pos) && !entry.disabled {
                            if let Some(ref callback) = entry.on_click {
                                ctx.callbacks.invoke(callback, ctx.actions);
                            }
                            state.set(group.open_id, false);
                            return InputResult::Consumed;
                        }
                    }
                }

                if !group_bounds.contains(ctx.mouse_pos)
                    && !dropdown_bounds.contains(ctx.mouse_pos)
                    && ctx.input.mouse_clicked(mouse_button::LEFT)
                {
                    state.set(group.open_id, false);
                    return InputResult::Consumed;
                }
            }

            x += label_size + item_spacing * 2.0;
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
        ctx.draw_rect(bounds, ctx.style().menu_bg);
        ctx.draw_line(
            Vec2::new(bounds.min.x(), bounds.max.y()),
            Vec2::new(bounds.max.x(), bounds.max.y()),
            ctx.style().separator,
            1.0,
        );

        let font_size = ctx.style().font_size;
        let item_spacing = ctx.style().window_padding;
        let mut x = bounds.min.x() + item_spacing;
        let y_center = bounds.min.y() + (self.height - font_size) * 0.5;

        for group in &self.groups {
            let label_size = ctx.measure_text(&group.label, font_size).x();
            let group_bounds = Rect2D::from_origin_size(
                Vec2::new(x, bounds.min.y()),
                Vec2::new(label_size + item_spacing * 2.0, self.height),
            );
            let group_hovered = group_bounds.contains(ctx.mouse_pos());
            if group_hovered {
                ctx.draw_rect(group_bounds, ctx.style().button_hovered);
            }
            ctx.draw_text(
                &group.label,
                Vec2::new(x + item_spacing, y_center),
                ctx.style().text_color,
                font_size,
            );

            let is_open: bool = state.get(group.open_id).unwrap_or_default();
            if is_open {
                let dropdown_y = group_bounds.max.y();
                let dropdown_width = 180.0_f32;
                let entry_height = 28.0_f32;
                let dropdown_bounds = Rect2D::from_origin_size(
                    Vec2::new(group_bounds.min.x(), dropdown_y),
                    Vec2::new(dropdown_width, group.items.len() as f32 * entry_height),
                );

                ctx.draw_rect(dropdown_bounds, ctx.style().window_bg);
                ctx.draw_rect_border(
                    dropdown_bounds,
                    ctx.style().window_bg,
                    ctx.style().window_border,
                    1.0,
                );

                for (i, entry) in group.items.iter().enumerate() {
                    let entry_bounds = Rect2D::from_origin_size(
                        Vec2::new(
                            dropdown_bounds.min.x(),
                            dropdown_y + i as f32 * entry_height,
                        ),
                        Vec2::new(dropdown_width, entry_height),
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
                    let entry_y = entry_bounds.center().y() - font_size * 0.5;
                    ctx.draw_text(
                        &entry.label,
                        Vec2::new(entry_bounds.min.x() + item_spacing, entry_y),
                        text_color,
                        font_size,
                    );
                }
            }

            x += label_size + item_spacing * 2.0;
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

fn measure_menu_label(label: &str, font_size: f32) -> f32 {
    let char_width = font_size * 0.6;
    label.chars().count() as f32 * char_width
}

use taffy::Size;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::build::CallbackTable;
    use crate::declarative::descriptor::MenuEntry;
    use crate::declarative::state::StateId;

    fn make_menubar() -> MenuBar {
        MenuBar::new(vec![], None, 28.0)
    }

    fn make_menubar_with_groups() -> (MenuBar, StateId, CallbackTable) {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let open_id = arena.get_or_create(view_id, false);
        let mut callbacks = CallbackTable::new();
        let cb = callbacks.push(|_actions| {});
        let menubar = MenuBar::new(
            vec![MenuGroup {
                label: "File".into(),
                open_id,
                items: vec![MenuEntry {
                    label: "Open".into(),
                    on_click: Some(cb),
                    disabled: false,
                }],
            }],
            None,
            28.0,
        );
        (menubar, open_id, callbacks)
    }

    #[test]
    fn test_menubar_diff_same_type() {
        let a = make_menubar();
        let b = make_menubar();
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }

    #[test]
    fn test_menubar_diff_different_type() {
        let mb = make_menubar();
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(mb.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_menubar_children() {
        let mut mb = make_menubar();
        assert!(mb.children().is_empty());
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        mb.children_mut().push(view_id);
        assert_eq!(mb.children().len(), 1);
    }

    #[test]
    fn test_menubar_toggle_dropdown() {
        let (menubar, open_id, mut callbacks) = make_menubar_with_groups();
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let open_id = arena.get_or_create(view_id, false);

        let menubar = MenuBar::new(
            vec![MenuGroup {
                label: "File".into(),
                open_id,
                items: vec![],
            }],
            None,
            28.0,
        );

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(katla_math::Vec2::new(30.0, 10.0));
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: katla_math::Vec2::new(30.0, 10.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
        };

        let bounds = Rect2D::new(
            katla_math::Vec2::new(0.0, 0.0),
            katla_math::Vec2::new(800.0, 28.0),
        );
        let result = menubar.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let is_open: bool = arena.get(open_id).unwrap_or_default();
        assert!(is_open, "clicking group should toggle dropdown open");
    }
}
