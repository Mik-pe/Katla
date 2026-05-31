use std::any::Any;

use katla_icons::ForkAwesome;
use katla_math::{Rect2D, Vec2};
use taffy::Style;

use super::super::animation::AnimationState;
use super::super::descriptor::Callback;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{InputContext, InputResult, Widget};
use crate::context::UiContext;
use crate::input::mouse_button;

pub(crate) struct Section {
    pub title: String,
    pub expanded_id: StateId,
    pub on_remove: Option<Callback>,
    children: Vec<ViewId>,
}

impl Widget for Section {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Section>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self) -> Style {
        Style::default()
    }

    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        state: &mut StateArena,
        bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        let font_size = 14.0_f32;
        let header_height = font_size + 8.0;

        let header_bounds =
            Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), header_height));

        if !header_bounds.contains(ctx.mouse_pos) {
            return InputResult::Ignore;
        }

        if !ctx.input.mouse_clicked(mouse_button::LEFT) {
            return InputResult::Ignore;
        }

        if self.on_remove.is_some() {
            let close_x = bounds.max.x() - font_size - 4.0;
            let close_bounds = Rect2D::from_origin_size(
                Vec2::new(close_x, bounds.min.y()),
                Vec2::new(font_size + 4.0, header_height),
            );
            if close_bounds.contains(ctx.mouse_pos) {
                if let Some(ref callback) = self.on_remove {
                    ctx.callbacks.invoke(callback, ctx.actions);
                }
                return InputResult::Consumed;
            }
        }

        let expanded: bool = state.get(self.expanded_id).unwrap_or_default();
        state.set(self.expanded_id, !expanded);
        InputResult::Consumed
    }

    fn draw(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        animation: &AnimationState,
        _children: &[ViewId],
    ) {
        let expanded: bool = state.get(self.expanded_id).unwrap_or_default();
        let font_size = ctx.style().font_size;

        let header_height = font_size + 8.0;
        let header_bounds =
            Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), header_height));
        ctx.draw_rect(header_bounds, ctx.style().window_title_bg);

        let chevron = if expanded {
            ForkAwesome::CHEVRON_DOWN
        } else {
            ForkAwesome::CHEVRON_RIGHT
        };
        let chevron_y = header_bounds.center().y() - font_size * 0.5;
        ctx.draw_icon(
            chevron,
            Vec2::new(bounds.min.x() + 4.0, chevron_y),
            font_size,
            ctx.style().text_color,
        );

        let title_x = bounds.min.x() + font_size + 8.0;
        ctx.draw_text(
            &self.title,
            Vec2::new(title_x, chevron_y),
            animation.apply_to_color(ctx.style().text_color),
            font_size,
        );

        if self.on_remove.is_some() {
            let close_x = bounds.max.x() - font_size - 4.0;
            ctx.draw_text(
                "\u{00d7}",
                Vec2::new(close_x, chevron_y),
                ctx.style().text_disabled,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_section(title: &str) -> Section {
        Section {
            title: title.to_string(),
            expanded_id: StateId::test_id(),
            on_remove: None,
            children: Vec::new(),
        }
    }

    fn make_section_with_remove() -> (Section, crate::declarative::CallbackTable) {
        let mut table = crate::declarative::build::CallbackTable::new();
        let cb = table.push(|_actions| {});
        (
            Section {
                title: "Test".to_string(),
                expanded_id: StateId::test_id(),
                on_remove: Some(cb),
                children: Vec::new(),
            },
            table,
        )
    }

    #[test]
    fn test_section_toggle_expanded() {
        let mut state = StateArena::new();
        let dummy_view = ViewId::from(slotmap::KeyData::from_ffi(0));
        let expanded_id = state.get_or_create(dummy_view, false);
        let section = Section {
            title: "Test".to_string(),
            expanded_id,
            on_remove: None,
            children: Vec::new(),
        };

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(50.0, 5.0));
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(50.0, 5.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
        };

        let bounds = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(200.0, 100.0));
        let result = section.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let expanded: bool = state.get(expanded_id).unwrap_or_default();
        assert!(expanded);
    }

    #[test]
    fn test_section_on_remove() {
        let (section, mut table) = make_section_with_remove();
        let mut state = StateArena::new();

        let font_size = 14.0_f32;
        let bounds = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(200.0, 100.0));

        let close_x = bounds.max.x() - font_size - 4.0;
        let click_pos = Vec2::new(close_x + 2.0, 5.0);

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(click_pos);
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: click_pos,
            callbacks: &mut table,
            actions: &mut actions,
        };

        let result = section.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);
    }

    #[test]
    fn test_section_diff() {
        let a = make_section("A");
        let b = make_section("B");
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }

    #[test]
    fn test_section_children() {
        let mut section = make_section("Test");
        assert!(section.children().is_empty());

        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        section.children_mut().push(view_id);
        assert_eq!(section.children().len(), 1);
    }

    #[test]
    fn test_section_focusable() {
        let section = make_section("Test");
        assert!(section.focusable());
    }
}
