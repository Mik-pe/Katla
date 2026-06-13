use std::any::{Any, TypeId};

use katla_math::{Rect2D, Vec2};
use taffy::Style;

use crate::context::UiContext;
use crate::style::FontSize;

use super::actions::ActionStream;
use super::animation::AnimationState;
use super::descriptor::Alignment;
use super::diff::DiffAction;
use super::state::{StateArena, ViewId};

/// Function signature for measuring text dimensions during layout.
pub type MeasureFn<'a> = &'a dyn Fn(&str, Option<FontSize>) -> Vec2;

/// Extracted children from a container widget.
///
/// Container widgets return their children via [`Widget::take_children()`].
/// The tree uses this during `sync_tree` to discover and recurse into children.
pub enum ChildWidgets {
    /// Leaf widget with no children.
    None,
    /// Single-child container (Panel, ScrollView, Overlay, etc.).
    Single(Box<dyn Widget>),
    /// Multi-child container with optional keys (HStack, VStack, Grid).
    Multi(Vec<(Option<u64>, Box<dyn Widget>)>),
    /// ZStack with per-child alignment.
    ZStack(Vec<(Alignment, Option<u64>, Box<dyn Widget>)>),
    /// TransitionContainer wrapping a single child with transition config.
    Transition {
        child: Box<dyn Widget>,
        transition: super::transition::Transition,
    },
}

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
    /// `info` provides interaction state, node identity, and resolved child bounds.
    fn draw(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        animation: &AnimationState,
        children: &[ViewId],
        info: &DrawInfo,
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

    /// Extract child widgets from this container during tree sync.
    ///
    /// Container widgets take their child widgets out and return them.
    /// Leaf widgets return [`ChildWidgets::None`].
    fn take_children(&mut self) -> ChildWidgets {
        ChildWidgets::None
    }

    /// Compute position delta for special positioning (Overlay, Modal, DraggablePanel, ZStack).
    ///
    /// Returns the offset to apply to the taffy-computed bounds.
    /// For ZStack children, computes alignment-based offset from parent and child bounds.
    fn resolve_position_delta(
        &self,
        bounds: Rect2D,
        parent_bounds: Rect2D,
        zstack_alignment: Option<Alignment>,
        _state: &StateArena,
    ) -> Vec2 {
        match zstack_alignment {
            Some(alignment) => {
                let dx = parent_bounds.width() - bounds.width();
                let dy = parent_bounds.height() - bounds.height();
                let hx = match alignment {
                    Alignment::TopLeading | Alignment::Leading | Alignment::BottomLeading => 0.0,
                    Alignment::Top | Alignment::Center | Alignment::Bottom => dx * 0.5,
                    Alignment::TopTrailing | Alignment::Trailing | Alignment::BottomTrailing => dx,
                    Alignment::BottomCenter => dx * 0.5,
                };
                let vy = match alignment {
                    Alignment::TopLeading | Alignment::Top | Alignment::TopTrailing => 0.0,
                    Alignment::Leading | Alignment::Center | Alignment::Trailing => dy * 0.5,
                    Alignment::BottomLeading
                    | Alignment::Bottom
                    | Alignment::BottomTrailing
                    | Alignment::BottomCenter => dy,
                };
                let target = parent_bounds.min + Vec2::new(hx, vy);
                target - bounds.min
            }
            None => Vec2::ZERO,
        }
    }

    /// Whether this node should clip its children to its bounds.
    fn needs_clip_children(&self) -> bool {
        false
    }

    /// Whether children should be drawn (e.g. collapsed Section, closed Modal).
    fn should_draw_children(&self, _state: &StateArena) -> bool {
        true
    }

    /// Scroll offset for ScrollView content.
    fn scroll_offset(&self, _state: &StateArena) -> f32 {
        0.0
    }

    /// Whether this widget participates in hit testing for input.
    fn interactive(&self) -> bool {
        false
    }

    /// Whether this widget should handle input even when the mouse is outside
    /// its layout bounds (e.g. MenuBar with open dropdowns).
    ///
    /// When `true`, the input dispatch loop will call `handle_input` on this
    /// widget as a secondary pass, regardless of hit-test results.
    fn wants_global_input(&self, _state: &StateArena) -> bool {
        false
    }

    /// Whether this widget creates an isolated focus scope.
    ///
    /// Focus scope widgets (Panel, Modal, DraggablePanel) limit Tab/Shift+Tab
    /// navigation to their descendants only.
    fn is_focus_scope(&self) -> bool {
        false
    }

    /// Whether this focus scope traps focus when active.
    ///
    /// Returns `true` when the scope should prevent focus from escaping.
    /// Modal returns `true` when open. Regular scopes (Panel, DraggablePanel)
    /// return `false` — focus can leave via mouse click.
    fn focus_scope_trap(&self, _state: &StateArena) -> bool {
        false
    }

    /// Access the transition config if this is a TransitionContainer.
    fn as_transition(&self) -> Option<&super::transition::Transition> {
        None
    }

    /// Draw overlay elements after children have been drawn.
    ///
    /// Called by the pipeline after all children are drawn. Widgets that need
    /// to render elements on top of their children (e.g., DockSpace chrome)
    /// override this method.
    fn draw_after_children(
        &self,
        _ctx: &mut UiContext,
        _state: &StateArena,
        _bounds: Rect2D,
        _children: &[ViewId],
        _children_bounds: &[Rect2D],
    ) {
    }
}

/// Extension trait providing `.boxed()` on all widget types.
///
/// Converts a concrete widget type into `Box<dyn Widget>` for use in
/// container constructors and the `Build` trait.
pub trait WidgetBox: Widget + Sized {
    fn boxed(self) -> Box<dyn Widget> {
        Box::new(self)
    }
}

impl<W: Widget> WidgetBox for W {}

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
        info: &DrawInfo,
    ) {
        (**self).draw(ctx, state, bounds, animation, children, info)
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

    fn take_children(&mut self) -> ChildWidgets {
        (**self).take_children()
    }

    fn resolve_position_delta(
        &self,
        bounds: Rect2D,
        parent_bounds: Rect2D,
        zstack_alignment: Option<Alignment>,
        state: &StateArena,
    ) -> Vec2 {
        (**self).resolve_position_delta(bounds, parent_bounds, zstack_alignment, state)
    }

    fn needs_clip_children(&self) -> bool {
        (**self).needs_clip_children()
    }

    fn should_draw_children(&self, state: &StateArena) -> bool {
        (**self).should_draw_children(state)
    }

    fn scroll_offset(&self, state: &StateArena) -> f32 {
        (**self).scroll_offset(state)
    }

    fn interactive(&self) -> bool {
        (**self).interactive()
    }

    fn wants_global_input(&self, state: &StateArena) -> bool {
        (**self).wants_global_input(state)
    }

    fn is_focus_scope(&self) -> bool {
        (**self).is_focus_scope()
    }

    fn focus_scope_trap(&self, state: &StateArena) -> bool {
        (**self).focus_scope_trap(state)
    }

    fn as_transition(&self) -> Option<&super::transition::Transition> {
        (**self).as_transition()
    }

    fn draw_after_children(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        children: &[ViewId],
        children_bounds: &[Rect2D],
    ) {
        (**self).draw_after_children(ctx, state, bounds, children, children_bounds)
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

/// Per-node drawing info passed to [`Widget::draw()`].
///
/// Bundles interaction state, node identity, and resolved child bounds
/// into a single struct to keep the trait method signature manageable.
pub struct DrawInfo<'a> {
    pub interaction: &'a DrawInteraction,
    pub view_id: ViewId,
    pub children_bounds: &'a [Rect2D],
}

/// Tracks interactive state across frames for the declarative view tree.
///
/// Analogous to the immediate mode `active_id`/`hovered_id`/`focused_id` pattern,
/// but stored on the retained tree for cross-frame interactions like slider drags.
#[derive(Default)]
pub struct InteractionState {
    /// Node being actively pressed/dragged (e.g. slider thumb mid-drag).
    pub active_id: Option<ViewId>,
    /// Node currently under the mouse cursor.
    pub hovered_id: Option<ViewId>,
    /// Node with keyboard focus (synced with FocusManager).
    pub focused_id: Option<ViewId>,
    /// For Vec3Slider: which axis (0, 1, 2) is being dragged.
    pub drag_axis: Option<usize>,
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
    /// The view ID of the widget receiving input.
    pub view_id: ViewId,
    /// The currently active widget (e.g. slider being dragged), if any.
    pub active_id: Option<ViewId>,
    /// The currently focused widget, if any.
    pub focused_id: Option<ViewId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::layout::measure_text_descriptor;

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
            _info: &DrawInfo,
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

    #[test]
    fn test_default_trait_methods() {
        let mut w = TestWidget;
        assert!(matches!(w.take_children(), ChildWidgets::None));
        assert_eq!(
            w.resolve_position_delta(
                Rect2D::default(),
                Rect2D::default(),
                None,
                &StateArena::new()
            ),
            Vec2::ZERO
        );
        assert!(!w.needs_clip_children());
        assert!(w.should_draw_children(&StateArena::new()));
        assert_eq!(w.scroll_offset(&StateArena::new()), 0.0);
        assert!(!w.interactive());
        assert!(w.as_transition().is_none());
    }

    // VAL-CROSS-021: Widget trait downcasting works for various widget types
    #[test]
    fn test_widget_downcast_button() {
        use crate::declarative::widgets::button::Button;
        let w: Box<dyn Widget> = Box::new(Button {
            label: "Click".into(),
            fill_color: None,
            border_color: None,
            on_click: None,
        });
        assert!(
            w.as_any().downcast_ref::<Button>().is_some(),
            "should downcast to Button"
        );
        assert_eq!(w.as_any().downcast_ref::<Button>().unwrap().label, "Click");
        assert!(
            w.as_any()
                .downcast_ref::<crate::declarative::widgets::text::Text>()
                .is_none(),
            "Button should not downcast to Text"
        );
    }

    #[test]
    fn test_widget_downcast_text() {
        use crate::declarative::widgets::text::Text;
        let w: Box<dyn Widget> = Box::new(Text {
            content: "Hello".into(),
            color: None,
            font_size: None,
        });
        assert!(
            w.as_any().downcast_ref::<Text>().is_some(),
            "should downcast to Text"
        );
        assert_eq!(w.as_any().downcast_ref::<Text>().unwrap().content, "Hello");
    }

    #[test]
    fn test_widget_downcast_slider() {
        use crate::declarative::state::StateId;
        use crate::declarative::widgets::slider::Slider;
        let sid = StateId::test_id();
        let w: Box<dyn Widget> = Box::new(Slider {
            label: "Vol".into(),
            value_id: sid,
            range: 0.0..=1.0,
            show_value: false,
            precision: 2,
        });
        assert!(
            w.as_any().downcast_ref::<Slider>().is_some(),
            "should downcast to Slider"
        );
        assert_eq!(w.as_any().downcast_ref::<Slider>().unwrap().label, "Vol");
    }

    #[test]
    fn test_widget_downcast_toggle() {
        use crate::declarative::widgets::toggle::Toggle;
        let sid = crate::declarative::state::StateId::test_id();
        let w: Box<dyn Widget> = Box::new(Toggle {
            label: "On".into(),
            value_id: sid,
        });
        assert!(
            w.as_any().downcast_ref::<Toggle>().is_some(),
            "should downcast to Toggle"
        );
    }

    #[test]
    fn test_widget_downcast_empty() {
        use crate::declarative::widgets::button::Button;
        use crate::declarative::widgets::empty::Empty;
        let w: Box<dyn Widget> = Box::new(Empty);
        assert!(
            w.as_any().downcast_ref::<Empty>().is_some(),
            "should downcast to Empty"
        );
        assert!(
            w.as_any().downcast_ref::<Button>().is_none(),
            "Empty should not downcast to Button"
        );
    }

    #[test]
    fn test_widget_downcast_box_dyn_widget() {
        use crate::declarative::widgets::text::Text;
        let inner: Box<dyn Widget> = Box::new(Text {
            content: "inner".into(),
            color: None,
            font_size: None,
        });
        // Box<dyn Widget> delegates as_any to inner
        assert!(
            inner.as_any().downcast_ref::<Text>().is_some(),
            "boxed widget should downcast to concrete type"
        );
    }

    #[test]
    fn test_widget_downcast_mut() {
        use crate::declarative::widgets::text::Text;
        let mut w: Box<dyn Widget> = Box::new(Text {
            content: "Hello".into(),
            color: None,
            font_size: None,
        });
        {
            let text_ref = w.as_any_mut().downcast_mut::<Text>().unwrap();
            text_ref.content = "World".into();
        }
        assert_eq!(w.as_any().downcast_ref::<Text>().unwrap().content, "World");
    }
}
