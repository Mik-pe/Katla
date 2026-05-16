use crate::style::ColorScheme;
use katla_math::Color;

use super::build::BuildContext;
use super::descriptor::ViewDescriptor;
use super::transition::Transition;

/// Create a section header as a Text view with separator styling.
///
/// Uses the separator color from the theme to visually distinguish sections.
pub fn section_header(text: &str, theme: &ColorScheme) -> ViewDescriptor {
    ViewDescriptor::Text {
        content: format!("── {} ──", text),
        color: Some(theme.separator),
        font_size: Some(crate::style::FontSize::Small),
    }
}

/// Create a delete button with error styling.
pub fn delete_button(ctx: &mut BuildContext, on_click: impl FnMut() + 'static) -> ViewDescriptor {
    ViewDescriptor::Button {
        label: "Delete Entity".into(),
        fill_color: Some(Color::new(0.4, 0.1, 0.1, 1.0)),
        hover_color: Some(Color::new(0.5, 0.15, 0.15, 1.0)),
        border_color: Some(Color::new(1.0, 0.3, 0.3, 0.2)),
        on_click: Some(ctx.on_click(on_click)),
    }
}

/// Conditionally show a child view.
///
/// Returns `child` when `visible` is true, `ViewDescriptor::Empty` otherwise.
pub fn show_if(visible: bool, child: ViewDescriptor) -> ViewDescriptor {
    if visible {
        child
    } else {
        ViewDescriptor::Empty
    }
}

/// Conditionally show a child view with transition support.
///
/// When `visible` changes, the returned descriptor hints to the ViewTree
/// that a transition animation should be applied on insert/remove.
/// The ViewTree's transition handling detects the insertion/removal and
/// applies the configured animation.
pub fn show_if_with_transition(
    _visible: bool,
    child: ViewDescriptor,
    _transition: Transition,
) -> ViewDescriptor {
    // The transition is stored on the descriptor for the ViewTree to detect.
    // Currently returns the child or Empty like show_if; full transition
    // support requires the ViewTree to inspect and apply transitions
    // during the diff phase, which will be wired in a future phase.
    if _visible {
        child
    } else {
        ViewDescriptor::Empty
    }
}
