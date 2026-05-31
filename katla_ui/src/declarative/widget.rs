use std::any::{Any, TypeId};

use katla_math::{Rect2D, Vec2};
use taffy::Style;

use crate::context::UiContext;
use crate::style::FontSize;

use super::actions::ActionStream;
use super::animation::AnimationState;
use super::descriptor::ViewDescriptor;
use super::diff::{DiffAction, diff_descriptor};
use super::draw::draw_descriptor_with_id;
use super::state::{StateArena, ViewId};
use super::tree::InteractionState;

/// Function signature for measuring text dimensions during layout.
pub type MeasureFn<'a> = &'a dyn Fn(&str, Option<FontSize>) -> Vec2;

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
    ///
    /// The `measure` function provides text measurement. Widgets that don't
    /// need it can ignore the parameter.
    fn layout_style(&self, measure: MeasureFn<'_>) -> Style;

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
    ///
    /// `interaction` provides hover/active/focus state for the node.
    /// `view_id` identifies this node for interaction checks.
    /// `children_bounds` provides resolved bounds of child nodes.
    fn draw(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        animation: &AnimationState,
        children: &[ViewId],
        interaction: &DrawInteraction,
        view_id: ViewId,
        children_bounds: &[Rect2D],
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

impl Widget for Box<dyn Widget> {
    fn widget_type(&self) -> TypeId {
        (**self).widget_type()
    }

    fn as_any(&self) -> &dyn Any {
        (**self).as_any()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        (**self).as_any_mut()
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        (**self).diff_against(prev)
    }

    fn layout_style(&self, measure: MeasureFn<'_>) -> Style {
        (**self).layout_style(measure)
    }

    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        state: &mut StateArena,
        bounds: Rect2D,
        children: &[ViewId],
    ) -> InputResult {
        (**self).handle_input(ctx, state, bounds, children)
    }

    fn draw(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        animation: &AnimationState,
        children: &[ViewId],
        interaction: &DrawInteraction,
        view_id: ViewId,
        children_bounds: &[Rect2D],
    ) {
        (**self).draw(
            ctx,
            state,
            bounds,
            animation,
            children,
            interaction,
            view_id,
            children_bounds,
        )
    }

    fn should_rebuild(&self, prev: &dyn Widget) -> bool {
        (**self).should_rebuild(prev)
    }

    fn focusable(&self) -> bool {
        (**self).focusable()
    }

    fn children(&self) -> &[ViewId] {
        (**self).children()
    }

    fn children_mut(&mut self) -> &mut Vec<ViewId> {
        (**self).children_mut()
    }
}

/// Bridge widget that wraps a [`ViewDescriptor`] and implements [`Widget`].
///
/// Used during migration: constructors return `Box<dyn Widget>` backed by
/// `DescriptorWidget`, and the tree code extracts the descriptor via
/// [`descriptor()`](DescriptorWidget::descriptor).
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

    pub fn descriptor_mut(&mut self) -> &mut ViewDescriptor {
        &mut self.descriptor
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
        if let Some(other) = prev.as_any().downcast_ref::<DescriptorWidget>() {
            diff_descriptor(&other.descriptor, &self.descriptor)
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
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
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        animation: &AnimationState,
        _children: &[ViewId],
        _interaction: &DrawInteraction,
        view_id: ViewId,
        children_bounds: &[Rect2D],
    ) {
        let interaction_state = InteractionState::default();
        draw_descriptor_with_id(
            &self.descriptor,
            ctx,
            bounds,
            state,
            children_bounds,
            &interaction_state,
            view_id,
            animation,
        );
    }

    fn should_rebuild(&self, _prev: &dyn Widget) -> bool {
        true
    }

    fn focusable(&self) -> bool {
        false
    }

    fn children(&self) -> &[ViewId] {
        &[]
    }

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

/// Interaction state passed to Widget::draw for visual feedback.
pub struct DrawInteraction {
    pub hovered_id: Option<ViewId>,
    pub active_id: Option<ViewId>,
    pub focused_id: Option<ViewId>,
}

impl DrawInteraction {
    pub fn is_hovered(&self, id: ViewId) -> bool {
        self.hovered_id == Some(id)
    }

    pub fn is_active(&self, id: ViewId) -> bool {
        self.active_id == Some(id)
    }

    pub fn is_focused(&self, id: ViewId) -> bool {
        self.focused_id == Some(id)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::layout::measure_text_descriptor;
    use crate::declarative::widget::Widget;

    /// A minimal widget for testing trait object safety.
    struct TestWidget;

    impl Widget for TestWidget {
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn diff_against(&self, _prev: &dyn Widget) -> DiffAction {
            DiffAction::Update
        }
        fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
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
            _interaction: &DrawInteraction,
            _view_id: ViewId,
            _children_bounds: &[Rect2D],
        ) {
        }
    }

    #[test]
    fn test_widget_trait_object_safe() {
        fn assert_widget(_: &dyn Widget) {}
        assert_widget(&TestWidget);
    }

    #[test]
    fn test_input_result_variants() {
        assert_eq!(InputResult::Consumed, InputResult::Consumed);
        assert_ne!(InputResult::Consumed, InputResult::Bubble);
    }

    #[test]
    fn test_draw_interaction_helpers() {
        let id = ViewId::from(slotmap::KeyData::from_ffi(1));
        let interaction = DrawInteraction {
            hovered_id: Some(id),
            active_id: None,
            focused_id: Some(id),
        };
        assert!(interaction.is_hovered(id));
        assert!(!interaction.is_active(id));
        assert!(interaction.is_focused(id));
    }

    #[test]
    fn test_layout_style_with_measure() {
        let w = TestWidget;
        let style = w.layout_style(&measure_text_descriptor);
        assert_eq!(style, Style::default());
    }
}
