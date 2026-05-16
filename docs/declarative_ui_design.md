# Declarative UI Architecture for Katla

## 1. Architecture Overview

### Current State: Immediate Mode

The existing `katla_ui` crate follows a pure immediate mode pattern. Every frame, the application:

1. Calls `ui.begin(screen_size)`
2. Imperatively calls widget functions (`ui.add(Button::new(...).bounds(bounds))`)
3. Manually tracks state (scroll positions, text input cursors, edit buffers) in external structs
4. Calls `ui.end()` → `DrawList` → GPU render

This works well for debug overlays but shows friction in the editor code: large widget functions like `Inspector::ui()` pass 7+ `&mut` references, manually manage cursors, and duplicate layout logic.

### Target State: Declarative Layer on Top

The declarative system is a **retained view tree** that produces the same `DrawList` output. It does not replace the immediate mode core — it layers on top of it.

```
┌──────────────────────────────────┐
│  Declarative Views (new)         │  ← ViewDescriptor enum, state, diffing
│  katla_ui::declarative           │
├──────────────────────────────────┤
│  Immediate Mode Core (existing)  │  ← UiContext, widgets, drawing primitives
│  katla_ui::context               │
├──────────────────────────────────┤
│  GPU Pipeline (unchanged)        │  ← DrawList, Vertex, DrawCmd
│  katla_ui::draw_list             │
└──────────────────────────────────┘
```

**What stays:**
- `DrawList` and its batched rendering pipeline (`add_rect`, `add_rounded_rect_aa`, `finalize()`)
- `Vertex`, `DrawCmd`, `TextureId` types
- Font system (`FontSystem`, `FontId`)
- `UiStyle`, `ColorScheme`, `FontSize` — the entire style system
- All drawing primitives (`draw_rect`, `draw_text`, `draw_icon`, etc.)
- `UiInputState` for mouse/keyboard input
- `Response` interaction semantics (clicked, hovered, active, etc.)

**What gets layered:**
- Layout: Taffy-based flexbox + anchor-based positioning replaces manual cursor management
- State: Arena-allocated `State<T>` cells replace external `&mut` parameters
- Views: A `ViewDescriptor` enum with diffing replaces the `Widget` trait's full re-creation
- Animation: Tweening/transition system built on the retained tree

**The `Widget` trait remains** for immediate mode usage. The declarative layer internally uses `UiContext` drawing methods, but only the explicit-bounds variants (`draw_rect` with a `Rect2D`, never cursor-based helpers like `begin_row`/`end_row`).

### Retained View Tree Concept

```
ViewTree {
    nodes: SlotMap<ViewId, ViewNode>,
    state: HashMap<(ViewId, u32), Box<dyn Any>>,  // arena state cells
    taffy: TaffyTree,                               // layout nodes
    root: Option<ViewId>,
    dirty: bool,
}

ViewNode {
    descriptor: ViewDescriptor,     // The enum describing what this node is
    layout_id: TaffyNodeId,         // Flexbox layout node
    bounds: Rect2D,                 // Computed absolute screen bounds
    children: Vec<ViewId>,
    parent: Option<ViewId>,
    animations: Vec<Animation>,     // Active tweens
    state_version: u32,             // Bumped when descriptor changes
}
```

Each frame:
1. Re-evaluate root view's `build()`, producing a new `ViewDescriptor` tree
2. Diff the new descriptor tree against the existing `ViewNode` tree
3. Run Taffy layout (only if tree structure or constraints changed)
4. Walk the tree, calling `draw()` on each descriptor variant which emits into `DrawList`
5. The resulting `DrawList` is identical to what immediate mode produces

### Declarative Layer's UiContext Usage

The declarative layer calls only explicit-bounds drawing methods on `UiContext`:

| Safe for declarative layer | Immediate-mode only |
|---|---|
| `draw_rect(bounds, color)` | `begin_row()` / `end_row()` |
| `draw_text(text, pos, color, size)` | `begin_column()` / `end_column()` |
| `draw_icon(icon, pos, size, color)` | `set_cursor()` / `cursor()` |
| `draw_rounded_rect(bounds, color, r)` | `begin_grid()` / `end_grid()` |
| `draw_line(a, b, color, w)` | `grid_item()` |
| `draw_circle(center, r, color, segs)` | `add()` (Widget trait) |

The declarative layer never touches the cursor or layout stacks. It computes bounds via Taffy and passes them directly to draw primitives.

## 2. Core Types and Traits

### ViewDescriptor Enum

Rather than a trait with an associated `Body` type (which creates generic soup in Rust), the core type is a concrete enum. Every kind of view is a variant. This eliminates deeply nested generic types, works naturally with Rust's pattern matching, and provides structural identity for diffing for free (enum variants are the identity).

```rust
/// The core view descriptor — a cheap value describing what should appear on screen.
///
/// Produced by `build()` methods, diffed across frames to produce minimal updates.
#[derive(Clone)]
pub enum ViewDescriptor {
    /// Empty — renders nothing.
    Empty,

    /// A text label.
    Text {
        content: String,
        color: Option<Color>,
        font_size: Option<FontSize>,
    },

    /// A clickable button.
    Button {
        label: String,
        fill_color: Option<Color>,
        hover_color: Option<Color>,
        border_color: Option<Color>,
        on_click: Option<Callback>,
    },

    /// A slider bound to a value.
    Slider {
        label: String,
        value_id: StateId,
        range: RangeInclusive<f32>,
        show_value: bool,
        precision: usize,
    },

    /// A toggle (checkbox).
    Toggle {
        label: String,
        value_id: StateId,
    },

    /// A text input field.
    TextField {
        placeholder: String,
        value_id: StateId,
        on_submit: Option<Callback>,
    },

    /// A progress bar.
    Progress {
        value: f32,
        range: RangeInclusive<f32>,
        fill_color: Option<Color>,
    },

    /// A color picker button.
    ColorPicker {
        label: String,
        value_id: StateId,
    },

    /// An image.
    Image {
        texture: TextureId,
        uv: Option<Rect2D>,
        tint: Color,
    },

    /// Horizontal stack — children laid out left to right.
    HStack(Box<StackDescriptor>),

    /// Vertical stack — children laid out top to bottom.
    VStack(Box<StackDescriptor>),

    /// Z-axis stack — children overlap, later children on top.
    ZStack(Box<ZStackDescriptor>),

    /// Scrollable container.
    ScrollView(Box<ScrollDescriptor>),

    /// Panel with header.
    Panel(Box<PanelDescriptor>),

    /// Absolute-positioned overlay (tooltips, popups).
    Overlay(Box<OverlayDescriptor>),

    /// A raw custom draw callback — for cases not covered by built-in variants.
    Custom(CustomDrawFn),
}

/// Shared stack layout configuration.
#[derive(Clone)]
pub struct StackDescriptor {
    pub children: Vec<ViewDescriptor>,
    pub spacing: f32,
    pub padding: Padding,
    pub alignment: Alignment,
}

/// Z-stack with per-child alignment.
#[derive(Clone)]
pub struct ZStackDescriptor {
    pub children: Vec<(Alignment, ViewDescriptor)>,
    pub padding: Padding,
}

/// Scroll container.
#[derive(Clone)]
pub struct ScrollDescriptor {
    pub content: Box<ViewDescriptor>,
    pub scroll_state_id: StateId,
}

/// Panel with title bar.
#[derive(Clone)]
pub struct PanelDescriptor {
    pub title: String,
    pub content: Box<ViewDescriptor>,
    pub header_height: f32,
}

/// Absolute-positioned overlay.
#[derive(Clone)]
pub struct OverlayDescriptor {
    pub anchor: Anchor,
    pub offset: Vec2,
    pub content: Box<ViewDescriptor>,
}

/// Anchor position on screen or parent.
#[derive(Clone, Copy, Debug)]
pub enum Anchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopCenter,
    BottomCenter,
    Center,
}

/// Callback handle — indexes into a callback table on the ViewTree.
#[derive(Clone)]
pub struct Callback(pub u32);

/// Custom draw function pointer.
pub type CustomDrawFn = fn(&mut UiContext, Rect2D);
```

### The Build Trait

Instead of a `View` trait with an associated type that creates generic explosion, views implement a simple `build` method that returns `ViewDescriptor`:

```rust
/// Trait for types that can produce a view descriptor tree.
///
/// Implement this for your custom view types. The `build` method
/// is called each frame to produce the current view tree.
pub trait Build {
    /// Produce this frame's view descriptor tree.
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor;
}
```

`BuildContext` provides access to state and environment:

```rust
/// Context passed during `build()` to access state and environment.
pub struct BuildContext<'a> {
    node_id: ViewId,
    state_arena: &'a mut StateArena,
    callbacks: &'a mut CallbackTable,
    env: &'a Environment,
}

impl<'a> BuildContext<'a> {
    /// Get or create a state cell at this node.
    pub fn state<T: Clone + PartialEq + 'static>(&mut self, initial: T) -> StateId {
        self.state_arena.get_or_create(self.node_id, initial)
    }

    /// Read environment value (theme, config, etc.).
    pub fn env<T: Clone + 'static>(&self) -> Option<&T> {
        self.env.get::<T>()
    }

    /// Register a callback, returning a handle.
    pub fn on_click<F: FnMut() + 'static>(&mut self, f: F) -> Callback {
        self.callbacks.push(f)
    }
}
```

### Convenience: Blanket impl for closures

```rust
impl<F: FnMut(&mut BuildContext) -> ViewDescriptor + 'static> Build for F {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        (self)(ctx)
    }
}
```

### Why enum, not trait with associated type

The review identified that `type Body: View` associated types create several problems in Rust:

1. **Generic soup**: `VStack<HStack<Tuple<(ButtonView, Text, SliderView)>>>` makes error messages unreadable and balloons compile times
2. **`Box<dyn View>` defeats diffing**: Once you erase types, you can't distinguish `Text` from `Button` during diffing
3. **`impl View` return types are incompatible** with associated types — you'd need unstable TAITs

The enum approach solves all three: no generic parameters, diffing is a simple `match` on variants, and every type is concrete. The tradeoff is that the enum is not open-ended — adding new view types requires adding variants. This is acceptable for a game engine where the widget set is bounded and controlled.

## 3. State Management

### Arena-Based State

Instead of `Rc<RefCell<T>>` (runtime borrow checks, pointer chasing, reference counting overhead), state lives in a typed arena owned by `ViewTree`. The tree has exclusive `&mut` access during the frame, so no `RefCell` is needed.

```rust
slotmap::new_key_type! { pub struct StateId; }

/// Arena of typed state cells, keyed by (ViewId, slot_index).
pub struct StateArena {
    cells: HashMap<(ViewId, u32), StateCell>,
}

struct StateCell {
    value: Box<dyn Any>,
    dirty: bool,
}

impl StateArena {
    /// Get or create a state cell. Returns a StateId handle.
    pub fn get_or_create<T: Clone + PartialEq + 'static>(
        &mut self,
        node_id: ViewId,
        initial: T,
    ) -> StateId {
        // keyed by (node_id, next_slot_index_for_this_node)
        // ...
    }

    /// Read a state value. Panics if type mismatches.
    pub fn get<T: Clone + 'static>(&self, id: StateId) -> T {
        // Direct access, no RefCell, no runtime borrow check
        // ...
    }

    /// Write a state value. Marks dirty if value changed.
    pub fn set<T: PartialEq + 'static>(&mut self, id: StateId, value: T) {
        // Compare old vs new, set dirty flag only if changed
        // ...
    }
}
```

### Binding<T> — External State

`Binding<T>` connects external game/editor state to the declarative system. It is NOT stored in the arena — it is a wrapper around a user-provided getter/setter pair:

```rust
/// A two-way binding to external state.
///
/// Created by the application layer to connect game/editor data
/// into the declarative view tree.
pub struct Binding<T> {
    get: Box<dyn Fn() -> T>,
    set: Box<dyn Fn(T)>,
}

impl<T: Clone> Binding<T> {
    /// Create a binding from a getter and setter.
    pub fn new(get: impl Fn() -> T + 'static, set: impl Fn(T) + 'static) -> Self {
        Self {
            get: Box::new(get),
            set: Box::new(set),
        }
    }

    /// Create a binding from a mutable reference.
    ///
    /// The reference must remain valid for the lifetime of the binding.
    /// Typically used when the binding is created and consumed within the same frame.
    pub fn from_ref(value: &mut T) -> BindingRef<'_, T> {
        BindingRef { value }
    }

    /// Read the current value.
    pub fn get(&self) -> T {
        (self.get)()
    }

    /// Write a new value.
    pub fn set(&self, value: T) {
        (self.set)(value)
    }
}

/// Short-lived binding from a mutable reference. Avoids heap allocation.
pub struct BindingRef<'a, T> {
    value: &'a mut T,
}

impl<'a, T: Clone> BindingRef<'a, T> {
    pub fn get(&self) -> T {
        self.value.clone()
    }

    pub fn set(&mut self, val: T) {
        *self.value = val;
    }
}
```

Usage — connecting ECS data to a declarative view:

```rust
// In katla_app, constructing a binding to ECS state:
let health_binding = Binding::new(
    || world.get_health(player_entity),
    |v| world.set_health(player_entity, v),
);

// The declarative view uses it:
struct HealthBarView {
    health: Binding<f32>,
    max_health: f32,
}

impl Build for HealthBarView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let fraction = self.health.get() / self.max_health;
        ViewDescriptor::Progress {
            value: fraction,
            range: 0.0..=1.0,
            fill_color: Some(Color::rgb(0.3, 0.8, 0.3)),
        }
    }
}
```

### Reactive Update Cycle

The dirtiness model is simple and explicit:

```
Frame N:
  1. Process input events → update StateArena entries
  2. If any state cell is dirty (or on first frame):
       a. Re-evaluate build() starting from root
       b. Diff resulting ViewDescriptor tree against existing ViewNode tree
       c. Patch: insert new nodes, remove dead nodes, update changed props
       d. Sync Taffy tree with new node structure
       e. Run Taffy layout solver
       f. Clear dirty flags
  3. If no state changed: skip build/diff/layout, go straight to draw
  4. Walk tree, call draw() on each node → emits into DrawList
  5. Finalize DrawList
```

**Dirtiness is node-local**: when `StateArena::set()` is called, only the `dirty` flag on that cell is set. A global `tree_dirty` flag is also set as a fast check. On the next frame, the entire tree is rebuilt (since determining the minimal subtree to rebuild requires dependency tracking, which adds complexity for marginal gain). For a game UI with 50-200 nodes, full rebuild is < 0.1ms.

**Taffy layout is cached**: the Taffy tree is only re-run if the node structure changed (detected during diffing). If only a state value changed (e.g., slider position) but the tree shape is the same, layout is reused.

### Replacing the `&mut` Pattern

| Immediate Mode | Declarative |
|---|---|
| `&mut edit.pos` passed through 7 layers | `ctx.state([0.0; 3])` owned by view node |
| `&mut scroll_state` stored externally | `ctx.state(0.0f32)` scroll offset in arena |
| `&mut selected_entity` as parameter | `Binding<Option<EntityId>>` from parent |
| `&mut pending_actions: Vec<EditorAction>` | `ctx.emit(Action::DeleteEntity(id))` action stream |

### Action Stream

Emitted actions are collected during the frame and processed after drawing, avoiding mutation-while-rendering issues:

```rust
/// Actions emitted by views during build/draw.
pub struct ActionStream {
    actions: Vec<Box<dyn Any>>,
}

impl ActionStream {
    pub fn emit<A: 'static>(&mut self, action: A) {
        self.actions.push(Box::new(action));
    }

    /// Drain all actions of a specific type.
    pub fn drain<A: 'static>(&mut self) -> impl Iterator<Item = A> + '_ {
        // Filter and downcast, removing matched actions
        // ...
    }
}
```

Usage:
```rust
// During build:
let on_click = ctx.on_click(|| {
    ctx.emit(EditorAction::DeleteEntity(entity_id));
});

// After frame, in katla_app:
for action in action_stream.drain::<EditorAction>() {
    process_editor_action(action);
}
```

## 4. Layout System

### Taffy Integration

[Taffy](https://github.com/DioxusOS/taffy) is a Flexbox/Grid layout engine written in Rust:

- Pure Rust, no C dependencies
- Used by Dioxus, Bevy, and other Rust UI frameworks
- Implements CSS Flexbox and Grid layout
- Operates on a node tree, producing `Rect` bounds per node

```toml
# katla_ui/Cargo.toml
[dependencies]
taffy = "0.7"
```

### Why Taffy (and where it falls short)

Taffy's flexbox is ideal for editor panels (the inspector, hierarchy, preferences). For game HUDs, it can approximate most layouts, but game UI also needs **anchor-based positioning** — "pin this element to the bottom-right corner of the screen." The `Overlay` variant in `ViewDescriptor` handles this: it bypasses Taffy and positions children absolutely relative to an anchor point.

For the common cases (stacks of buttons, lists, forms), Taffy's flexbox is the right tool. For the game-specific cases (HUD anchors, minimap overlays), the `Overlay` + `Anchor` system handles it directly.

### Layout Containers

The stack variants in `ViewDescriptor` are the primary layout mechanism:

```rust
// HStack: horizontal arrangement
ViewDescriptor::HStack(Box::new(StackDescriptor {
    children: vec![
        ViewDescriptor::Text { content: "HP".into(), color: None, font_size: None },
        ViewDescriptor::Progress { value: 0.8, range: 0.0..=1.0, fill_color: None },
    ],
    spacing: 4.0,
    padding: Padding::all(8.0),
    alignment: Alignment::Center,
}))

// VStack: vertical arrangement
ViewDescriptor::VStack(Box::new(StackDescriptor {
    children: vec![/* ... */],
    spacing: 2.0,
    padding: Padding::all(12.0),
    alignment: Alignment::Leading,
}))

// ZStack: overlapping layers with per-child alignment
ViewDescriptor::ZStack(Box::new(ZStackDescriptor {
    children: vec![
        (Alignment::BottomLeading, health_bar_descriptor),
        (Alignment::TopTrailing, score_descriptor),
        (Alignment::BottomCenter, inventory_descriptor),
    ],
    padding: Padding::zero(),
}))
```

### Layout Properties

```rust
/// Common layout types.
#[derive(Clone, Copy, Debug)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    pub const fn all(v: f32) -> Self {
        Self { top: v, right: v, bottom: v, left: v }
    }
    pub const fn horizontal(v: f32) -> Self {
        Self { top: 0.0, right: v, bottom: 0.0, left: v }
    }
    pub const fn zero() -> Self {
        Self { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Alignment {
    Leading,       // Left in LTR
    Trailing,      // Right in LTR
    Center,
    Top,
    Bottom,
    TopLeading,
    TopTrailing,
    BottomLeading,
    BottomTrailing,
    BottomCenter,
}

/// Flex properties applied to any view descriptor via a wrapper.
#[derive(Clone)]
pub struct FlexProps {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub aspect_ratio: Option<f32>,
}

impl Default for FlexProps {
    fn default() -> Self {
        Self {
            width: None, height: None,
            min_width: None, min_height: None,
            max_width: None, max_height: None,
            flex_grow: 0.0, flex_shrink: 1.0,
            aspect_ratio: None,
        }
    }
}
```

Each `ViewNode` stores both its `ViewDescriptor` and its `FlexProps`. The Taffy tree is built from `FlexProps` + the tree structure of stack children.

### Layout to DrawList Bridge

After Taffy computes layout, bounds are accumulated from parent to children:

```rust
/// Resolve Taffy layout output to absolute Rect2D bounds.
fn resolve_bounds(
    taffy: &TaffyTree,
    node: TaffyNodeId,
    parent_offset: Vec2,
) -> Rect2D {
    let layout = taffy.layout(node).unwrap();
    let x = parent_offset.x() + layout.location.x;
    let y = parent_offset.y() + layout.location.y;
    let w = layout.size.width;
    let h = layout.size.height;
    Rect2D::new(
        Vec2::new(x, y),
        Vec2::new(x + w, y + h),
    )
}
```

The tree walk starts at the root with offset `(0, 0)` and passes each node's absolute position down to its children. Each `ViewNode` stores its computed `bounds: Rect2D` after layout.

### Text Intrinsic Sizing

Taffy needs to know the intrinsic size of text nodes. Before running layout, each `Text` descriptor's content is measured using the existing font system:

```rust
fn measure_text_node(text: &str, font_size: Option<FontSize>, ctx: &UiContext) -> Vec2 {
    let size = font_size.unwrap_or(ctx.style().font_size);
    ctx.measure_text(text, size)
}
```

This measurement feeds into Taffy's `SizeBaselinesAndMargins` via the measure function callback.

## 5. Input Routing and Focus Management

### Input Flow

Input flows through the declarative system in a dedicated pass, separate from the immediate mode input handling:

```
Frame N:
  1. ui.begin() — resets immediate mode state
  2. Declarative: input_pass() — hit-test, dispatch events
  3. Declarative: build/diff/layout — if dirty
  4. Declarative: draw_pass() — emit draw calls
  5. Immediate mode code (existing panels, debug overlay)
  6. ui.end() — finalize DrawList
```

The declarative input pass runs before immediate mode widgets, so both systems see the same input state. Input is consumed by the first system that handles it.

### Hit Testing

The declarative tree walks bounds in reverse Z-order (last drawn = topmost) to find the node under the cursor:

```rust
fn hit_test(
    nodes: &SlotMap<ViewId, ViewNode>,
    root: ViewId,
    mouse_pos: Vec2,
) -> Option<ViewId> {
    // Walk children in reverse order (topmost first)
    // Return the deepest leaf node whose bounds contain mouse_pos
    // Skip non-interactive nodes (Text, Progress, etc.)
}
```

### Focus Management

```rust
/// Tracks which declarative node has keyboard focus.
pub struct FocusManager {
    focused: Option<ViewId>,
    focus_chain: Vec<ViewId>,  // Tab order
}

impl FocusManager {
    /// Move focus to the next focusable node.
    pub fn focus_next(&mut self) { /* ... */ }

    /// Move focus to the previous focusable node.
    pub fn focus_prev(&mut self) { /* ... */ }

    /// Check if a node is focused.
    pub fn is_focused(&self, id: ViewId) -> bool { /* ... */ }
}
```

Focusable nodes are collected during the build pass (any node with an `on_click` callback, `TextField`, etc.). Tab navigation cycles through them.

### Coexistence with Immediate Mode Input

When both systems coexist, the declarative tree checks if it consumed the input (click/hover) and sets a flag. The immediate mode `UiContext` can check this flag to skip input for that frame:

```rust
impl UiContext {
    /// Returns true if the declarative system consumed input this frame.
    pub fn input_consumed_by_declarative(&self) -> bool {
        self.declarative_input_consumed
    }
}
```

This prevents double-handling (e.g., a declarative button and an immediate mode button both responding to the same click).

### Gamepad Navigation

```rust
/// Gamepad-driven directional navigation.
pub struct GamepadNavigator {
    focused: Option<ViewId>,
    spatial_map: SpatialIndex,  // 2D index of focusable nodes
}

impl GamepadNavigator {
    /// Move focus in a cardinal direction (D-pad / left stick).
    pub fn navigate(&mut self, direction: Direction, nodes: &SlotMap<ViewId, ViewNode>) {
        // Find the nearest focusable node in the given direction
        // using spatial indexing on node bounds
    }
}

#[derive(Clone, Copy)]
pub enum Direction {
    Up, Down, Left, Right,
}
```

Gamepad button mapping (A = confirm/activate, B = back/cancel) is handled at the application layer via the `ActionStream`.

## 6. Text Input and IME

### TextField State

Text input in the declarative system uses dedicated state in the arena:

```rust
/// State for a text input field.
pub struct TextFieldState {
    pub text: String,
    pub cursor_pos: usize,
    pub selection_anchor: Option<usize>,
    pub scroll_offset: f32,
    pub ime_cursor: Vec2,  // Position for IME candidate window
}
```

### IME Integration

The declarative system communicates IME requirements to the application layer:

```rust
/// IME-related information a TextField sends to the application.
pub struct ImeRequest {
    /// Screen-space position for the IME candidate window.
    pub cursor_rect: Rect2D,
    /// Whether IME is currently active.
    pub active: bool,
}
```

When a `TextField` node has focus, the `ViewTree` reports its IME requirements. The application forwards these to the windowing system (winit). Pre-edit text from the IME is stored in the arena and applied during the next build pass.

### Clipboard

Clipboard access goes through the existing `ClipboardProvider` trait:

```rust
pub trait ClipboardProvider {
    fn get(&mut self) -> Option<String>;
    fn set(&mut self, text: &str);
}
```

The `ViewTree` holds a `Box<dyn ClipboardProvider>` and passes it to `TextField` nodes during the input pass.

## 7. Animation and Transitions

### Animation Architecture

Animations are stored on `ViewNode` and ticked each frame:

```rust
/// An active animation on a view node.
pub struct Animation {
    id: AnimationId,
    property: AnimatedProperty,
    tween: Tween,
    start_time: f64,
    on_complete: Option<Callback>,
}

/// Properties that can be animated. Extensible — add new variants as needed.
#[derive(Clone)]
pub enum AnimatedProperty {
    Opacity(f32),           // Current opacity value
    OffsetX(f32),           // Horizontal offset in pixels
    OffsetY(f32),           // Vertical offset in pixels
    Scale(f32),             // Uniform scale factor
    CornerRadius(f32),      // Corner radius in pixels
    ColorChannel(u8, f32),  // (channel_index, value) for color transitions
}

/// A tween from one value to another over time.
#[derive(Clone)]
pub struct Tween {
    pub from: f32,
    pub to: f32,
    pub duration: f64,
    pub easing: Easing,
}

/// Easing functions.
#[derive(Clone)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
    Spring { stiffness: f32, damping: f32 },
}

/// Interpolation trait for animatable values.
pub trait Interpolate: Copy {
    fn lerp(self, other: Self, t: f32) -> Self;
}

impl Interpolate for f32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }
}
```

### Keyframe Animations

```rust
/// A keyframe animation with multiple stops.
pub struct KeyframeAnimation {
    pub property: AnimatedProperty,
    pub keyframes: Vec<Keyframe>,
    pub on_complete: Option<Callback>,
}

#[derive(Clone)]
pub struct Keyframe {
    pub time: f64,       // 0.0..1.0 normalized time
    pub value: f32,
    pub easing: Easing,
}
```

### Transition System

Transitions are triggered when nodes are inserted into or removed from the tree:

```rust
/// Transition configuration attached to a view descriptor.
#[derive(Clone)]
pub struct Transition {
    pub insert: Option<TweenConfig>,
    pub remove: Option<TweenConfig>,
    pub property: AnimatedProperty,
}

#[derive(Clone)]
pub struct TweenConfig {
    pub duration: f64,
    pub easing: Easing,
}

/// Helper to create common transitions.
impl Transition {
    pub fn fade(duration: f64) -> Self {
        Transition {
            insert: Some(TweenConfig { duration, easing: Easing::EaseOut }),
            remove: Some(TweenConfig { duration, easing: Easing::EaseIn }),
            property: AnimatedProperty::Opacity(1.0),
        }
    }

    pub fn slide_up(duration: f64) -> Self {
        Transition {
            insert: Some(TweenConfig { duration, easing: Easing::EaseOut }),
            remove: Some(TweenConfig { duration, easing: Easing::EaseIn }),
            property: AnimatedProperty::OffsetY(0.0),
        }
    }

    pub fn scale(from: f32, to: f32, duration: f64) -> Self {
        Transition {
            insert: Some(TweenConfig { duration, easing: Easing::Spring { stiffness: 300.0, damping: 20.0 } }),
            remove: Some(TweenConfig { duration, easing: Easing::EaseIn }),
            property: AnimatedProperty::Scale(to),
        }
    }
}
```

Transitions are stored on the parent node and applied to children as they appear/disappear.

### Animation Frame Integration

During the draw pass:

1. Tick all active animations based on `dt`
2. For each animated node, compute interpolated value
3. Apply as overrides before calling draw:
   - `Opacity` → multiply color alpha
   - `OffsetX/Y` → translate bounds
   - `Scale` → scale bounds from center
   - `CornerRadius` → override radius parameter
4. Remove completed animations, fire `on_complete` callbacks

### Conditional Rendering with Transitions

```rust
/// Conditionally show a child, with transition support.
///
/// When `visible` changes, the child animates in or out
/// rather than appearing/disappearing instantly.
pub fn show_if(
    visible: bool,
    child: ViewDescriptor,
    transition: Transition,
) -> ViewDescriptor {
    if visible {
        child
    } else {
        ViewDescriptor::Empty
    }
    // The ViewTree detects the insertion/removal and applies the transition
}
```

## 8. Widget Catalog

### Standard Declarative Widgets

All built-in widgets are `ViewDescriptor` variants or helper functions that produce them:

| Widget | Immediate Mode | Declarative |
|--------|---------------|-------------|
| Button | `Button::new("text").bounds(b)` | `ViewDescriptor::Button { label, on_click, .. }` |
| Label | `ui.draw_text(...)` | `ViewDescriptor::Text { content, .. }` |
| Slider | `LabeledSlider::new(label, &mut val, range).bounds(b)` | `ViewDescriptor::Slider { value_id, range, .. }` |
| TextInput | `ui.text_input(..., &mut TextInputState)` | `ViewDescriptor::TextField { value_id, .. }` |
| Checkbox | `ui.checkbox(..., &mut checked)` | `ViewDescriptor::Toggle { value_id, .. }` |
| ComboBox | `ui.combo_box(...)` | `Picker` helper producing `VStack` + state |
| ScrollArea | `ui.scroll_area(..., state, bounds, \|\| {...})` | `ViewDescriptor::ScrollView { content, scroll_state_id }` |
| Panel | `Panel::new("name").bounds(b).show(ui)` | `ViewDescriptor::Panel { title, content, .. }` |
| ColorPicker | `ColorPickerButton::new(...)` | `ViewDescriptor::ColorPicker { value_id, .. }` |
| ProgressBar | `ui.progress_bar(...)` | `ViewDescriptor::Progress { value, range, .. }` |
| TreeView | `ui.tree_view(...)` | `TreeView` helper producing recursive `VStack` + state |
| Tabs | `ui.tab_bar(...)` | `TabView` helper producing `VStack` + state |
| Image | `ui.draw_image(...)` | `ViewDescriptor::Image { texture, uv, .. }` |

### Helper Functions for Common Patterns

```rust
/// Create a labeled slider that reads/writes via a StateId.
pub fn labeled_slider(
    ctx: &mut BuildContext,
    label: &str,
    value_id: StateId,
    range: RangeInclusive<f32>,
) -> ViewDescriptor {
    ViewDescriptor::Slider {
        label: label.to_string(),
        value_id,
        range,
        show_value: true,
        precision: 2,
    }
}

/// Create a section header with a separator line.
pub fn section_header(text: &str, theme: &ColorScheme) -> ViewDescriptor {
    ViewDescriptor::Custom(move |ui, bounds| {
        let y = bounds.min.y() + 2.0;
        ui.draw_line(
            Vec2::new(bounds.min.x(), y),
            Vec2::new(bounds.max.x(), y),
            theme.separator,
            1.0,
        );
        let font_size = ui.scaled_font_size(FontSize::Small);
        ui.draw_text(text, Vec2::new(bounds.min.x(), y + 4.0), theme.text_accent, font_size);
    })
}

/// Create a delete button styled with error colors.
pub fn delete_button(ctx: &mut BuildContext, on_click: impl FnMut() + 'static) -> ViewDescriptor {
    ViewDescriptor::Button {
        label: "Delete Entity".into(),
        fill_color: Some(Color::new(0.4, 0.1, 0.1, 1.0)),
        hover_color: Some(Color::new(0.5, 0.15, 0.15, 1.0)),
        border_color: Some(Color::new(1.0, 0.3, 0.3, 0.2)),
        on_click: Some(ctx.on_click(on_click)),
    }
}
```

### Diffing

Since `ViewDescriptor` is an enum, diffing is a `match` on variants:

```rust
fn diff_descriptor(old: &ViewDescriptor, new: &ViewDescriptor) -> DiffAction {
    match (old, new) {
        // Same variant — update props in place, keep state and children
        (ViewDescriptor::Text { .. }, ViewDescriptor::Text { .. }) => DiffAction::Update,
        (ViewDescriptor::Button { .. }, ViewDescriptor::Button { .. }) => DiffAction::Update,
        (ViewDescriptor::HStack(_), ViewDescriptor::HStack(_)) => DiffAction::RecurseChildren,
        (ViewDescriptor::VStack(_), ViewDescriptor::VStack(_)) => DiffAction::RecurseChildren,
        // Different variant — replace the node (destroys state)
        _ => DiffAction::Replace,
    }
}

enum DiffAction {
    Update,           // Same type, update props
    RecurseChildren,  // Same container type, diff children
    Replace,          // Different type, tear down and rebuild
}
```

State cells are keyed by `(ViewId, slot_index)`. When a node is `Replace`d, its state is destroyed. When it's `Update`d, state persists. This avoids the "shifting slot positions" problem that React's keyless lists have — because the slot index is stable within a node, not across siblings.

For lists with dynamic ordering (e.g., a list of entities), explicit keys can be provided:

```rust
/// A keyed list for dynamic collections.
pub struct KeyedList {
    pub items: Vec<(String, ViewDescriptor)>,  // (key, descriptor)
}
```

Keyed lists diff by matching keys rather than position.

## 9. Example Code

### Custom View: Health Bar

```rust
struct HealthBarView {
    health_id: StateId,
    max_health: f32,
}

impl Build for HealthBarView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let fraction = ctx.state_arena.get::<f32>(self.health_id) / self.max_health;
        let pct = format!("{:.0}%", fraction * 100.0);

        ViewDescriptor::HStack(Box::new(StackDescriptor {
            children: vec![
                ViewDescriptor::Text {
                    content: pct,
                    color: Some(Color::WHITE),
                    font_size: Some(FontSize::Small),
                },
                ViewDescriptor::Progress {
                    value: fraction,
                    range: 0.0..=1.0,
                    fill_color: Some(Color::rgb(0.3, 0.8, 0.3)),
                },
            ],
            spacing: 8.0,
            padding: Padding::all(4.0),
            alignment: Alignment::Center,
        }))
    }
}
```

### Declarative Inspector Equivalent

```rust
struct InspectorView {
    selected: Binding<Option<EntityId>>,
}

struct InspectorEditState {
    pos: [f32; 3],
    rot: [f32; 3],
    scale: [f32; 3],
}

impl Build for InspectorView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let Some(entity_id) = self.selected.get() else {
            return ViewDescriptor::Text {
                content: "No entity selected".into(),
                color: None,
                font_size: None,
            };
        };

        let edit_id = ctx.state(InspectorEditState {
            pos: [0.0; 3],
            rot: [0.0; 3],
            scale: [1.0, 1.0, 1.0],
        });

        // Read state values
        let edit = ctx.state_arena.get::<InspectorEditState>(edit_id);

        ViewDescriptor::VStack(Box::new(StackDescriptor {
            children: vec![
                section_header("Transform", ctx.env::<ColorScheme>().unwrap()),
                ViewDescriptor::Slider {
                    label: "Position X".into(),
                    value_id: edit_id,  // In practice: a projected sub-state
                    range: -100.0..=100.0,
                    show_value: true,
                    precision: 2,
                },
                // ... more sliders for Y, Z, rotation, scale
                delete_button(ctx, || ctx.emit(EditorAction::DeleteEntity(entity_id))),
            ],
            spacing: 4.0,
            padding: Padding::all(12.0),
            alignment: Alignment::Leading,
        }))
    }
}
```

### Game HUD Example

```rust
struct GameHud {
    health: Binding<f32>,
    max_health: f32,
    score: Binding<u32>,
    minimap_texture: TextureId,
}

impl Build for GameHud {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let health_frac = self.health.get() / self.max_health;
        let score_text = format!("Score: {}", self.score.get());

        ViewDescriptor::ZStack(Box::new(ZStackDescriptor {
            children: vec![
                // Bottom-left: health bar
                (Alignment::BottomLeading, ViewDescriptor::HStack(Box::new(StackDescriptor {
                    children: vec![
                        ViewDescriptor::Text {
                            content: "HP".into(),
                            color: Some(Color::WHITE),
                            font_size: Some(FontSize::Small),
                        },
                        ViewDescriptor::Progress {
                            value: health_frac,
                            range: 0.0..=1.0,
                            fill_color: Some(Color::rgb(0.3, 0.8, 0.3)),
                        },
                    ],
                    spacing: 4.0,
                    padding: Padding::all(12.0),
                    alignment: Alignment::Center,
                }))),

                // Top-right: score
                (Alignment::TopTrailing, ViewDescriptor::Text {
                    content: score_text,
                    color: Some(Color::WHITE),
                    font_size: Some(FontSize::Large),
                }),

                // Bottom-right: minimap
                (Alignment::BottomTrailing, ViewDescriptor::Image {
                    texture: self.minimap_texture,
                    uv: None,
                    tint: Color::WHITE,
                }),
            ],
            padding: Padding::zero(),
        }))
    }
}
```

### Animation/Transition Example

```rust
struct NotificationView {
    message_id: StateId,
    visible_id: StateId,
}

impl Build for NotificationView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let visible = ctx.state_arena.get::<bool>(self.visible_id);
        let message = ctx.state_arena.get::<String>(self.message_id);

        if visible {
            ViewDescriptor::Text {
                content: message,
                color: Some(Color::WHITE),
                font_size: None,
            }
            .with_transition(Transition::fade(0.3))
        } else {
            ViewDescriptor::Empty
        }
    }
}
```

## 10. Migration Path

### Coexistence Strategy

The declarative layer is **opt-in**. Both systems share the same `DrawList` output:

```rust
fn generate_ui_draw_list(&mut self) {
    self.ui.begin(self.screen_size);

    // Declarative views — input pass, build, draw
    self.view_tree.frame(&mut self.ui);

    // Immediate mode — existing panels not yet migrated
    self.draw_debug_overlay(&mut self.ui);

    let draw_list = self.ui.end();
    self.ui_draw_list = draw_list;
}
```

`ViewTree::frame()` handles the full cycle: input → build → diff → layout → draw, all calling into `UiContext`.

### Panel-by-Panel Migration

1. **Phase 1**: Migrate toolbar (simple, few widgets)
2. **Phase 2**: Migrate inspector (most complex, validates state/interaction)
3. **Phase 3**: Migrate hierarchy, preferences, asset browser
4. **Phase 4**: Remove dead immediate mode code for migrated panels
5. Keep immediate mode API available for game HUDs and debug overlays

### Mixed Mode Within a Panel

If needed, immediate mode and declarative code can coexist within the same panel using `ViewDescriptor::Custom`:

```rust
// Use Custom to embed immediate mode code inside a declarative tree
ViewDescriptor::Custom(|ui, bounds| {
    // Immediate mode drawing with explicit bounds
    ui.draw_rounded_rect(bounds, ui.style().panel_bg, 4.0);
    ui.draw_text("Legacy widget", bounds.min, Color::WHITE, 14.0);
})
```

### Cross-System Z-Ordering

Z-ordering between declarative and immediate mode is determined by call order within the frame:
- Declarative views drawn first → lower z-index
- Immediate mode drawn after → higher z-index
- Both systems can use explicit z-index overrides if needed

### Popup Handling

The existing `UiContext` popup system works for immediate mode. The declarative system handles popups via `ViewDescriptor::Overlay`:

```rust
// Declarative popup — positioned relative to a trigger node
ViewDescriptor::Overlay(Box::new(OverlayDescriptor {
    anchor: Anchor::BottomLeft,
    offset: Vec2::new(0.0, -30.0),
    content: Box::new(popup_content),
}))
```

## 11. Performance Model

### Per-Frame Cost (Steady State — No Changes)

| Operation | Cost | Notes |
|---|---|---|
| Dirty check | ~0 | Single bool check |
| Layout | 0 | Cached from previous frame |
| Draw walk | O(nodes) | Same as immediate mode |
| Animation tick | O(animations) | Only active animations |

### Per-Frame Cost (State Changed)

| Operation | Cost | Notes |
|---|---|---|
| Build | O(nodes) | Re-evaluate all `build()` calls |
| Diff | O(nodes) | Enum variant comparison |
| Layout | O(nodes) | Taffy flexbox solver |
| Draw walk | O(nodes) | Same as above |

For 50-200 nodes (typical editor UI), the full cycle is under 0.1ms. For game HUDs with thousands of elements, the dirty check short-circuits avoid unnecessary work.

### Allocation Strategy

Zero allocations per frame in steady state:

- **ViewDescriptor trees**: reused across frames via a double-buffer (old tree + new tree swap)
- **Strings**: `ViewDescriptor::Text` content is `String`-owned; for frequently changing text (score counters, FPS), use `format!` into the descriptor (allocates per frame but is unavoidable for dynamic text)
- **Taffy nodes**: pooled and reused; diffing only adds/removes when tree structure changes
- **StateArena**: pre-allocated `HashMap`, entries persist across frames
- **Callback table**: pre-allocated `Vec`, cleared and re-populated each frame

### DrawList Batching

The current `DrawList` batches by `(texture, clip_rect, z_index)`. The declarative tree's draw walk produces draw calls in tree order, which may not be optimal for batching. If this becomes a bottleneck, a post-draw sort pass can reorder by `(texture, z_index)` before finalization. The existing `DrawList::finalize()` already sorts by z-index, so this is handled.

## 12. Testing Strategy

### Unit Tests

- **State arena**: get/set/dirty flag lifecycle
- **Diffing**: verify `DiffAction` for various old/new descriptor pairs
- **Layout**: verify Taffy output matches expected bounds for given flex configs
- **Callbacks**: verify callbacks fire when input hits the right bounds

### Snapshot Tests

- **ViewDescriptor tree snapshots**: serialize the descriptor tree before/after state changes, compare against golden files
- **DrawList snapshots**: verify the vertex/command output matches expected batches

### Integration Tests

- **Coexistence**: render a mixed declarative + immediate mode frame, verify DrawList output
- **Input routing**: simulate clicks at specific positions, verify the correct callback fires
- **Focus management**: verify Tab key cycles through focusable nodes

### Visual Regression (Future)

- Render declarative views to a headless surface, compare pixel output against reference images
- This requires a rendering context, so it's a Phase 3+ concern

## 13. Serialization and Data-Driven UI

### Motivation

Game UI is often authored by designers and loaded from data files (health bars, dialog trees, menus). The declarative system should support this.

### Approach

`ViewDescriptor` is a data type that can be serialized/deserialized:

```rust
// A subset of ViewDescriptor that is serializable (no callbacks or fn pointers)
#[derive(Serialize, Deserialize)]
pub enum ViewDescriptorData {
    Text { content: String, color: Option<[f32; 4]>, font_size: Option<String> },
    Button { label: String },
    Progress { value: f32, range: [f32; 2] },
    HStack { children: Vec<ViewDescriptorData>, spacing: f32, padding: [f32; 4] },
    VStack { children: Vec<ViewDescriptorData>, spacing: f32, padding: [f32; 4] },
    ZStack { children: Vec<([f32; 4], ViewDescriptorData)> },
    Image { texture_path: String },
    // ...
}
```

Data-driven views use string keys for bindings:

```json
{
    "type": "HStack",
    "children": [
        { "type": "Text", "content": "HP", "font_size": "Small" },
        { "type": "Progress", "value_binding": "player.health_fraction", "range": [0, 1] }
    ],
    "spacing": 4,
    "padding": [8, 8, 8, 8]
}
```

The application provides a `BindingResolver` that maps string keys to `Binding<T>` instances:

```rust
pub trait BindingResolver {
    fn resolve_f32(&self, key: &str) -> Option<Binding<f32>>;
    fn resolve_u32(&self, key: &str) -> Option<Binding<u32>>;
    fn resolve_string(&self, key: &str) -> Option<Binding<String>>;
    fn resolve_bool(&self, key: &str) -> Option<Binding<bool>>;
}
```

### Hot Reload

Data files can be watched for changes. On change, the `ViewDescriptorData` is reloaded and the view tree is rebuilt. This enables rapid iteration on UI layout without recompiling.

## 14. Dependency Considerations

### New Dependencies

| Crate | Version | Purpose | Notes |
|-------|---------|---------|-------|
| `taffy` | 0.7 | Flexbox/Grid layout | Pure Rust, used by Bevy, Dioxus |
| `slotmap` | 1.0 | Stable IDs for view tree nodes | Arena allocation with generational keys |

No other new dependencies. Animation, state management, diffing, and serialization are implemented within `katla_ui`.

### Dependency Rule Compliance

From `AGENTS.md`:
- **katla_ui** must NOT depend on: `katla_ecs`, `katla_app`
- **katla_ui** CAN depend on: `katla_math`, `katla_gfx`

The declarative layer lives entirely within `katla_ui`. It has no knowledge of ECS entities, game state, or the application layer. `Binding<T>` is generic — the application passes in ECS-derived data.

```toml
# katla_ui/Cargo.toml — additions
[dependencies]
taffy = "0.7"
slotmap = "1.0"
```

### Module Structure within `katla_ui`

```
katla_ui/src/
├── declarative/           # New module
│   ├── mod.rs             # Public API re-exports
│   ├── descriptor.rs      # ViewDescriptor enum + related types
│   ├── build.rs           # Build trait, BuildContext
│   ├── state.rs           # StateArena, StateId, Binding<T>
│   ├── actions.rs         # ActionStream
│   ├── tree.rs            # ViewTree (retained tree + diffing + frame lifecycle)
│   ├── layout.rs          # Taffy integration, Padding, Alignment, FlexProps
│   ├── focus.rs           # FocusManager, GamepadNavigator
│   ├── input.rs           # Hit testing, input dispatch
│   ├── animation.rs       # Tween, Easing, Animation, KeyframeAnimation
│   ├── transition.rs      # Transition, show_if helper
│   ├── draw.rs            # Per-variant draw dispatch (descriptor → UiContext calls)
│   ├── serialize.rs       # ViewDescriptorData, BindingResolver
│   └── helpers.rs         # section_header, delete_button, show_if, etc.
├── context/               # Existing (unchanged)
├── draw_list.rs           # Existing (unchanged)
├── style.rs               # Existing (unchanged)
├── widget.rs              # Existing (unchanged, immediate mode Widget trait)
├── response.rs            # Existing (unchanged)
└── lib.rs                 # Existing + pub mod declarative
```

## 15. Implementation Phases

### Phase 1: Foundation (3-4 days)
**Goal**: Core types, state arena, tree storage — no visual output.

- `ViewDescriptor` enum
- `Build` trait and `BuildContext`
- `StateArena` with `StateId` handles
- `Binding<T>` and `BindingRef<T>`
- `ViewTree` with `SlotMap`-based node storage
- `ActionStream`
- Basic tree diffing (enum variant matching)
- Unit tests for state lifecycle and diffing

**Deliverable**: Tests creating a tree, mutating state, verifying diff output.

### Phase 2: Layout (3-4 days)
**Goal**: Taffy integration producing `Rect2D` bounds.

- Add `taffy` and `slotmap` dependencies
- `HStack`, `VStack`, `ZStack` descriptor handling
- `Padding`, `Alignment`, `FlexProps` types
- Taffy node tree synchronized with view tree
- `resolve_bounds` with absolute coordinate accumulation
- Text intrinsic sizing (measure via font system)
- Tests verifying layout output matches expected bounds

**Deliverable**: Layout tests that create descriptor trees and assert computed bounds.

### Phase 3: Drawing + Input (4-5 days)
**Goal**: Views render and respond to input.

- Per-variant draw dispatch (`ViewDescriptor` → `UiContext` draw calls)
- `ViewTree::frame()` entry point
- Hit testing against computed bounds
- Callback dispatch (clicks, hovers)
- `FocusManager` (Tab navigation)
- `input_consumed_by_declarative` flag on `UiContext`
- Integration test: declarative button that responds to a simulated click

**Deliverable**: A simple declarative view (text + button) rendering alongside existing immediate mode UI, with working click handling.

### Phase 4: Interactive Widgets (4-5 days)
**Goal**: Full widget set for editor use.

- Slider (with arena state for drag tracking)
- Toggle (checkbox)
- TextField (with cursor, selection, clipboard)
- ColorPicker
- ScrollView (with scroll state in arena)
- Panel
- Helper functions (section_header, delete_button, etc.)

**Deliverable**: Inspector panel rewritten in declarative style.

### Phase 5: Animation (3-4 days)
**Goal**: Smooth visual transitions.

- `Tween`, `Easing`, `Animation` types
- `KeyframeAnimation` for multi-stop animations
- `Transition` system (fade, slide, scale)
- `show_if` helper with transition support
- Spring interpolation
- `on_complete` callbacks
- Frame-time-based animation ticking

**Deliverable**: Animated panel open/close, smooth health bar drain.

### Phase 6: Polish + Migration (5-7 days)
**Goal**: Migrate remaining panels, add gamepad support.

- GamepadNavigator (directional focus)
- IME integration stubs
- Serialization (ViewDescriptorData)
- Migrate toolbar, hierarchy, asset browser, preferences
- Remove dead immediate mode code for migrated panels
- Keep immediate mode API for game HUDs and debug overlays

**Deliverable**: Fully declarative editor UI with immediate mode fallback.

---

### Estimated Timeline

| Phase | Effort | Dependencies |
|-------|--------|-------------|
| 1. Foundation | 3-4 days | None |
| 2. Layout | 3-4 days | Phase 1 |
| 3. Drawing + Input | 4-5 days | Phase 2 |
| 4. Interactive Widgets | 4-5 days | Phase 3 |
| 5. Animation | 3-4 days | Phase 3 |
| 6. Polish + Migration | 5-7 days | Phase 4+5 |

**Total estimate**: 22-29 days of focused work.

### Risk Mitigation

- **Phase 1-3 are independently validatable** — no need to commit to the full plan
- **The immediate mode API is never removed** — worst case, partially migrated panels still work
- **Taffy is battle-tested** — used in production by Bevy, Dioxus, and others
- **No GPU pipeline changes** — the DrawList contract is stable
- **Enum-based descriptors** — easy to extend, no generic explosion, predictable compile times
- **Panel-by-panel migration** — each panel migration is independent and reversible

### Validation Milestone

After Phase 3, before continuing to Phase 4, implement a minimal proof-of-concept:
- A single declarative panel (toolbar) rendering alongside existing immediate mode panels
- The toolbar has 3-4 buttons with working click handlers
- Input correctly routes between declarative and immediate mode
- DrawList output is identical to what the immediate mode toolbar produced

This validates the core architecture before investing in the full widget catalog.
