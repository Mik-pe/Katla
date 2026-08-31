use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;
use crate::style::ColorScheme;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, ViewId};
use super::super::widget::{DrawInfo, InputContext, InputResult, MeasureFn, Widget};

/// Height of the mock editor's title strip inside the preview.
const STRIP_HEIGHT: f32 = 8.0;
/// Vertical pitch of the fake code lines.
const LINE_PITCH: f32 = 7.0;
const LINE_HEIGHT: f32 = 3.0;
const LINE_LEADING: f32 = 5.0;
/// Horizontal inset of the mocked code area.
const CODE_INSET: f32 = 6.0;

/// A miniature mocked-editor thumbnail filled with a color scheme's own
/// colors — background, title strip, and syntax-like code lines. Used by
/// theme pickers so each option previews the theme it applies.
pub struct ThemePreview {
    pub scheme: ColorScheme,
}

impl ThemePreview {
    pub fn new(scheme: ColorScheme) -> Self {
        Self { scheme }
    }
}

impl Widget for ThemePreview {
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
                width: Dimension::Length(138.0),
                height: Dimension::Length(40.0),
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

        // Editor surface.
        ctx.draw_rounded_rect(bounds, s.window_bg, 4.0);
        ctx.draw_rounded_selection_border(bounds, s.panel_border, 1.0, 4.0);

        // Title strip (like the editor's tab bar row).
        let strip = Rect2D::new(
            Vec2::new(bounds.min.x() + 1.0, bounds.min.y() + 1.0),
            Vec2::new(bounds.max.x() - 1.0, bounds.min.y() + 1.0 + STRIP_HEIGHT),
        );
        ctx.draw_rect(strip, s.panel_bg);
        // A fake active tab on the strip.
        let tab = Rect2D::from_origin_size(
            Vec2::new(strip.min.x() + 4.0, strip.min.y() + 2.0),
            Vec2::new(18.0, 4.0),
        );
        ctx.draw_rect(tab, s.window_title_bg_active);

        // Code lines: keyword, plain, string, comment + an accent cursor.
        let first = bounds.min.y() + STRIP_HEIGHT + LINE_LEADING;
        let w = bounds.width();
        let lines = [
            (0.0, 0.42, s.highlight),
            (6.0, 0.30, s.text_secondary),
            (6.0, 0.36, s.success),
            (0.0, 0.22, s.text_muted),
        ];
        for (i, (indent, fraction, color)) in lines.iter().enumerate() {
            let y = first + i as f32 * LINE_PITCH;
            let width = (fraction * (w - CODE_INSET * 2.0)).max(2.0);
            let x = bounds.min.x() + CODE_INSET + indent;
            if x + width > bounds.max.x() - CODE_INSET {
                continue;
            }
            ctx.draw_rect(
                Rect2D::from_origin_size(Vec2::new(x, y), Vec2::new(width, LINE_HEIGHT)),
                *color,
            );
        }
        // Cursor after the second line.
        let cursor_x = bounds.min.x() + CODE_INSET + 6.0 + (0.30 * (w - CODE_INSET * 2.0)) + 2.0;
        if cursor_x + 1.0 < bounds.max.x() - CODE_INSET {
            ctx.draw_rect(
                Rect2D::from_origin_size(
                    Vec2::new(cursor_x, first + LINE_PITCH),
                    Vec2::new(1.0, LINE_HEIGHT),
                ),
                s.accent,
            );
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
    fn test_theme_preview_layout_is_fixed() {
        let scheme = ColorScheme::by_name("dark").expect("dark scheme exists");
        let preview = ThemePreview::new(scheme);
        let style = preview.layout_style(&crate::declarative::layout::measure_text_descriptor);
        assert_eq!(style.size.width, Dimension::Length(138.0));
        assert_eq!(style.size.height, Dimension::Length(40.0));
    }

    #[test]
    fn test_theme_preview_diff_same_type_updates() {
        let a = ThemePreview::new(ColorScheme::by_name("dark").expect("dark scheme exists"));
        let b = ThemePreview::new(ColorScheme::by_name("nord").expect("nord scheme exists"));
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }
}
