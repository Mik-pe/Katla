use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;
use crate::style::ColorScheme;
use crate::tokens;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInfo, InputContext, InputResult, MeasureFn, Widget};

/// Swatch geometry: fixed strip with an inset color bar.
const SWATCH_WIDTH: f32 = 44.0;
const SWATCH_HEIGHT: f32 = 16.0;
const BAR_INSET: f32 = 4.0;
const BAR_HEIGHT: f32 = 6.0;

/// A compact color strip previewing a color scheme — background chip with a
/// segmented bar of the scheme's accent colors. Used by theme choosers so
/// each option is recognizable without rendering miniature editor mockups.
pub struct ThemeSwatch {
    pub scheme: ColorScheme,
}

impl ThemeSwatch {
    pub fn new(scheme: ColorScheme) -> Self {
        Self { scheme }
    }
}

impl Widget for ThemeSwatch {
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
                width: Dimension::Length(SWATCH_WIDTH),
                height: Dimension::Length(SWATCH_HEIGHT),
            },
            ..Style::default()
        }
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
        ctx: &mut UiContext,
        _state: &StateArena,
        bounds: Rect2D,
        _animation: &AnimationState,
        _children: &[ViewId],
        _info: &DrawInfo,
    ) {
        let s = &self.scheme;

        ctx.draw_rounded_rect(bounds, s.window_bg, tokens::RADIUS_CONTROL);
        ctx.draw_rounded_selection_border(bounds, s.panel_border, 1.0, tokens::RADIUS_CONTROL);

        // Segmented bar: accent / highlight / success in fixed proportions.
        let bar_y = bounds.center().y() - BAR_HEIGHT * 0.5;
        let bar_width = bounds.width() - BAR_INSET * 2.0;
        let segments = [(0.45, s.accent), (0.30, s.highlight), (0.25, s.success)];
        let mut x = bounds.min.x() + BAR_INSET;
        for (fraction, color) in segments {
            let width = fraction * bar_width - 2.0;
            ctx.draw_rounded_rect(
                Rect2D::from_origin_size(Vec2::new(x, bar_y), Vec2::new(width, BAR_HEIGHT)),
                color,
                1.0,
            );
            x += fraction * bar_width;
        }
    }

    fn focusable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_swatch_layout_is_fixed() {
        let scheme = ColorScheme::by_name("dark").expect("dark scheme exists");
        let swatch = ThemeSwatch::new(scheme);
        let style = swatch.layout_style(&crate::declarative::layout::measure_text_descriptor);
        assert_eq!(style.size.width, Dimension::Length(SWATCH_WIDTH));
        assert_eq!(style.size.height, Dimension::Length(SWATCH_HEIGHT));
    }

    #[test]
    fn test_theme_swatch_diff_same_type_updates() {
        let a = ThemeSwatch::new(ColorScheme::by_name("dark").expect("dark scheme exists"));
        let b = ThemeSwatch::new(ColorScheme::by_name("nord").expect("nord scheme exists"));
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }
}
