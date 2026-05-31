use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, FlexDirection, FlexWrap, LengthPercentage, Size, Style};

use super::super::animation::AnimationState;
use super::super::descriptor::FlexProps;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{
    ChildWidgets, DrawInteraction, InputContext, InputResult, MeasureFn, Widget,
};
use crate::context::UiContext;

pub struct Grid {
    pub columns: usize,
    pub cell_size: Vec2,
    pub spacing: f32,
    pub flex: FlexProps,
    pub child_widgets: Vec<super::super::constructors::KeyedChild>,
    children: Vec<ViewId>,
}

impl Grid {
    pub fn new(
        columns: usize,
        cell_size: Vec2,
        spacing: f32,
        flex: FlexProps,
        child_widgets: Vec<super::super::constructors::KeyedChild>,
    ) -> Self {
        Self {
            columns,
            cell_size,
            spacing,
            flex,
            child_widgets,
            children: Vec::new(),
        }
    }
}

impl Widget for Grid {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Grid>().is_some() {
            DiffAction::RecurseChildren
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        let col_width = self.cell_size.x();
        let row_height = self.cell_size.y();
        let rows = (self.children.len().max(1) + self.columns - 1) / self.columns.max(1);
        let mut style = Style {
            size: Size {
                width: Dimension::Length(
                    col_width * self.columns as f32 + self.spacing * (self.columns as f32 - 1.0),
                ),
                height: Dimension::Length(
                    row_height * rows as f32 + self.spacing * (rows as f32 - 1.0),
                ),
            },
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            gap: Size {
                width: LengthPercentage::Length(self.spacing),
                height: LengthPercentage::Length(self.spacing),
            },
            ..Style::default()
        };
        crate::declarative::layout::apply_flex_props(&mut style, &self.flex);
        style
    }

    fn handle_input(
        &self,
        _ctx: &mut InputContext<'_>,
        _state: &mut StateArena,
        _bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        InputResult::Ignore
    }

    fn draw(
        &self,
        _ctx: &mut UiContext,
        _state: &StateArena,
        _bounds: Rect2D,
        _animation: &AnimationState,
        _children: &[ViewId],
        _interaction: &DrawInteraction,
        _view_id: ViewId,
        _children_bounds: &[Rect2D],
    ) {
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
        let children: Vec<(Option<u64>, Box<dyn Widget>)> = self
            .child_widgets
            .drain(..)
            .map(|kc| (kc.key, kc.widget))
            .collect();
        ChildWidgets::Multi(children)
    }
}

impl Grid {
    pub fn grid_spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }
    pub fn flex_width(mut self, w: f32) -> Self {
        self.flex.width = Some(w);
        self
    }
    pub fn flex_height(mut self, h: f32) -> Self {
        self.flex.height = Some(h);
        self
    }
    pub fn flex_grow(mut self, grow: f32) -> Self {
        self.flex.flex_grow = grow;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_diff_same_type() {
        let a = Grid::new(3, Vec2::new(100.0, 50.0), 0.0, FlexProps::default(), vec![]);
        let b = Grid::new(4, Vec2::new(80.0, 40.0), 8.0, FlexProps::default(), vec![]);
        assert_eq!(b.diff_against(&a), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_grid_diff_different_type() {
        let grid = Grid::new(3, Vec2::new(100.0, 50.0), 0.0, FlexProps::default(), vec![]);
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(grid.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_grid_children() {
        let mut grid = Grid::new(3, Vec2::new(100.0, 50.0), 0.0, FlexProps::default(), vec![]);
        assert!(grid.children().is_empty());
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        grid.children_mut().push(view_id);
        assert_eq!(grid.children().len(), 1);
    }

    #[test]
    fn test_grid_layout_style() {
        let grid = Grid::new(3, Vec2::new(100.0, 50.0), 8.0, FlexProps::default(), vec![]);
        let style = grid.layout_style(&crate::declarative::layout::measure_text_descriptor);
        assert_eq!(style.flex_direction, FlexDirection::Row);
        assert_eq!(style.flex_wrap, FlexWrap::Wrap);
    }
}
