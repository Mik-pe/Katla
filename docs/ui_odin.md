# Retained UI in Odin

`odin/ui` owns declarative reconciliation, state, flex/grid layout, input routing,
focus, text editing, dock structure and ordered draw commands. It imports no scene,
ECS, window, GPU or renderer package. The application owns editor actions, panel
contents, resource paths, clipboard transport and native texture mapping. The font
provider and GPU consumer live in the application renderer.

## Frame and ownership

Initialize one stationary `ui.Context` with `context_init`, a real `Font_Provider`
and optional theme/allocator. A second initialization of a live owner fails.
`frame(context, descriptor, input, logical_size)` reconciles a tree of globally
unique nonzero descriptor keys, lays it out, processes ordered events, and returns
`Draw_List` plus `Frame_Result`. Descriptor strings, children, options and syntax
ranges are copied during reconciliation. Invalid descriptors are rejected before
changing the previous tree or draw list. Missing text state is an error.

`Node_Id` includes owner, key and generation. `State_Id` adds an explicit hook
slot. `state(context, key, slot, initial)` reserves a typed cell for the next frame;
`state_get` returns borrowed strings and `state_set` replaces a matching variant
with owned data. Removal invalidates old IDs; remounting a removed key creates a
new generation. A reordered key retains state, selection, scroll and edit history.

Only active dock contents need to mount. Before each frame, the application calls
`retain(context, inactive_panel_key)` for existing inactive subtrees. Their state,
text selection, scroll and undo survive, while they have no draw or input entries.
`forget` explicitly releases a dormant subtree. Ordinary unreserved removal still
collects nodes. State hooks belonging to a mounted root can also hold panel state.

Drain actions after the frame. `actions_drain_all` preserves event order, including
a drag's final edit before a subsequent click. `actions_drain(context, T)` extracts
one variant when cross-variant order is irrelevant. Returned slices belong to the
caller. `actions_clear` releases remaining actions and captured text snapshots.
`Text_Action` references its state cell; `action_text` reads its latest live value
or its captured value if the subtree was removed. Snapshots remain borrowed until
`actions_clear`, including after draining. Blur and removal submit dirty text.

Draw command arrays, text and syntax ranges remain immutable until the next
successful frame or context destruction. The renderer must consume or freeze them
before then. Every command has its own logical clip. `Texture_Id` and `Font_Id`
are opaque application identities; the UI never inspects native resources.

## Layout and widgets

The layout solver is written directly in Odin. There is no Taffy runtime, Rust
layout helper, external solver callback or silent missing-layout path. Styles
cover the layout used by the previous editor: rows/columns, wrapping rows,
fixed/equal-track grids, overlay stacks, padding/margins, independent gaps,
pixel/percentage lengths, min/max dimensions, grow/shrink, aspect ratio,
cross alignment, justification and anchored absolute placement. `percent(0.5)`
means half the containing extent. Default cross alignment stretches; default
shrink is one, with `no_shrink` for explicit nonshrinking content. Flex items that
reach min/max constraints freeze before remaining space is redistributed. Root
auto dimensions fill the available viewport, while explicit dimensions and aspect
ratios apply. Grid `columns`, `cell_size` and gaps correspond to the previous Rust
grid constructor. An unspecified cell width divides the available grid width.

Descriptors cover text, buttons/icons, menus/context menus, text/code/numeric
inputs, sliders and drag values, checkboxes, combos, tree/selectable rows,
scroll areas, tabs/docks, modals, splitters/sections, images, tooltips, separators,
progress and timeline tracks. Containers compose children; application meanings
are carried by `action` and `payload`. The application controls expanded tree
contents and mounts panel contents into `dock_bounds` results. A tree disclosure
only expands rows with `has_children`; body clicks select independently.

Slider rendering and capture share `slider_track`. A held drag continues outside
the field and reports start/change/finish actions. Numeric input keeps an editing
buffer and publishes only finite, bounded parsed values as one edit gesture;
invalid text remains marked without changing the application value. Selectable
and tree rows with `draggable` emit pointer source intents; crossing four logical
pixels suppresses the normal release click. Asset payloads and drops remain app
policy. Scrollbars share exact thumb geometry with drawing, support both axes,
and preserve capture outside bounds. Scroll descendants receive inherited clips.

## Layers, focus and text

Drawing and hit testing use matching content/overlay/popup/modal/tooltip layers.
Hidden or dormant ancestors exclude descendants. Disabled ancestors block input.
Popup backgrounds cover lower controls and scrollbars. The outside click that
closes a popup is consumed. Modal input traps focus and blocks outside pointer
and keyboard delivery. Popup menu arrows and Tab move within menu focus scopes;
modal Tab wraps within the modal. `Dismiss_Action` asks the app to hide the menu
or modal. `Frame_Result` separates consumed events from current retained pointer
and keyboard capture. Viewport images emit their own pointer actions through dock
containers, including dragging outside the viewport. Window blur ends captures.

Text editing owns UTF-8 byte offsets, selections, clipboard operations, ordered
commit/submit, IME preedit, caret visibility and bounded local undo/redo. Code
editing adds line gutters and line selection, indentation, full-text syntax runs
and explicit document replacement with history reset. Saving a script is an app
action, not a consequence of UI state changing. A submitted action in the same
frame as an IME commit observes the newly committed cell. Preedit is not committed
until a `Text_Commit` arrives. The IME request supplies the actual logical caret
rectangle for the native candidate window.

A production `Font_Provider` must supply measurement, caret, hit testing, visual
navigation and logical grapheme navigation. All use the same shaped layout as
rendering; there are no fabricated production metrics or scalar-navigation
fallbacks. Carets use a deterministic primary visual location for each UTF-8 byte
offset. Deletion uses UAX29 grapheme boundaries. `Text_Run` colors are UTF-8 ranges;
the renderer shapes the complete text once and colors glyphs by their clusters,
including ligatures. `pixel_scale` changes raster resolution without changing
logical geometry. Native font/layout evidence belongs to the font and renderer
validation, independently of the CPU metric fixtures below.

## Validation and migration evidence

Run `odin test odin/ui -vet -strict-style -sanitize:address`. The current fourteen
flow tests cover keyed identity/reorder/removal and rejected-frame recovery,
flex min/max freezing and percentages, wrapping/grid updates, padding/anchored
stack/resize cases, captured slider endpoints, Unicode commit before submit,
clipboard editing, modal and popup focus, inactive code history retention,
indent/undo/syntax ranges, numeric rejection and atomic acceptance, scrollbar
capture and nested clipping, exact tab migration/reopening, snapshot rollback,
stale dock IDs, drag/click distinction, blur submission after removal, ordered finish/click and actual dock drag input, disabled
ancestors and capture retirement. Tests use explicitly named deterministic font
fixtures to isolate CPU layout/input contracts; they do not claim native shaping.
All fourteen pass with ASan and zero outstanding tracked allocations.

The source migration fixtures preserve assertions in the former
`katla_ui/src/declarative/layout.rs` tests for padding, flex width changes, grid
column/cell changes, stack positioning, zero-size and viewport resize, and the
former `dock/serialization.rs` schema. They are source-linked contract tests,
not a newly introduced Rust solver or an executed cross-language pixel oracle.

Dock persistence keeps the Rust tagged JSON `Split`/`Leaf`/`Empty` structure,
`Horizontal`/`Vertical`, ratio, two children, tabs and active index. Optional
`Dock_Tab_Codec` maps stable IDs to existing serialized panel enum names.
`dock_restore` validates a complete candidate before replacing the current tree;
duplicate tabs, malformed structure and invalid IDs cannot partly publish. Ratios
zero and one from legacy snapshots remain supported. UI splitter dragging keeps
an interactive 0.05–0.95 range. `dock_apply` and `dock_open` are the canonical
mutation APIs; applications do not mutate node maps or tab arrays directly.
