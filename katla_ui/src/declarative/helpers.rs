use crate::style::ColorScheme;
use crate::style::FontSize;
use katla_math::Color;

use super::actions::ActionStream;
use super::build::BuildContext;
use super::constructors::{button, text};
use super::transition::Transition;
use super::widget::{Widget, WidgetBox};

/// Create a section header as a Text view with separator styling.
///
/// Uses the separator color from the theme to visually distinguish sections.
pub fn section_header(label: &str, theme: &ColorScheme) -> Box<dyn Widget> {
    text(format!("── {} ──", label))
        .color(theme.separator)
        .font_size(FontSize::Small)
        .boxed()
}

/// Create a delete button with error styling.
pub fn delete_button(
    ctx: &mut BuildContext,
    on_click: impl FnMut(&mut ActionStream) + 'static,
) -> Box<dyn Widget> {
    button("Delete Entity")
        .fill(Color::new(0.4, 0.1, 0.1, 1.0))
        .hover(Color::new(0.5, 0.15, 0.15, 1.0))
        .border(Color::new(1.0, 0.3, 0.3, 0.2))
        .on_click(ctx.on_click(on_click))
        .boxed()
}

/// Conditionally show a child view.
///
/// Returns `child` when `visible` is true, `empty().boxed()` otherwise.
pub fn show_if(visible: bool, child: Box<dyn Widget>) -> Box<dyn Widget> {
    if visible {
        child
    } else {
        super::constructors::empty().boxed()
    }
}

/// Conditionally show a child view with transition support.
///
/// When `visible` changes, the returned widget hints to the ViewTree
/// that a transition animation should be applied on insert/remove.
/// The ViewTree's sync_tree detects the insertion/removal and applies
/// the configured animation.
pub fn show_if_with_transition(
    visible: bool,
    child: Box<dyn Widget>,
    transition: Transition,
) -> Box<dyn Widget> {
    if visible {
        super::constructors::wrap_transition_container(child, transition).boxed()
    } else {
        super::constructors::empty().boxed()
    }
}

/// Conditionally show one of two branches with stable identity.
///
/// When `condition` is true, returns `if_true`; otherwise returns `if_false`.
/// Both branches are always expressed as widgets so diffing can match
/// the node identity across frames without destroying state. Prefer this over
/// `show_if` when the hidden branch has its own state that should survive
/// toggling.
pub fn show_if_else(
    condition: bool,
    if_true: Box<dyn Widget>,
    if_false: Box<dyn Widget>,
) -> Box<dyn Widget> {
    if condition { if_true } else { if_false }
}
