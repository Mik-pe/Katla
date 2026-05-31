# State-of-the-Art Declarative/Immediate-Mode UI Systems for Game Engines

## Research Report — May 2026

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [System-by-System Analysis](#2-system-by-system-analysis)
   - 2.1 [egui](#21-egui)
   - 2.2 [Dear ImGui Docking](#22-dear-imgui-docking)
   - 2.3 [Bevy UI / bevy_egui](#23-bevy-ui--bevy_egui)
   - 2.4 [Slint](#24-slint)
   - 2.5 [Xilem](#25-xilem)
   - 2.6 [Vello + Parley (Linebender)](#26-vello--parley-linebender)
   - 2.7 [Compose/SwiftUI-Inspired Patterns](#27-composeswiftui-inspired-patterns)
   - 2.8 [Unity UI Toolkit](#28-unity-ui-toolkit)
   - 2.9 [Unreal Common UI](#29-unreal-common-ui)
   - 2.10 [Taffy](#210-taffy)
3. [Cross-Cutting Analysis by Topic](#3-cross-cutting-analysis-by-topic)
   - 3.1 [Architecture Spectrum](#31-architecture-spectrum)
   - 3.2 [State Management](#32-state-management)
   - 3.3 [Layout and Composition](#33-layout-and-composition)
   - 3.4 [Input and Event Routing](#34-input-and-event-routing)
   - 3.5 [Docking/Panel Systems](#35-dockingpanel-systems)
   - 3.6 [The "Frame Function" Pattern](#36-the-frame-function-pattern)
4. [Recommendations for Katla](#4-recommendations-for-katla)

---

## 1. Executive Summary

The modern UI landscape for game engines has converged on a **hybrid declarative** approach that provides immediate-mode ergonomics (describe what you want each frame) over retained-mode infrastructure (efficient persistent layout trees, caching, incremental updates). The key insight across all systems studied is:

> **"Hidden inside of every UI framework is some kind of incrementalization framework."** — Ron Minsky (Jane Street)
>
> **"Modern UI frameworks represent a hybrid approach that blends concepts from both retained and immediate mode UIs. They provide the mental model and simplicity of immediate mode while maintaining the performance benefits of retained mode."** — AUI Framework

### Key Findings

1. **Pure immediate-mode is insufficient** for complex editor UIs — it cannot efficiently handle large lists, complex layout, or retained state like docking configurations
2. **Pure retained-mode is too verbose** — it requires manual state synchronization that becomes unmaintainable at scale
3. **The winning pattern is "declarative frame function over retained tree"**: you write a function that describes the UI each frame, and the framework diffs it against a retained backend tree
4. **Taffy (Flexbox/Grid)** is the universal layout choice — used by Bevy, Dioxus, egui_taffy, and already in Katla
5. **Identity-based diffing** (stable IDs keyed by position or explicit key) is the core mechanism for reconciling frame functions with retained trees
6. **For docking specifically**, the industry has converged on a separate retained tree (ImGui's `DockNode` hierarchy) managed outside the immediate-mode API surface

---

## 2. System-by-System Analysis

### 2.1 egui

**Architecture:** Pure immediate-mode with retained state storage

egui rebuilds the entire UI from scratch every frame but maintains persistent state via a three-tier system:

| Tier | Struct | Lifetime | Purpose |
|------|--------|----------|---------|
| Memory | `Memory` | Application | Global settings, widget state, window positions |
| PassState | `PassState` | Single frame | Shapes, accessibility tree, allocated space |
| Local | Various | Function scope | Transient UI state within closures |

**State Management:**
- Uses an `Id` system (64-bit hash) to key persistent state in `IdTypeMap`
- `IdTypeMap` is a heterogeneous type-safe storage that maps `(Id, TypeId)` → value
- Widget state (collapsed, scroll position, etc.) persists across frames via `Memory.data`
- Double-buffered `PassState`: `this_pass` collects current frame, `prev_pass` used for hit-testing against last frame's geometry
- Context access is closure-based (`ctx.memory(|m| ...)`) to prevent deadlocks from the internal `RwLock`

**Layout:**
- Simple sequential layout within each `Ui` region (no Flexbox/Grid natively)
- Each `Ui` has an allocated `Rect` and a cursor that advances as widgets are added
- `egui_taffy` exists as a community crate adding Flexbox layout via Taffy
- Layout is computed during the frame function, not as a separate pass

**Input/Event Routing:**
- Raw input stored globally in `InputState`
- Widgets check for interactions via `Response` objects returned from widget calls
- No explicit event routing tree — each widget independently checks if it was interacted with
- Focus tracked per-viewport in `Memory.focus`

**Docking:**
- `egui_dock` (community crate, not official) provides docking via a retained tree of `DockNode`s
- The docking tree is maintained separately from the immediate-mode UI
- Tree structure: `DockNode::Leaf(tab strip) | DockNode::Split { fraction, left, right } | DockNode::Empty`
- Tab state and split ratios are persistent state managed outside egui's frame function

**Key Design Patterns:**
- **ID-based state recovery**: widgets generate IDs from their position in the call hierarchy, recovering state from the previous frame
- **Closure-based context access**: prevents borrow checker issues with the global state
- **Response pattern**: widgets return a `Response` indicating interaction state, avoiding callback hell
- **Area/Panel system**: retained-mode "areas" (windows, panels) that persist across frames, containing immediate-mode content

**Weaknesses:**
- No native Flexbox/Grid layout
- Layout is coupled to the frame function execution order
- Complex retained state (docking, complex trees) must be managed manually outside the framework
- Performance degrades with very large UIs due to full rebuild every frame

---

### 2.2 Dear ImGui Docking

**Architecture:** Pure immediate-mode with a retained docking tree overlay

The docking branch adds a retained-mode spatial tree on top of ImGui's immediate-mode core. Omar (the author) considers the docking code "not great" and wants to rewrite it a third time, but it's been the de facto standard for years and is used by most large teams.

**State Management:**
- Same as base ImGui: `PushID`/`PopID` stack for identity, `ImGuiStorage` for persistent widget state
- Docking state serialized to `.ini` files via `ImGuiSettingsHandler`
- `DockNode` tree is a fully retained data structure managed by the framework
- Dock layout is persistent and survives application restarts

**Layout:**
- Base ImGui: same sequential layout as egui (heavily inspired egui)
- Docking adds spatial splitting: `ImGuiDockNode` can be split horizontally or vertically
- Each dock node has a tab bar and a content area
- Layout is managed by the docking tree, not the immediate-mode API

**Docking Architecture (The Key Innovation):**
```
ImGuiDockSpace (root)
├── ImGuiDockNode (split horizontal, 70/30)
│   ├── ImGuiDockNode (leaf: tabs [Scene, Game])
│   └── ImGuiDockNode (split vertical, 50/50)
│       ├── ImGuiDockNode (leaf: tabs [Inspector, Hierarchy])
│       └── ImGuiDockNode (leaf: tabs [Console, Assets])
```

- `DockSpace` is created by `ImGui::DockSpaceOverViewport()` — an invisible fullscreen window
- Windows opt into docking via `ImGuiWindowFlags_DockNodeHost`
- `DockBuilder` API (in `imgui_internal.h`) for programmatic layout setup
- User interaction (drag tabs to dock) is handled entirely by the framework
- Tab reordering, splitting, and undocking are built-in behaviors

**Key Insight for Katla:**
ImGui's approach proves that **docking MUST be a retained tree** managed separately from the content frame function. The content inside each dock tab can be immediate-mode, but the dock structure itself is a persistent spatial tree. The `DockBuilder` API is acknowledged as "not great" — Katla should design a better programmatic API for initial layout.

---

### 2.3 Bevy UI / bevy_egui

**Architecture:** Retained-mode, ECS-based (bevy_ui) with immediate-mode bridge (bevy_egui)

**bevy_ui (Retained, ECS-native):**
- UI nodes are ECS entities with components (`Node`, `Style`, `BackgroundColor`, etc.)
- Parent-child relationships via Bevy's hierarchy system
- Layout computed by Taffy (Flexbox + CSS Grid)
- Fully retained: UI persists in the ECS world until explicitly despawned
- Data-driven: UI can be loaded from BSN (Bevy Scene Notation) asset files
- Behavior via Bevy's observer system (ECS events)

**bevy_egui (Immediate mode bridge):**
- Wraps egui as a Bevy plugin
- Bridges egui's state management with Bevy's ECS
- Good for debug overlays and tools, but architecturally separate from the game UI

**bevy_immediate (Emerging, 2025):**
- egui-inspired immediate mode API over retained ECS entities
- Each frame: iterate entities, make minimal changes needed
- Does NOT despawn/respawn — updates existing entities in-place
- Proves that immediate-mode API can work as a thin layer over retained ECS storage

**Bevy's Vision for UI (from "A vision for Bevy UI" document):**
- **Retained mode** as the foundation
- **ECS-powered**: same data structures as the rest of the game
- **Data-driven**: UI from assets on disk, supporting non-programmer tooling
- **Dogfooding**: Bevy's own editor will be built with bevy_ui
- **Incrementalization is essential**: "we agree with Raph Levien's position"
- **No VDOM**: direct retained entity tree, not a virtual DOM diffing approach
- **Observers for behavior**: ECS observers on UI entities for event handling
- **Layout via Taffy**: Flexbox and Grid following CSS spec

**Key Design Patterns:**
- `bsn!` macro for declaring UI hierarchies in code (mirrors asset file syntax)
- Scenes as widgets: a widget is a collection of entities spawned as a scene subtree
- `Patch` trait for inheriting/customizing existing widget scenes
- State machine via `bevy_state`: spawn/despawn UI on state transitions

**Weaknesses:**
- Still maturing — widget library is not yet complete
- No docking system yet
- Reactivity story is still evolving
- Complex UI patterns require significant boilerplate

---

### 2.4 Slint

**Architecture:** Fully retained, compiled declarative UI

Slint takes a fundamentally different approach from the other systems studied. It's a **compiled declarative UI framework** with its own markup language (`.slint` files).

**Architecture:**
- `.slint` files compile to native code (Rust, C++, Python, JS)
- Five-layer architecture: Application → API → Compilation → Core Runtime → Platform/Rendering
- Core runtime uses an `ItemTree` (retained item tree) with `ItemVTable` for polymorphism
- Each item has: `init`, `layout_info`, `input_event`, `render` methods

**State Management:**
- **Reactive property system**: properties track dependencies and propagate changes
- `PropertyTracker` manages fine-grained reactivity
- Three property visibility levels: `in` (input), `out` (output), `in-out` (bidirectional)
- Model system for dynamic lists with `Repeater` components
- Compiled bindings ensure changes propagate efficiently without runtime diffing

**Layout:**
- Built-in layout algorithms: `VerticalLayout`, `HorizontalLayout`, `GridLayout`
- Also supports Taffy for Flexbox (via `sharedparley` integration)
- Layout info computed via `layout_info()` on the `ItemVTable`
- Properties like `min-width`, `preferred-height`, `padding` drive layout

**Composition:**
- Components inherit from other components
- `for` loops for dynamic lists
- `if` conditionals for dynamic visibility
- Properties and callbacks define component contracts

**Key Design Patterns:**
- **Compiled, not interpreted**: `.slint` files generate native code at build time
- **Separation of concerns**: UI structure in `.slint` files, business logic in Rust/C++
- **Cross-language**: same UI works from Rust, C++, Python, JS
- **Property-driven reactivity**: no diffing needed because changes are tracked at the property level

**Weaknesses for Game Engines:**
- Not designed for game HUDs — targets desktop/embedded apps
- Compiled approach limits dynamic UI generation
- No docking system
- Rendering pipeline is separate from game rendering
- Property system overhead may not suit 60fps+ game rendering

---

### 2.5 Xilem

**Architecture:** Declarative view tree with diffing against retained widget tree (SwiftUI-inspired)

Xilem is Raph Levien's architecture for UI in Rust, designed to solve the fundamental problems with existing approaches. It's the most theoretically sophisticated system studied.

**Core Architecture — Three Synchronized Trees:**

```
┌─────────────┐     diff      ┌─────────────┐
│  View Tree  │ ◄────────────► │  View Tree  │
│  (frame N)  │               │ (frame N+1) │
└──────┬──────┘               └──────┬──────┘
       │ build/rebuild                │
       ▼                              │
┌─────────────┐                       │
│  View State │ ◄── persists across frames
│    Tree     │                       │
└──────┬──────┘                       │
       │                              │
       ▼                              ▼
┌─────────────────────────────────────────┐
│           Widget Tree (retained)         │
└─────────────────────────────────────────┘
```

1. **View Tree** — ephemeral, rebuilt each frame by the app logic function. Lightweight value objects describing the UI. Statically typed with `impl Trait`.
2. **View State Tree** — persists across cycles. Contains per-view-node state (similar to React hooks). Also statically typed.
3. **Widget Tree** — retained, traditional retained-mode UI tree. Type-erased children for practicality.

**The `View` Trait:**
```rust
trait View<State, Action = ()> {
    type State;    // per-node persistent state (like React hooks)
    type Element;  // corresponding widget type
    
    fn build(&self, cx: &mut Cx) -> (Self::State, Self::Element);
    fn rebuild(&self, prev: &Self, state: &mut Self::State, cx: &mut Cx) -> ChangeFlags;
    fn event(&self, state: &mut Self::State, cx: &mut Cx, event: &mut Event) -> Option<Action>;
}
```

**State Management:**
- **No shared mutable state** — this is the core design constraint
- App state passed as `&mut` through the event dispatch path
- `Adapt` nodes transform parent state into child state (evolution of Druid's lens concept)
- `Memoize` nodes skip rebuild when data hasn't changed (using `Arc::ptr_eq` for cheap comparison)
- Immutable data structures + pointer equality for efficient change detection

**Identity and Event Dispatch:**
- **Id paths** instead of flat IDs: `[1, 3]` means "child 1 of root → child 3 of that"
- Events dispatched by traversing the view tree along the id path
- Each traversal step provides `&mut` access to the app state
- `Adapt` nodes transform the state type during traversal

**Change Propagation:**
- Incremental computation engine at the core
- `Memoize` nodes compare data, skip subtree rebuilds if unchanged
- `Arc<T>` + `ptr_eq` for O(1) change detection on complex state
- Environment values with fine-grained subscriber notification

**Key Innovation — The Frame Function:**
```rust
fn app(count: &mut u32) -> impl View<u32> {
    v_stack((
        format!("Count: {}", count),
        button("Increment", |count| *count += 1),
    ))
}
```
This function is called on each "cycle" and produces a view tree. The view tree is diffed against the previous version to produce minimal updates to the retained widget tree. No VDOM overhead — the view tree IS the declaration.

**Weaknesses:**
- Still experimental / not production-ready
- Complex type signatures (monomorphization explosion)
- Static typing can be restrictive — `AnyView` escape hatch needed
- No docking or complex layout system yet
- Single-threaded reconciliation (parallel reconciliation planned but not implemented)

---

### 2.6 Vello + Parley (Linebender)

**Architecture:** Rendering and text infrastructure, not a UI framework

Vello and Parley are the rendering/text layers that power Xilem. They're infrastructure, not a complete UI system.

**Vello (Vector Renderer):**
- GPU-accelerated 2D vector rendering
- Three modes: full GPU, CPU/GPU hybrid, CPU-only
- Scene-based API: build a scene description, render it
- Uses compute shaders for efficient GPU rendering
- Supports gradients, text, images, paths

**Parley (Text Layout):**
- Rich text layout library
- Migrated to HarfRust (Rust port of HarfBuzz) for text shaping
- Handles line breaking, BiDi, complex script shaping
- Used by both Xilem and Slint

**Relevance to Katla:**
- Vello's scene-based rendering model is a good pattern: accumulate draw commands, then render in one pass
- Parley is the state-of-the-art for Rust text layout — worth using or learning from
- Linebender's approach of separating rendering from UI logic is sound architectural practice

---

### 2.7 Compose/SwiftUI-Inspired Patterns

**Architecture:** Declarative view tree with retained backend, reactive state

The patterns from Jetpack Compose and SwiftUI have become the de facto standard for modern UI. They represent the "declarative revolution" that all other systems are converging toward.

**Core Pattern — The Composable/View Function:**
```
@Composable  // or View { ... } in SwiftUI
fun MyApp(state: AppState) {
    Column {
        Text("Count: ${state.count}")
        Button(onClick = { state.count++ }) {
            Text("Increment")
        }
    }
}
```

This function:
1. Is called every time state changes (or every frame)
2. Produces a lightweight description of the UI
3. The framework diffs it against the previous description
4. Applies minimal updates to the retained rendering tree

**Key Concepts Applicable to Game UI:**

| Concept | Compose | SwiftUI | Application to Game UI |
|---------|---------|---------|----------------------|
| State hoisting | `remember` + `mutableStateOf` | `@State`, `@Binding` | Game state lives in ECS; UI reads it |
| Composition | `@Composable` functions | `View` structs | Widget functions that take props and return view trees |
| Side effects | `LaunchedEffect` | `.onAppear` | Trigger game actions from UI events |
| List diffing | `key { ... }` | `ForEach(id: \.id)` | Efficient list updates in inventories, etc. |
| Environment | `CompositionLocalProvider` | `@Environment` | Theme/style propagation without passing through every layer |

**State Management Pattern:**
- **Unidirectional data flow**: State → UI → Events → State updates → re-render
- **No two-way binding**: UI describes what it wants, doesn't mutate state directly
- **Recomposition is cheap**: framework skips unchanged subtrees

**Hybrid Mode Analysis (from AUI Framework research):**
Modern frameworks create "an immediate-mode-like developer experience while maintaining retained-mode-like performance benefits." They use:
- Sophisticated state management that tracks dependencies
- Trigger recompositions only when needed
- Diffing algorithms for efficient tree updates
- The visual representation is reevaluated only if state changes

---

### 2.8 Unity UI Toolkit

**Architecture:** Retained-mode visual tree with Flexbox layout and data binding

Unity UI Toolkit is Unity's next-generation UI system, replacing the imperative uGUI. It demonstrates the industry-wide shift from imperative to declarative UI in game engines.

**Core Architecture:**
- **Visual Tree**: lightweight retained-mode tree of `VisualElement` nodes
- **UXML**: declarative markup for UI structure (like HTML)
- **USS**: styling (like CSS)
- **Flexbox layout**: via a custom implementation following CSS spec
- **Data binding**: UI elements connected to data sources, changes propagate automatically

**State Management:**
- **Data binding**: native data binding between UI and data sources
- **Observable data**: changes in data automatically update UI
- **Manipulators**: event handlers attached to visual elements
- **Data-driven**: UI can be loaded from UXML assets

**Key Insight for Game Engines:**
> "UI Toolkit assumes that UI is not something you 'manually update,' but something that reflects state."

This is the fundamental shift: from "update the button text" to "the button text is bound to this data, update when data changes."

**Performance:**
- More predictable than uGUI (no hidden Canvas rebuild cascades)
- Retained tree avoids full-rebuild costs
- Cost scales more predictably with complexity
- Better for mobile/VR where predictability matters more than raw speed

**Weaknesses:**
- Animation tooling still behind uGUI
- Rendering flexibility (custom shaders, complex masking) not as mature
- Not open source — architecture cannot be studied directly

---

### 2.9 Unreal Common UI

**Architecture:** Retained-mode UMG/Slate with Common UI extensions

Unreal Engine's UI stack is built on Slate (low-level retained C++ framework) → UMG (Blueprint-accessible wrapper) → Common UI (modern extensions for multiplatform).

**Slate (Foundation):**
- Fully retained-mode widget tree
- Declarative C++ syntax via macros: `SNew(SButton).Text(FText::FromString("Click"))`
- `SWidget` base class with virtual methods for layout, input, painting
- `OnPaint`, `ComputeDesiredSize`, `ArrangeChildren` methods
- Attribute system for reactive property binding

**UMG (Designer Layer):**
- Blueprint-friendly wrapper over Slate widgets
- Visual designer for layout
- UMG widgets are `UObject`-based, own their Slate widget (`TakeWidget()`)
- Data binding via property binding system

**Common UI (Modern Extensions):**
- Solves multi-platform input (gamepad, keyboard, mouse)
- **CommonUIFocusManager**: manages focus chains for gamepad navigation
- **CommonBoundActionButton**: buttons that respond to input action mappings
- **CommonActivatableWidget**: widgets with activate/deactivate lifecycle
- Designed for the pattern: game HUDs AND complex menu systems

**Input Routing:**
- Focus-based: focused widget receives input first
- Event bubbling: unhandled events bubble to parent
- Common UI adds gamepad-aware focus navigation with visual focus indicators
- Input routing is explicit, not implicit like ImGui

**Key Design Patterns:**
- **Layered architecture**: Slate (low-level) → UMG (designer) → Common UI (game-specific)
- **Focus-driven input**: essential for gamepad support
- **Activatable widgets**: lifecycle management for showing/hiding panels
- **Style inheritance**: themed styling via `UCommonUIStyleSettings`

---

### 2.10 Taffy

**Architecture:** Retained layout tree library (Flexbox + CSS Grid)

Taffy is the de facto standard layout library for Rust UI frameworks. It's used by Bevy, Dioxus, and others, and is already used by Katla.

**Core API:**
```rust
let mut tree = TaffyTree::new();
let root = tree.new_leaf(taffy::style::Style { ..Default::default() })?;
let child = tree.new_leaf(taffy::style::Style { ..Default::default() })?;
tree.add_child(root, child)?;
tree.compute_layout(root, taffy::Size::MAX_CONTENT)?;
let layout = tree.layout(root)?;
```

**Architecture:**
- `TaffyTree` — arena-allocated node tree with `NodeId` handles
- Nodes have `Style` (input) and `Layout` (output)
- Supports: Flexbox, CSS Grid, Block layout
- Algorithms follow CSS specification closely

**Best Practices for Retained Layout Trees:**
1. **Cache layout results**: Only recompute when styles or available space change
2. **Mark dirty nodes**: Track which nodes need relayout, skip unchanged subtrees
3. **Two-phase layout**: measure (intrinsic sizes) → arrange (final positions)
4. **Separate style from layout**: Style is the input, Layout is the output
5. **Hidden nodes**: Use `display: none` to exclude subtrees from layout computation

**Relevance to Katla:**
Since Katla already uses Taffy, the key question is how to integrate it with the declarative UI system. The pattern should be:
1. Declarative frame function produces a view tree
2. View tree is reconciled with a retained node tree
3. Retained node tree maps 1:1 to Taffy nodes
4. Style changes are propagated to Taffy, which computes layout
5. Layout results drive rendering

---

## 3. Cross-Cutting Analysis by Topic

### 3.1 Architecture Spectrum

```
Pure Immediate      Hybrid Declarative          Pure Retained
    ◄──────────────────────────────────────────────────────►

ImGui ─── egui ─── Xilem ─── Compose/SwiftUI ─── Slint ─── Qt
                   Bevy*                             Unity UITK
                                                      Unreal Slate

* Bevy is retained but bevy_immediate adds immediate API on top
```

**The convergence point is clear**: the industry is converging on the "declarative frame function over retained tree" pattern. This is what Xilem, Compose, SwiftUI, and the Bevy vision all implement. The question is not *whether* to use this pattern, but *how* to implement it efficiently in Rust.

### 3.2 State Management

Three approaches, from most to least common in Rust UI:

| Approach | Examples | Rust Affinity | Complexity |
|----------|----------|---------------|------------|
| **Central state + event dispatch** | Elm/Iced, Bevy observers | ★★★★★ | Low |
| **Mutable state through view tree** | Xilem, SwiftUI | ★★★★ | Medium |
| **Shared mutable state** | React/Dioxus, egui | ★★ | High (RefCell/Arc) |

**For Katla (Rust, ECS-based):** The Bevy/Xilem hybrid approach is best:
- Game state lives in ECS components
- UI reads game state via ECS queries
- Events from UI are dispatched through the ECS observer system
- Widget-internal state (scroll position, hover state) managed by the UI framework

### 3.3 Layout and Composition

All studied systems agree on this stack:
1. **Flexbox** for most layout (via Taffy)
2. **CSS Grid** for complex grids (via Taffy)
3. **Absolute positioning** for overlays, popups
4. **Custom layout** for special cases (text flow, graph editors)

**Composition pattern:** All systems use tree composition:
- Container widgets contain child widgets
- Layout algorithms operate on the tree structure
- Children report intrinsic sizes, parents arrange them

### 3.4 Input and Event Routing

Two dominant patterns:

| Pattern | Used By | Pros | Cons |
|---------|---------|------|------|
| **Hit-test + response** | ImGui, egui | Simple, no dispatch tree | Can't handle gamepad focus |
| **Focus-chain + bubble** | Unreal, Unity, Bevy | Gamepad-friendly, accessible | More complex implementation |

**For Katla:** Focus-chain routing is essential for:
- Gamepad navigation (game HUDs)
- Keyboard shortcuts (editor)
- Accessibility (screen readers)
- Tab ordering in forms

The pattern: maintain a focus tree, route events to the focused widget first, bubble unhandled events to parent.

### 3.5 Docking/Panel Systems

**Universal pattern across all systems that implement docking:**

```
┌──────────────────────────────────────────────────────┐
│                    Dock Space (root)                   │
│  ┌──────────────────────┐  ┌────────────────────────┐ │
│  │   Dock Node (split)   │  │   Dock Node (leaf)     │ │
│  │  ┌───────┐ ┌───────┐ │  │  ┌──────────────────┐ │ │
│  │  │ Tab A │ │ Tab B │ │  │  │ Tab C (active)   │ │ │
│  │  └───────┘ └───────┘ │  │  └──────────────────┘ │ │
│  └──────────────────────┘  └────────────────────────┘ │
└──────────────────────────────────────────────────────┘
```

**Key findings:**
- Docking is ALWAYS a separate retained tree from the main UI tree
- ImGui: `DockNode` tree managed by `DockContext`
- egui_dock: `DockNode` enum tree with `Split`, `Leaf`, `Empty` variants
- Unity: `DockArea` + `DockPane` system
- Unreal: `SDockingTabStack` + `FTabManager`

**The dock tree contains:**
- Split nodes (with fraction, direction, two children)
- Leaf nodes (with tab strip, active tab index)
- Each tab references a content provider (widget/factory)

**The content inside dock tabs can be:**
- Immediate-mode frame functions (ImGui, egui)
- Retained widget trees (Unity, Unreal)
- Declarative view trees (Xilem-style)

### 3.6 The "Frame Function" Pattern

This is the central pattern for declarative UI. How do systems handle rebuilding the tree each frame?

**Approach 1: Full Rebuild, No Diffing (egui, ImGui)**
```
Every frame:
  1. Clear all widget state
  2. Run frame function, which rebuilds everything
  3. Render from scratch
Cost: O(n) every frame, where n = total widget count
```

**Approach 2: Virtual DOM Diffing (React, Dioxus)**
```
Every state change:
  1. Build new virtual DOM tree
  2. Diff old VDOM vs new VDOM
  3. Apply minimal patches to real DOM/widget tree
Cost: O(n) build + O(n) diff, but only on state change
```

**Approach 3: View Tree Diffing (Xilem, SwiftUI)**
```
Every state change:
  1. Build new view tree (lightweight value objects)
  2. Diff old view tree vs new view tree (typed, efficient)
  3. Apply changes to retained widget tree
  4. Drop old view tree
Cost: O(n) build, O(changed) diff — typed comparison is cheaper than VDOM
```

**Approach 4: ECS Observers (Bevy)**
```
On state change:
  1. ECS change detection fires
  2. Systems query changed components
  3. Update only affected UI entities
Cost: O(changed) — only touched entities are processed
```

**For Katla, the recommended approach is #3 (View Tree Diffing)** with these specifics:
- The "frame function" runs every frame (or on demand)
- Produces a lightweight declarative view tree
- Diffed against previous frame's view tree using position-based identity
- Changes applied to retained widget/Taffy tree
- Memoize nodes skip subtrees when their data hasn't changed

---

## 4. Recommendations for Katla

Based on this research, here are specific recommendations for Katla's UI system:

### 4.1 Architecture: Declarative Frame Function + Retained Backend

Follow the Xilem/SwiftUI pattern:
- **Frame function**: Rust function that returns a view tree, parameterized on app state
- **View tree**: lightweight, ephemeral, value-typed description of the UI
- **Retained widget tree**: persistent Taffy-backed tree that handles layout, input, rendering
- **Reconciliation**: diff view trees, apply minimal updates to widget tree

```rust
// Target API shape (conceptual)
fn editor_ui(state: &mut EditorState) -> impl View<EditorState> {
    dock_space("editor_dock", |dock_state| {
        panel("hierarchy", |s| hierarchy_panel(&mut s.scene)),
        panel("inspector", |s| inspector_panel(&mut s.selection)),
        panel("viewport", |s| viewport_panel(&mut s.viewport)),
    })
}
```

### 4.2 State Management: ECS + Adapt Nodes

- Game state lives in ECS components (no change)
- UI reads state via ECS queries
- Use Xilem-style `Adapt` nodes to transform ECS state into widget-local state
- Widget-internal state (scroll, hover, focus) managed by the UI framework
- No `Rc<RefCell<>>` — follow Xilem's no-shared-mutable-state principle

### 4.3 Layout: Taffy-First

- Keep Taffy as the layout engine (already in use)
- Each retained widget maps to a Taffy node
- Style changes propagated from view tree diff to Taffy nodes
- Support Flexbox (primary) and absolute positioning (overlays)
- Cache layout, only recompute on dirty

### 4.4 Docking: Separate Retained Tree

- Implement docking as a separate retained tree (proven pattern)
- `DockNode` enum: `Split { fraction, direction, children }` | `Leaf { tabs, active }`
- Dock tree persists across frames, managed by the dock system
- Content inside tabs uses the same declarative frame function
- Tab state (active tab, split ratios) persisted and serialized

### 4.5 Input: Focus-Chain Routing

- Implement focus-chain based routing (not hit-test like egui)
- Essential for gamepad support and accessibility
- Events route to focused widget, bubble to parent
- Separate focus scopes for different panels (editor panes, game HUD)

### 4.6 Incrementalization: Memoize + Dirty Tracking

- `Memoize` nodes that skip subtree rebuild when data unchanged
- `Arc<T>` + `ptr_eq` for O(1) change detection
- Taffy dirty tracking: only recompute layout for changed subtrees
- Rendering: only repaint damaged regions

### 4.7 Dual-Mode Support: HUD and Editor

The system should handle both use cases:

**Game HUDs (hot path):**
- Minimal overhead
- State driven by ECS game state
- May not run every frame (only when game state changes)
- Simpler widgets, fewer interactions

**Editor UI (complex path):**
- Full docking system
- Complex panels with inspectors, trees, graphs
- Runs every frame during editing
- Rich interaction (drag & drop, multi-selection, context menus)

Both modes use the same core architecture. The difference is in the widget library and update frequency.

### 4.8 Implementation Priority

1. **Core reconciliation loop**: frame function → view tree → diff → widget tree
2. **Taffy integration**: retained widget tree backed by Taffy layout
3. **Basic widgets**: containers (vstack, hstack, zstack), text, buttons, scroll
4. **Focus system**: focus-chain routing for keyboard and gamepad
5. **Docking system**: retained dock tree with tab management
6. **Advanced widgets**: tree views, inspectors, property editors
7. **Memoize/incremental**: skip unchanged subtrees for performance

---

## Sources

- Raph Levien, "Xilem: an architecture for UI in Rust" (2022)
- Omar Cornut, Dear ImGui Docking Wiki (2026)
- Alice Cecile, "A vision for Bevy UI" (2024)
- egui Memory and State Management (DeepWiki, 2026)
- Slint Overview (DeepWiki, 2026)
- Darko Tomic, "I Researched UI Toolkit So You Don't Have To" (2026)
- AUI Framework, "Retained and immediate UI" documentation
- Linebender blog posts (2025-2026)
- Unreal Engine Common UI documentation (Epic Games)
- Taffy documentation and source (DioxusLabs)
