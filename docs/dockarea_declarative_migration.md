# DockArea Declarative Migration Plan

## Background

`DockArea` is the last remaining immediate-mode widget in katla_ui. It bypasses the declarative pipeline (`ViewTree::frame()`) with a three-phase split in `layout.rs`:

```
1. DockArea::compute_leaf_bounds()  — layout + resize handles, runs BEFORE frame()
2. ViewTree::frame()                — all declarative panels
3. DockArea::show_chrome()          — tab bars + splitters + drag overlay, runs AFTER frame()
```

Goal: absorb DockArea into the declarative system so `ViewTree::frame()` is the single entry point for all UI.

## Research Findings

| Framework | Approach | Notes |
|-----------|----------|-------|
| imgui docking | Immediate-mode retained tree | Author calls it "unfinished, not great", wants 3rd rewrite |
| egui_dock | Retained `DockState` + immediate `show()` | Hybrid, same problem we have |
| Hello ImGui | Declarative config over immediate-mode rendering | Configuration layer only |
| Dockview / FlexLayout (web) | Pure retained/declarative | Serializable JSON layout |

All dock systems maintain a retained layout tree. The challenge is routing rendering through a single pipeline.

## Current Responsibilities

### DockArea does these things (cataloged from `katla_ui/src/widgets/dock.rs`):

1. **Layout tree** — `DockNode` (Split/Leaf), `DockLayout`, mutation methods
2. **Layout computation** — `compute_leaf_bounds()` walks tree, returns `(panel_id, Rect2D)` per leaf
3. **Resize handles** — `ResizeHandle` widget shown during layout, mutates `DockNode::Split::ratio`
4. **Tab bar rendering** — `DockTabBar` draws tabs, detects clicks/hovers/drags/close
5. **Splitter line rendering** — thin rectangles at split boundaries
6. **Drag-drop** — `DockDragState` tracks cross-frame drag, tear-off threshold, zone detection
7. **Drop zone overlay** — semi-transparent highlight showing where a tab will land
8. **Drag preview** — floating tab at mouse position during drag
9. **Panel registration** — `ui.register_panel()` for focus tracking

### katla_app integration (`katla_app/src/ui/editor_ui/layout.rs`):

- Phase 1: `compute_leaf_bounds()` → layout + resize
- Phase 2: Set env contexts on ViewTree with per-panel bounds
- Phase 3: `view_tree.frame()` → declarative pipeline
- Phase 4: `show_chrome()` → tabs + splitters + drag overlay
- Phase 5: Process dock interactions (close, drop, focus)

## Challenges

### C1. Ordering constraint

Panel descriptors need dock-computed bounds during `build()`, but the dock chrome must render AFTER panel content. The declarative `frame()` is single-pass: build → diff → layout → input → draw.

**Solution:** DockArea's `resolve_positions()` hook computes leaf bounds and stores them as env context. DockArea's `draw()` hook draws children first, then chrome on top.

### C2. Resize handles that mutate layout during layout

`compute_leaf_bounds_recursive()` shows `ResizeHandle` widgets that grab `active_id` and mutate `DockNode::Split::ratio`. This happens during what should be a read-only layout pass.

**Solution:** Treat resize handles like slider drags in the declarative input system. Store an `active_resize: Option<(DockNodeId, SplitDirection)>` in `InteractionState`. During input processing, detect splitter hit-test and update `ratio` via `StateArena::set()`.

### C3. Cross-frame drag state

`DockDragState` is stored on `EditorUI`, not in the declarative tree. It spans across `compute_leaf_bounds` and `show_chrome`.

**Solution:** Store `DockDragState` in `StateArena` via `ctx.state()`. Emit `DockAction` variants through `ActionStream`.

### C4. Chrome Z-ordering

Chrome must render on top of all panel content, including modals/context menus that panels might open.

**Solution:** Draw chrome as a separate layer within the DockArea draw handler, after all children. Use Z-ordering to ensure chrome is always on top.

### C5. Panel label function

`DockArea::show_chrome()` takes a closure `Fn(DockPanelId) -> &'static str` for tab labels. Closures can't be stored in `ViewDescriptor` (it must be `Clone`).

**Solution:** Store panel labels in the descriptor as `Vec<(DockPanelId, String)>`, updated each frame during `build()`.

## Architecture

### New types

```rust
// In katla_ui/src/declarative/descriptor.rs:

pub struct DockAreaDescriptor {
    pub layout_id: StateId,       // DockLayout in StateArena
    pub drag_id: StateId,         // DockDragState in StateArena
    pub panel_labels: Vec<(DockPanelId, String)>,
    pub flex: FlexProps,
}

// Add to ViewDescriptor enum:
// DockArea(Box<DockAreaDescriptor>)
```

### New actions

```rust
// Emitted via ActionStream, processed by katla_app after frame():

pub enum DockAction {
    TabClicked { leaf_tabs: Vec<DockPanelId>, clicked_index: usize },
    TabClosed(DockPanelId),
    PanelDropped { panel: DockPanelId, zone: DockZone, target: DockPanelId },
    PanelDragStarted(DockPanelId),
    RatioChanged { split_path: Vec<usize>, new_ratio: f32 },
}
```

### Revised frame flow

```
EditorUI::build():
  1. Set env contexts (same as today)
  2. view_tree.frame(ui, &EditorOverlayView, screen_size)
     └─ EditorOverlayView.build() produces:
        ZStack([
          DockArea { layout_id, drag_id, panel_labels },
          Overlay(toolbar),
          Overlay(status_bar),
          Overlay(gizmo),
          DraggablePanel(preferences),
          DraggablePanel(particle_inspector),
          DraggablePanel(co_creator),
        ])
     └─ Inside frame():
        a. Build descriptors
        b. Diff tree
        c. Layout via taffy (DockArea gets allocated bounds)
        d. Resolve positions:
           - DockArea resolve hook: walk DockNode tree, compute leaf bounds,
             store in env as DockPanelBoundsCtx
        e. Input:
           - Resize handle hit-test against splitter positions
           - Tab click/hover/drag via hit-test on tab bar regions
           - Drag-drop zone detection
        f. Draw:
           - DockArea draw: draw panel children first, then chrome on top
        g. Drain DockAction from ActionStream
  3. Process dock actions (move panels, split leaves, etc.)
```

### Panel content rendering

Each docked panel is a child of the DockArea descriptor. During `build()`, the DockArea reads its `DockLayout` from `StateArena`, iterates visible leaves, and for each active tab, creates a child descriptor positioned via the dock-computed bounds.

The `EditorOverlayView` continues to inject per-panel env contexts (bounds, entity data, etc.) before the frame. The DockArea reads these env contexts to build child panel descriptors.

Alternatively, the DockArea's `build()` could accept a panel builder closure stored as a `Callback`:

```rust
let panel_builder = ctx.on_click(|actions| { ... }); // Reuse callback mechanism
```

But since `Build` is called per-frame, the simplest approach is to have each panel's `Build` impl read its bounds from the env context, same as today.

## Phased Implementation

### Phase 1: Wrapper (low risk, ~2 days)

Add `ViewDescriptor::DockArea(Box<DockAreaDescriptor>)` that wraps the existing immediate-mode code. The draw handler internally calls the existing `compute_leaf_bounds()` + `show_chrome()`. This proves the descriptor plumbing works without changing behavior.

**Files to modify:**
- `katla_ui/src/declarative/descriptor.rs` — add `DockAreaDescriptor` and `ViewDescriptor::DockArea`
- `katla_ui/src/declarative/diff.rs` — add `DockArea` to discriminant match
- `katla_ui/src/declarative/layout.rs` — add taffy style mapping for `DockArea`
- `katla_ui/src/declarative/draw.rs` — add `draw_dock_area()` that calls existing code
- `katla_ui/src/declarative/input.rs` — add `DockArea` to interactive match
- `katla_ui/src/declarative/constructors.rs` — add `dock_area()` constructor
- `katla_app/src/ui/editor_ui/layout.rs` — restructure to use `DockArea` descriptor
- `katla_app/src/ui/editor_ui/declarative/editor_root.rs` — include DockArea in view tree

**Validation:** All 512+ tests pass. Visual output identical to current behavior.

### Phase 2: State migration (medium risk, ~3 days)

Move `DockLayout` and `DockDragState` into `StateArena`. Move resize handles into the declarative input system. Move tab bar rendering to use the existing `TabBar` descriptor pattern.

**Files to modify:**
- `katla_ui/src/declarative/input.rs` — add resize handle hit-test and ratio mutation
- `katla_ui/src/declarative/draw.rs` — port tab bar rendering from `DockTabBar`
- `katla_ui/src/declarative/tree.rs` — extend `InteractionState` for dock drag
- `katla_ui/src/widgets/dock.rs` — remove `DockTabBar::show()`, `ResizeHandle::show()`

**Validation:** Tab clicking, panel closing, panel resizing, tab dragging all work through declarative input.

### Phase 3: Full integration (higher risk, ~3.5 days)

Fully integrate chrome rendering and drag-drop into the declarative pipeline. Remove `compute_leaf_bounds()` and `show_chrome()` entry points. Refactor `EditorOverlayView` so panels are children of the DockArea descriptor.

**Files to modify:**
- `katla_ui/src/declarative/draw.rs` — draw children then chrome, Z-stack management
- `katla_ui/src/declarative/tree.rs` — position resolution for dock children
- `katla_ui/src/widgets/dock.rs` — remove `DockArea::compute_leaf_bounds()`, `show_chrome()`, make internal
- `katla_app/src/ui/editor_ui/layout.rs` — simplify to single `view_tree.frame()` call
- `katla_app/src/ui/editor_ui/declarative/editor_root.rs` — panels as DockArea children

**Validation:** All editor functionality works. Layout persistence works. Resize, drag-drop, close, focus all correct.

## Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Z-order regression | Chrome overlaps modals/menus | Draw chrome as explicit ZStack layer on top of panel content within DockArea |
| Resize input conflicts | Resize handles compete with panel content for focus | Register splitters as interactive view nodes with priority hit-testing |
| Bounds propagation | Panel bounds need dock-computed values during build | Two-phase resolve: taffy allocates DockArea bounds, then DockArea resolve hook computes leaf positions |
| Floating windows | `DockLayout::floating` needs separate handling | Render as `DraggablePanel` descriptors outside the dock tree |
| Action ordering | Dock mutations must happen after frame | Emit `DockAction` through `ActionStream`, process in layout.rs after `frame()` |

## Estimated Effort

| Phase | Duration | Risk |
|-------|----------|------|
| Phase 1: Wrapper | ~2 days | Low |
| Phase 2: State migration | ~3 days | Medium |
| Phase 3: Full integration | ~3.5 days | Higher |
| Testing & edge cases | ~2 days | Medium |
| **Total** | **~10.5 days** | |
