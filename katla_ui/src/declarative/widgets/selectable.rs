use std::any::Any;

use katla_math::Rect2D;
use taffy::{FlexDirection, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::Callback;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;
use crate::input::mouse_button;

pub(crate) struct Selectable {
    pub on_click: Option<Callback>,
    pub selected: bool,
    pub child_widget: Option<Box<dyn super::super::widget::Widget>>,
    pub(crate) children: Vec<ViewId>,
}

impl Widget for Selectable {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Selectable>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        Style {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
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
        let radius = bounds.height() * 0.4;
        if self.selected {
            ctx.draw_rounded_rect(
                bounds,
                animation.apply_to_color(ctx.style().selectable_selected),
                radius,
            );
        }
    }

    fn focusable(&self) -> bool {
        self.on_click.is_some()
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

    fn make_selectable(selected: bool) -> Selectable {
        Selectable {
            on_click: None,
            selected,
            child_widget: None,
            children: Vec::new(),
        }
    }

    fn make_selectable_with_callback() -> (Selectable, crate::declarative::CallbackTable) {
        let mut table = crate::declarative::build::CallbackTable::new();
        let cb = table.push(|_actions| {});
        (
            Selectable {
                on_click: Some(cb),
                selected: false,
                child_widget: None,
                children: Vec::new(),
            },
            table,
        )
    }

    #[test]
    fn test_selectable_click_fires_callback() {
        let (selectable, mut table) = make_selectable_with_callback();

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(katla_math::Vec2::new(50.0, 10.0));
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: katla_math::Vec2::new(50.0, 10.0),
            callbacks: &mut table,
            actions: &mut actions,
        };

        let bounds = Rect2D::new(
            katla_math::Vec2::new(0.0, 0.0),
            katla_math::Vec2::new(200.0, 20.0),
        );
        let result = selectable.handle_input(&mut ctx, &mut StateArena::new(), bounds, &[]);
        assert_eq!(result, InputResult::Consumed);
    }

    #[test]
    fn test_selectable_diff() {
        let a = make_selectable(false);
        let b = make_selectable(true);
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }

    #[test]
    fn test_selectable_children() {
        let mut selectable = make_selectable(false);
        assert!(selectable.children().is_empty());

        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        selectable.children_mut().push(view_id);
        assert_eq!(selectable.children().len(), 1);
    }

    #[test]
    fn test_selectable_focusable_with_callback() {
        let (selectable, _) = make_selectable_with_callback();
        assert!(selectable.focusable());
    }

    #[test]
    fn test_selectable_not_focusable_without_callback() {
        let selectable = make_selectable(false);
        assert!(!selectable.focusable());
    }
}
