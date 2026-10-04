# Declarative editor UI

The current implementation is `odin/ui`. Its authoritative API and ownership
contract are documented in [Retained UI in Odin](ui_odin.md). The application
composition lives in `odin/app/editor`; `odin/katla` supplies the platform loop and
native consumers. The [editor visual design](editor_ui_design.md) remains the
visual specification.

## Compose, reconcile and apply

One stationary `ui.Context` owns the retained tree and typed state cells. The
application builds `Descriptor` values with globally unique nonzero keys and
explicit action/payload identities. `ui.frame` validates and reconciles that tree,
solves flex/grid layout, processes ordered input and returns draw commands plus a
frame result. Invalid descriptors leave the previous accepted tree intact.
Descriptor strings, children and syntax ranges are copied during reconciliation.

Stable keys retain focus, text history, scroll and selection across reorder.
Removed keys invalidate generational node/state IDs. The application explicitly
retains inactive panel roots before a frame, preserving dormant panel state
without giving it draw or input entries. Closing a panel and switching its active
tab therefore have different lifecycle semantics.

After the frame, drain actions in wire order with `actions_drain_all`. A gesture's
final edit precedes a subsequent click. The application applies scene, document,
preferences and dock actions, then builds the next descriptors from accepted
state. UI state is not a second scene undo stack. Numeric controls validate finite
bounded input; inspector previews use one application `Scene_Gesture`, finish as
one shared history command, and cancel through the same native-safe restoration.
Text/code controls own local text undo; saving and script reload are explicit
application transactions.

## Layer boundaries

`odin/ui` imports no ECS, scene, window or graphics package. It owns layout, input
capture, focus, text editing, docking and ordered draw commands. The layout solver
is written in Odin. There is no external layout runtime or parallel immediate-mode
widget path.

The application owns panel content, selected entity IDs, asset paths, document
confirmation, script compilation, clipboard transport and meanings of actions.
The native renderer supplies a real `Font_Provider`, shaped text and texture
mapping. UI texture/font IDs are opaque application identities. Draw commands and
borrowed text remain immutable until the next successful frame or destruction;
consumers must finish or freeze them within that lifetime.

Drawing and hit testing use matching content, overlay, popup, modal and tooltip
layers. Dormant/hidden ancestors exclude descendants, disabled ancestors block
input, modals trap focus, and the outside click dismissing a popup is consumed.
Pointer capture survives movement outside a control and ends on blur. The frame
result tells the application which input was consumed before viewport navigation
or gizmo handling.

## Docking, documents and persistence

`Dock_Tree` owns main and floating panel roots. Typed dock actions handle open,
close, split, move, undock, raise and resize. `dock_bounds` supplies accepted panel
regions. Snapshot/restore validates a complete candidate, including duplicate
tabs across roots and floating rectangles, before replacement. The application
maps stable panel identities to persisted names and stores snapshots with its
[preferences service](../odin/app/preferences/store.odin).

The shell mounts hierarchy, viewport, inspector, assets, Co-Creator, preferences,
particles, console, mixer, timeline and code panels through that same retained UI.
Scene New/Open/Save/SaveAs/Quit use the document backend's dirty baseline and
Save/Discard/Cancel gate. Code tabs have their own draft/save/leave gate. File
validation, script reload, history and GPU publication stay in the application;
UI buttons do not bypass those contracts. See [scene persistence](scene_format.md)
and [agent authoring](agent-authoring.md).

## Validation

Run `odin test odin/ui -vet -strict-style -sanitize:address` for reconciliation,
layout, input, focus, text and dock ownership tests. Deterministic font fixtures
isolate CPU behavior; they do not prove native shaping or rendering. Use the
[canonical source build](odin_build.md) for the real editor and pinned font/GPU
consumers. Native UI acceptance must inspect the affected renderer path with
backend validation and actual readbacks; headless synthetic-input journeys and
real desktop input establish different evidence.
