use std::{any::Any, cell::RefCell};

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::MenuGroup;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{ChildWidgets, DrawInfo, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;
use crate::input::mouse_button;

pub struct MenuBar {
    pub groups: Vec<MenuGroup>,
    pub right_content: Option<Box<dyn super::super::widget::Widget>>,
    pub height: f32,
    children: Vec<ViewId>,
    label_widths: RefCell<Vec<f32>>,
    compact: bool,
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
            label_widths: RefCell::new(Vec::new()),
            compact: false,
        }
    }

    /// Resolve the same measured label regions used for painting and input.
    pub fn group_bounds(&self, bounds: Rect2D) -> Vec<Rect2D> {
        let mut x = bounds.min.x() + MENU_PADDING;
        self.label_widths
            .borrow()
            .iter()
            .map(|label_width| {
                let width = label_width + MENU_PADDING * 2.0;
                let group = Rect2D::from_origin_size(
                    Vec2::new(x, bounds.min.y()),
                    Vec2::new(width, self.height),
                );
                x += width;
                group
            })
            .collect()
    }

    /// Resolve dropdown row regions, including compact separators.
    pub fn entry_bounds(&self, group: usize, bounds: Rect2D) -> Vec<Rect2D> {
        let mut y = bounds.max.y();
        self.groups[group]
            .items
            .iter()
            .map(|entry| {
                let height = if entry.label.is_empty() { 8.0 } else { 28.0 };
                let row = Rect2D::from_origin_size(
                    Vec2::new(bounds.min.x(), y),
                    Vec2::new(180.0, height),
                );
                y += height;
                row
            })
            .collect()
    }

    /// Size the bar to its measured menu labels for use beside app branding.
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
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

    fn layout_style(&self, measure: MeasureFn<'_>) -> Style {
        let widths: Vec<f32> = self
            .groups
            .iter()
            .map(|g| measure(&g.label, None, None).x())
            .collect();
        let width = widths.iter().sum::<f32>() + MENU_PADDING * (2.0 * widths.len() as f32 + 2.0);
        *self.label_widths.borrow_mut() = widths;
        Style {
            size: Size {
                width: if self.compact {
                    Dimension::Length(width)
                } else {
                    Dimension::Percent(1.0)
                },
                height: Dimension::Length(self.height),
            },
            ..Style::default()
        }
    }

    fn take_children(&mut self) -> ChildWidgets {
        self.right_content
            .take()
            .map(ChildWidgets::Single)
            .unwrap_or(ChildWidgets::None)
    }

    fn wants_global_input(&self, state: &StateArena) -> bool {
        self.groups
            .iter()
            .any(|g| state.get::<bool>(g.open_id).unwrap_or_default())
    }

    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        state: &mut StateArena,
        bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        let any_open = self
            .groups
            .iter()
            .any(|g| state.get::<bool>(g.open_id).unwrap_or_default());
        let group_bounds_list = self.group_bounds(bounds);

        // Hover-switch: if any menu is open and mouse is over another group,
        // switch to that group
        if any_open && bounds.contains(ctx.mouse_pos) {
            for (i, &gb) in group_bounds_list.iter().enumerate() {
                if gb.contains(ctx.mouse_pos) {
                    let currently_open: bool =
                        state.get(self.groups[i].open_id).unwrap_or_default();
                    if !currently_open {
                        for g in &self.groups {
                            state.set(g.open_id, false);
                        }
                        state.set(self.groups[i].open_id, true);
                    }
                    return InputResult::Consumed;
                }
            }
        }

        // Handle clicks on group labels (open/close toggle)
        if bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
            for (i, &gb) in group_bounds_list.iter().enumerate() {
                if gb.contains(ctx.mouse_pos) {
                    let is_open: bool = state.get(self.groups[i].open_id).unwrap_or_default();
                    for g in &self.groups {
                        state.set(g.open_id, false);
                    }
                    state.set(self.groups[i].open_id, !is_open);
                    return InputResult::Consumed;
                }
            }
        }

        // Handle dropdown item clicks (may be outside menu bar bounds)
        if any_open && ctx.input.mouse_clicked(mouse_button::LEFT) {
            for (i, &gb) in group_bounds_list.iter().enumerate() {
                let is_open: bool = state.get(self.groups[i].open_id).unwrap_or_default();
                if !is_open {
                    continue;
                }

                let rows = self.entry_bounds(i, gb);
                let dropdown_bounds = Rect2D::new(
                    Vec2::new(gb.min.x(), gb.max.y()),
                    rows.last()
                        .map_or(Vec2::new(gb.min.x() + 180.0, gb.max.y()), |row| row.max),
                );
                if dropdown_bounds.contains(ctx.mouse_pos) {
                    for (entry, entry_bounds) in self.groups[i].items.iter().zip(&rows) {
                        if entry_bounds.contains(ctx.mouse_pos)
                            && !entry.disabled
                            && !entry.label.is_empty()
                        {
                            if let Some(ref callback) = entry.on_click {
                                ctx.callbacks.invoke(callback, ctx.actions);
                            }
                            state.set(self.groups[i].open_id, false);
                            return InputResult::Consumed;
                        }
                    }
                    return InputResult::Consumed;
                }

                // Click outside both group label and dropdown closes the menu
                if !gb.contains(ctx.mouse_pos) && !dropdown_bounds.contains(ctx.mouse_pos) {
                    state.set(self.groups[i].open_id, false);
                    return InputResult::Consumed;
                }
            }
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
        ctx.draw_rect(bounds, ctx.style().menu_bg);

        let font_size = ctx.style().font_size;
        let item_spacing = MENU_PADDING;
        let y_center = bounds.min.y() + (self.height - font_size) * 0.5;

        for (group_index, (group, group_bounds)) in self
            .groups
            .iter()
            .zip(self.group_bounds(bounds))
            .enumerate()
        {
            let group_hovered = group_bounds.contains(ctx.mouse_pos());
            if group_hovered {
                ctx.draw_rect(group_bounds, ctx.style().button_hovered);
            }
            ctx.draw_text(
                &group.label,
                Vec2::new(group_bounds.min.x() + item_spacing, y_center),
                ctx.style().text_color,
                font_size,
            );

            let is_open: bool = state.get(group.open_id).unwrap_or_default();
            if is_open {
                let rows = self.entry_bounds(group_index, group_bounds);
                let dropdown_bounds = Rect2D::new(
                    Vec2::new(group_bounds.min.x(), group_bounds.max.y()),
                    rows.last().map_or(
                        Vec2::new(group_bounds.min.x() + 180.0, group_bounds.max.y()),
                        |row| row.max,
                    ),
                );

                let previous_z = ctx.draw_list.z_index();
                let popup_z = previous_z.max(crate::context::z_index::POPUP);
                ctx.draw_list.set_z_index(popup_z);
                ctx.register_hover_layer(popup_z, dropdown_bounds);
                ctx.draw_rect(dropdown_bounds, ctx.style().window_bg);
                ctx.draw_rect_border(
                    dropdown_bounds,
                    ctx.style().window_bg,
                    ctx.style().window_border,
                    1.0,
                );

                for (entry, &entry_bounds) in group.items.iter().zip(&rows) {
                    if entry.label.is_empty() {
                        let center = entry_bounds.center().y();
                        ctx.draw_rect(
                            Rect2D::from_origin_size(
                                Vec2::new(entry_bounds.min.x() + item_spacing, center),
                                Vec2::new(entry_bounds.width() - item_spacing * 2.0, 1.0),
                            ),
                            ctx.style().separator,
                        );
                        continue;
                    }
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
                ctx.draw_list.set_z_index(previous_z);
            }
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

    fn interactive(&self, _state: &StateArena) -> bool {
        true
    }
}

const MENU_PADDING: f32 = 8.0;

use taffy::Size;

impl MenuBar {
    pub fn right_content(mut self, content: Box<dyn super::super::widget::Widget>) -> Self {
        self.right_content = Some(content);
        self
    }
    pub fn menubar_height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }
}

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
        let (_menubar, _open_id, mut callbacks) = make_menubar_with_groups();
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

        menubar.layout_style(&|_, _, _| Vec2::new(25.0, 12.0));

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
            focused_id: None,
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

    #[test]
    fn test_menu_hit_regions_follow_measured_glyph_widths() {
        for scale in [1.0, 1.5, 2.0] {
            let mut arena = StateArena::new();
            let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
            let file = arena.get_or_create(view_id, false);
            let edit = arena.get_or_create(view_id, false);
            let bar = MenuBar::new(
                vec![
                    MenuGroup {
                        label: "File".into(),
                        open_id: file,
                        items: vec![],
                    },
                    MenuGroup {
                        label: "Edit".into(),
                        open_id: edit,
                        items: vec![],
                    },
                ],
                None,
                38.0,
            )
            .compact();
            bar.layout_style(&|label, _, _| {
                Vec2::new(
                    if label == "File" {
                        60.0 * scale
                    } else {
                        15.0 * scale
                    },
                    12.0 * scale,
                )
            });
            let bounds = Rect2D::from_origin_size(Vec2::new(70.0, 0.0), Vec2::new(300.0, 38.0));
            // File has wide glyphs: this point would have fallen in Edit
            // with the former character-count approximation.
            let position = Vec2::new(70.0 + MENU_PADDING * 2.0 + 55.0 * scale, 15.0);
            let mut input = crate::input::UiInputState::default();
            input.set_mouse_pos(position);
            input.set_mouse_button(mouse_button::LEFT, true);
            let mut callbacks = CallbackTable::new();
            let mut actions = crate::declarative::actions::ActionStream::new();
            let mut ctx = InputContext {
                input: &input,
                mouse_pos: position,
                callbacks: &mut callbacks,
                actions: &mut actions,
                view_id,
                active_id: None,
                focused_id: None,
            };
            assert_eq!(
                bar.handle_input(&mut ctx, &mut arena, bounds, &[]),
                InputResult::Consumed
            );
            assert_eq!(arena.get::<bool>(file), Some(true));
            assert_eq!(arena.get::<bool>(edit), Some(false));
        }
    }

    #[test]
    fn test_menu_separator_consumes_click_without_closing() {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let open_id = arena.get_or_create(view_id, true);
        let bar = MenuBar::new(
            vec![MenuGroup {
                label: "File".into(),
                open_id,
                items: vec![
                    MenuEntry {
                        label: "Open".into(),
                        on_click: None,
                        disabled: false,
                    },
                    MenuEntry {
                        label: String::new(),
                        on_click: None,
                        disabled: false,
                    },
                    MenuEntry {
                        label: "Save".into(),
                        on_click: None,
                        disabled: false,
                    },
                ],
            }],
            None,
            38.0,
        );
        bar.layout_style(&|_, _, _| Vec2::new(25.0, 12.0));
        let bounds = Rect2D::from_origin_size(Vec2::ZERO, Vec2::new(800.0, 38.0));
        let position = Vec2::new(30.0, 70.0);
        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(position);
        input.set_mouse_button(mouse_button::LEFT, true);
        let mut callbacks = CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: position,
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id,
            active_id: None,
            focused_id: None,
        };
        assert_eq!(
            bar.handle_input(&mut ctx, &mut arena, bounds, &[]),
            InputResult::Consumed
        );
        assert_eq!(arena.get::<bool>(open_id), Some(true));
    }
}
