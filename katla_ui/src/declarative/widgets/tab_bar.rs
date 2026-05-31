use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, FlexDirection, LengthPercentage, Size, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::TabItem;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{
    ChildWidgets, DrawInteraction, InputContext, InputResult, MeasureFn, Widget,
};
use crate::context::UiContext;
use crate::input::mouse_button;

pub struct TabBar {
    pub tabs: Vec<TabItem>,
    pub selected_id: StateId,
    pub child_widget: Option<Box<dyn super::super::widget::Widget>>,
    pub(crate) children: Vec<ViewId>,
}

impl TabBar {
    pub fn new(
        tabs: Vec<TabItem>,
        selected_id: StateId,
        child_widget: Option<Box<dyn super::super::widget::Widget>>,
    ) -> Self {
        Self {
            tabs,
            selected_id,
            child_widget,
            children: Vec::new(),
        }
    }
}

impl Widget for TabBar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<TabBar>().is_some() {
            DiffAction::RecurseChildren
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        let tab_height = 28.0_f32;
        Style {
            size: Size {
                width: Dimension::Percent(1.0),
                height: Dimension::Length(tab_height),
            },
            flex_direction: FlexDirection::Row,
            gap: Size {
                width: LengthPercentage::Length(0.0),
                height: LengthPercentage::Length(0.0),
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
        let tab_count = self.tabs.len().max(1);
        let tab_width = bounds.width() / tab_count as f32;

        if bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
            let tab_index = ((ctx.mouse_pos.x() - bounds.min.x()) / tab_width)
                .clamp(0.0, tab_count as f32 - 0.01) as usize;
            if tab_index < self.tabs.len() {
                state.set(self.selected_id, tab_index);
                return InputResult::Consumed;
            }
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
        _interaction: &DrawInteraction,
        _view_id: ViewId,
        _children_bounds: &[Rect2D],
    ) {
        let font_size = ctx.style().font_size;
        let selected: usize = state.get(self.selected_id).unwrap_or_default();
        let tab_count = self.tabs.len().max(1);
        let tab_width = bounds.width() / tab_count as f32;

        for (i, tab) in self.tabs.iter().enumerate() {
            let tab_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x() + i as f32 * tab_width, bounds.min.y()),
                Vec2::new(tab_width, bounds.height()),
            );
            let is_selected = i == selected;
            let is_tab_hovered = tab_bounds.contains(ctx.mouse_pos());

            let bg = if is_selected {
                ctx.style().selectable_selected
            } else if is_tab_hovered {
                ctx.style().button_hovered
            } else {
                ctx.style().button_normal
            };
            ctx.draw_rect(tab_bounds, animation.apply_to_color(bg));

            if is_selected {
                ctx.draw_line(
                    Vec2::new(tab_bounds.min.x(), tab_bounds.max.y() - 2.0),
                    Vec2::new(tab_bounds.max.x(), tab_bounds.max.y() - 2.0),
                    animation.apply_to_color(ctx.style().text_color),
                    2.0,
                );
            }

            let label_size = ctx.measure_text(&tab.label, font_size);
            let text_pos = Vec2::new(
                tab_bounds.center().x() - label_size.x() * 0.5,
                tab_bounds.center().y() - label_size.y() * 0.5,
            );
            ctx.draw_text(
                &tab.label,
                text_pos,
                animation.apply_to_color(ctx.style().text_color),
                font_size,
            );
        }
    }

    fn focusable(&self) -> bool {
        true
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

    fn interactive(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tab_bar() -> TabBar {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let selected_id = arena.get_or_create(view_id, 0usize);
        TabBar::new(
            vec![
                TabItem {
                    label: "Tab A".into(),
                },
                TabItem {
                    label: "Tab B".into(),
                },
            ],
            selected_id,
            None,
        )
    }

    #[test]
    fn test_tab_bar_diff_same_type() {
        let a = make_tab_bar();
        let b = make_tab_bar();
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_tab_bar_diff_different_type() {
        let tb = make_tab_bar();
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(tb.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_tab_bar_children() {
        let mut tb = make_tab_bar();
        assert!(tb.children().is_empty());
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        tb.children_mut().push(view_id);
        assert_eq!(tb.children().len(), 1);
    }

    #[test]
    fn test_tab_bar_click_switches_tab() {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let selected_id = arena.get_or_create(view_id, 0usize);

        let tab_bar = TabBar::new(
            vec![
                TabItem { label: "A".into() },
                TabItem { label: "B".into() },
                TabItem { label: "C".into() },
            ],
            selected_id,
            None,
        );

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(katla_math::Vec2::new(150.0, 10.0));
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: katla_math::Vec2::new(150.0, 10.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
        };

        let bounds = Rect2D::new(
            katla_math::Vec2::new(0.0, 0.0),
            katla_math::Vec2::new(300.0, 28.0),
        );
        let result = tab_bar.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let selected: usize = arena.get(selected_id).unwrap_or_default();
        assert_eq!(
            selected, 1,
            "clicking on second tab should set selected to 1"
        );
    }

    #[test]
    fn test_tab_bar_focusable() {
        let tb = make_tab_bar();
        assert!(tb.focusable());
    }
}
