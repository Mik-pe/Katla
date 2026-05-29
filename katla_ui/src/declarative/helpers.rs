use crate::style::ColorScheme;
use crate::style::FontSize;
use katla_math::Color;

use super::actions::ActionStream;
use super::build::BuildContext;
use super::constructors::{button, text};
use super::descriptor::ViewDescriptor;
use super::transition::Transition;

/// Create a section header as a Text view with separator styling.
///
/// Uses the separator color from the theme to visually distinguish sections.
pub fn section_header(label: &str, theme: &ColorScheme) -> ViewDescriptor {
    text(format!("── {} ──", label))
        .color(theme.separator)
        .font_size(FontSize::Small)
}

/// Create a delete button with error styling.
pub fn delete_button(
    ctx: &mut BuildContext,
    on_click: impl FnMut(&mut ActionStream) + 'static,
) -> ViewDescriptor {
    button("Delete Entity")
        .fill(Color::new(0.4, 0.1, 0.1, 1.0))
        .hover(Color::new(0.5, 0.15, 0.15, 1.0))
        .border(Color::new(1.0, 0.3, 0.3, 0.2))
        .on_click(ctx.on_click(on_click))
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
/// The ViewTree's sync_tree detects the insertion/removal and applies
/// the configured animation.
pub fn show_if_with_transition(
    visible: bool,
    child: ViewDescriptor,
    transition: Transition,
) -> ViewDescriptor {
    if visible {
        ViewDescriptor::TransitionContainer {
            child: Box::new(child),
            transition,
        }
    } else {
        ViewDescriptor::Empty
    }
}

/// Conditionally show one of two branches with stable identity.
///
/// When `condition` is true, returns `if_true`; otherwise returns `if_false`.
/// Both branches are always expressed as descriptors so diffing can match
/// the node identity across frames without destroying state. Prefer this over
/// `show_if` when the hidden branch has its own state that should survive
/// toggling.
pub fn show_if_else(
    condition: bool,
    if_true: ViewDescriptor,
    if_false: ViewDescriptor,
) -> ViewDescriptor {
    if condition { if_true } else { if_false }
}
