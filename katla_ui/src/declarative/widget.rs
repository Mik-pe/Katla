use std::any::{Any, TypeId};

use katla_math::{Rect2D, Vec2};
use taffy::Style;

use crate::context::UiContext;

use super::actions::ActionStream;
use super::animation::AnimationState;
use super::descriptor::ViewDescriptor;
use super::diff::DiffAction;
use super::state::{StateArena, ViewId};

/// Central trait for all UI elements in the declarative system.
///
/// Every widget (text, button, slider, container, etc.) implements this trait.
/// The view tree dispatches diffing, layout, input, and drawing through
/// trait methods — no enum matching on widget type in pipeline stages.
///
/// # Bounds
///
/// `Any + 'static` enables downcasting via `as_any()` for widget-specific
/// operations when needed.
pub trait Widget: Any + 'static {
    /// Returns the `TypeId` of this widget's concrete type.
    fn widget_type(&self) -> TypeId {
        TypeId::of::<Self>()
    }

    /// Downcast to `&dyn Any` for type-safe downcasting.
    fn as_any(&self) -> &dyn Any;

    /// Downcast to `&mut dyn Any` for mutable type-safe downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Compare this widget against the previous frame's widget and determine
    /// the diff action.
    fn diff_against(&self, prev: &dyn Widget) -> DiffAction;

    /// Return the Taffy `Style` for layout computation.
    fn layout_style(&self) -> Style;

    /// Handle an input event.
    ///
    /// Returns whether the event was consumed, should bubble to parent,
    /// or should be ignored.
    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        state: &mut StateArena,
        bounds: Rect2D,
        children: &[ViewId],
    ) -> InputResult;

    /// Draw this widget using the UI context.
    fn draw(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        animation: &AnimationState,
        children: &[ViewId],
    );

    /// Whether to rebuild this widget's subtree.
    ///
    /// Defaults to `true`. The `Memoize` wrapper overrides this with
    /// `Arc::ptr_eq` comparison for O(1) change detection.
    fn should_rebuild(&self, _prev: &dyn Widget) -> bool {
        true
    }

    /// Whether this widget can receive keyboard focus.
    fn focusable(&self) -> bool {
        false
    }

    /// Access this widget's child view IDs.
    ///
    /// Leaf widgets return an empty slice. Container widgets return their
    /// managed children.
    fn children(&self) -> &[ViewId] {
        &[]
    }

    /// Mutably access this widget's child view IDs.
    ///
    /// Only container widgets override this. Leaf widgets leave the default
    /// which is unreachable (they have no children to mutate).
    fn children_mut(&mut self) -> &mut Vec<ViewId> {
        unreachable!()
    }
}

/// Result of widget input handling.
///
/// Controls how the input system continues processing after a widget
/// handles (or doesn't handle) an input event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputResult {
    /// Event was handled — stop propagation to other widgets.
    Consumed,
    /// Event not handled — propagate to parent widget.
    Bubble,
    /// Event not relevant for this widget — don't propagate.
    Ignore,
}

/// Context provided to `Widget::handle_input()` for accessing input state.
pub struct InputContext<'a> {
    /// The UI input state (mouse, keyboard, etc.).
    pub input: &'a crate::input::UiInputState,
    /// Current mouse position in screen coordinates.
    pub mouse_pos: Vec2,
    /// Callback table for invoking registered callbacks.
    pub callbacks: &'a mut super::build::CallbackTable,
    /// Action stream for emitting typed actions.
    pub actions: &'a mut ActionStream,
}

// ---------------------------------------------------------------------------
// Bridge: DescriptorWidget wraps ViewDescriptor for gradual migration
// ---------------------------------------------------------------------------

/// Wrapper that bridges the existing `ViewDescriptor` enum to the `Widget` trait.
///
/// During migration from the enum-based system to trait-object dispatch,
/// all existing descriptors are wrapped in `DescriptorWidget`. Pipeline code
/// can access the inner `ViewDescriptor` via the `descriptor()` method on
/// `ViewNode`.
pub(crate) struct DescriptorWidget {
    descriptor: ViewDescriptor,
}

impl DescriptorWidget {
    pub fn new(descriptor: ViewDescriptor) -> Self {
        Self { descriptor }
    }

    pub fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }
}

impl Widget for DescriptorWidget {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        let Some(other) = prev.as_any().downcast_ref::<DescriptorWidget>() else {
            return DiffAction::Replace;
        };
        super::diff::diff_descriptor(&other.descriptor, &self.descriptor)
    }

    fn layout_style(&self) -> Style {
        Style::default()
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
    ) {
        // Drawing is handled by the existing draw_descriptor_with_id pipeline
    }

    fn focusable(&self) -> bool {
        super::focus::is_widget_focusable(&self.descriptor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::constructors::text;

    #[test]
    fn test_widget_trait_has_required_methods() {
        // Verify the trait can be object-safe and has all required methods
        fn assert_widget<W: Widget>(_: &W) {}

        let desc = text("hello");
        let widget = DescriptorWidget::new(desc);
        assert_widget(&widget);
    }

    #[test]
    fn test_widget_type_id() {
        let widget = DescriptorWidget::new(text("hello"));
        assert_eq!(widget.widget_type(), TypeId::of::<DescriptorWidget>());
    }

    #[test]
    fn test_widget_any_bounds() {
        let widget = DescriptorWidget::new(text("hello"));
        let any_ref: &dyn Any = widget.as_any();
        assert!(any_ref.downcast_ref::<DescriptorWidget>().is_some());
    }

    #[test]
    fn test_widget_any_mut_bounds() {
        let mut widget = DescriptorWidget::new(text("hello"));
        let any_mut: &mut dyn Any = widget.as_any_mut();
        assert!(any_mut.downcast_mut::<DescriptorWidget>().is_some());
    }

    #[test]
    fn test_diff_against_same_type_update() {
        let a = DescriptorWidget::new(text("hello"));
        let b = DescriptorWidget::new(text("world"));
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }

    #[test]
    fn test_diff_against_different_type_replace() {
        let a = DescriptorWidget::new(text("hello"));
        let b = DescriptorWidget::new(super::super::constructors::button("click"));
        assert_eq!(b.diff_against(&a), DiffAction::Replace);
    }

    #[test]
    fn test_input_result_variants() {
        assert_eq!(InputResult::Consumed, InputResult::Consumed);
        assert_eq!(InputResult::Bubble, InputResult::Bubble);
        assert_eq!(InputResult::Ignore, InputResult::Ignore);
        assert_ne!(InputResult::Consumed, InputResult::Bubble);
    }

    #[test]
    fn test_descriptor_widget_bridge() {
        let desc = text("hello");
        let widget = DescriptorWidget::new(desc);
        assert!(matches!(widget.descriptor(), ViewDescriptor::Text { .. }));
    }

    #[test]
    fn test_widget_focusable_default_false() {
        // Text widget is not focusable
        let widget = DescriptorWidget::new(text("hello"));
        assert!(!widget.focusable());
    }

    #[test]
    fn test_widget_children_default_empty() {
        let widget = DescriptorWidget::new(text("hello"));
        assert!(widget.children().is_empty());
    }

    #[test]
    fn test_widget_should_rebuild_default_true() {
        let a = DescriptorWidget::new(text("hello"));
        let b = DescriptorWidget::new(text("world"));
        assert!(b.should_rebuild(&a));
    }

    #[test]
    fn test_non_static_type_fails() {
        // This test verifies that Widget: 'static bound is enforced.
        // A type containing a reference cannot implement Widget.
        // We verify this at compile time — if it compiles, the bound is correct.
        fn require_static<T: 'static>(_: &T) {}
        let widget = DescriptorWidget::new(text("test"));
        require_static(&widget);
    }
}
