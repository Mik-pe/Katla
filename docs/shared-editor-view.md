# Shared editor viewport over MCP

Build `cargo build -p game` (MCP is enabled in the standard editor build). Launch the editor with
`KATLA_MCP_SOCKET=/tmp/katla-editor.sock target/debug/katla --scene assets/scenes/shared-room.katla`.
Use the actual Cargo target directory when it is configured externally. The Unix
socket is private to the current OS user (0600). Existing sockets are never
silently removed. `katla-mcp /tmp/katla-editor.sock` is the stdio MCP command for
attaching an external client to that already running editor. Without the socket
environment variable, the editor itself serves MCP over stdio. MCP logs must go
to stderr, and stdout belongs to the protocol.

`editor_view` accepts an action:

- `observe`: optional candidate `limit` (default 64, maximum 256).
- `set_camera`: world `position` and `target` arrays.
- `select`: a generational `entity_id` string, or null to clear selection.
- `focus`: `entity_id` and optional `select` (default false).
- `undo` / `redo`: undo or redo the last agent scene operation.

Every successful action returns metadata and a PNG of the next committed editor
viewport. Camera changes are immediate and ephemeral: no game camera or scene
file is changed. Manual navigation can immediately take over. Focus animation
also cancels on manual navigation. `observe` also works during Play/Pause; camera, selection, focus and history
changes require edit mode. Use `simulation` for explicit preview transitions. A stopped render loop
fails requests after 15 seconds rather than waiting indefinitely.

The image and object-ID samples are queued from the same committed submission,
before its graph sources can be overwritten. Metadata is sampled at that commit,
before deferred editor actions. Frame, submission, capture time, return frame and
physical image dimensions describe provenance. Selection is optional; observing
never selects a candidate. Center and pointer picks come from the committed GPU
object-ID image and its own instance-to-generational-entity mapping. They identify
only the foremost mapped pickable draw at the sampled pixel. Raw encoded samples
are returned as `center_pick_sample` and `pointer_pick_sample`; nonzero unmapped
values can belong to editor overlays such as gizmos and are not scene entity IDs.
Pointer coordinates refer
to the image's native rows; the camera's projected rectangles use top-left image
coordinates. The cursor is not a gaze sensor.

Candidates reuse the render camera's frustum and the transformed drawable bounds
consumed by scene rendering. Bounds may intersect while their origin is outside
the image. Screen rectangles conservatively project near-clipped box edges;
frustum intersection proves neither occlusion visibility nor room membership.
IDs are sorted before truncation, and total/truncation counts are explicit.
Entity references in MCP scene commands are decimal strings preserving the full
generational u64 value, including when a JSON client uses floating-point numbers.
Renderables without bounds are counted separately. Skinned deformation and
non-drawable semantic entities need supplementary scene queries. A room cannot
be inferred completely from one frustum: inspect the image and hierarchy, query
nearby objects beyond the image, and ask for clarification when necessary.

The prepared room has two differently colored doors, walls, a window and a low
cabinet. Place the camera inside at `[0,1.6,1]`, looking at `[0,1.2,-6]`, with no
selection. Discuss the left/right door, focus a candidate, widen it using
`set_field`, then use `undo`. For furnishing, use `search_assets` to discover resource-relative model paths,
`material` to inspect and edit per-object PBR factors, and the scene spawn/model
tools. The [agent authoring guide](agent-authoring.md) includes room recipes and
copyable requests. This exercises engine affordances; it does not certify
an external model's semantic room understanding.

`python3 scripts/validate_shared_view.py /tmp/katla-editor.sock` reloads this
prepared test scene in the running editor, verifies actual captured geometry for
a selected door's relative 50% widening and undo, then places and undoes a cube.
It also checks selection-free observation, center GPU picking, stale ID rejection,
candidate truncation, off-camera spatial queries and project model discovery.
It never saves the scene; PNGs and frame metadata go to
`/tmp/katla-shared-view-proof` by default. Run it only in the prepared test editor,
as reloading replaces its current scene and undo history.

Agent scene changes and undo invalidate the selected inspector's cached fields
before the next UI build, so stale slider values cannot overwrite those changes.
The application-owned default material stays protected across scene loads;
editor overlay resources do not replace that protection.

## Existing external conversation

The Scene assistant panel forwards text, a committed viewport PNG and its metadata
to one explicitly selected, already loaded Codex conversation. It does not own an
internal LLM session. Socket and conversation ID live under Preferences → Connection
and persist with editor preferences. `KATLA_CODEX_SOCKET` and `KATLA_CODEX_THREAD`
can provide initial values. The normal panel shows connection status and the host's
conversation name when available. The first connection is explicit; saved or environment-supplied choices are
reconnected on launch. A failed connection
never falls back to another agent.

This uses the supported [Codex App Server](https://developers.openai.com/codex/app-server/)
transport: `codex app-server proxy --sock <absolute-private-control-socket>`.
The host must already be running and expose a private Unix control socket; Katla
checks the chosen thread is in its loaded list, rejoins it, and uses `turn/start`
when idle or `turn/steer` with the exact active turn ID when busy. It never invokes
`thread/start`, forks, starts an app-server daemon, reads authentication files, or
supplies model/approval/sandbox overrides. Streaming messages retain turn and item
IDs. EOF or a dropped proxy disables sending; reconnecting replaces only Katla's
proxy, not the external conversation or active turn. Unsent queued questions
are cancelled when that connection is replaced.

Katla does not answer server requests for approvals, tools or authentication.
The panel directs attention to the main host. Multi-client approval ownership
must be verified against the actual host before claiming live acceptance.
MCP scene-tool exposure remains host configuration: `katla-mcp` attaches that
conversation's tools to the already running Katla editor. The question transport
and MCP tool transport have distinct responsibilities and socket paths.

The installed Codex 0.144.4 CLI exposes the supported proxy. Its default daemon
control socket was absent in this session, so attaching to the desktop's actual
conversation has **not** been verified. An existing desktop login alone is not
evidence that the desktop exposes this interface. Explicit supported host
configuration is currently required; no desktop socket discovery is claimed.

## Teen bedroom blockout

`assets/scenes/teen-room-blockout.katla` is an editable prepared scene containing
15 geometric additions: a bed and bedding, bedside table, desk at the window,
monitor, chair, wardrobe, bookcase and rug. These are honest unit-cube proxies,
not fabricated furniture models. The current model inventory includes boxes,
fox/tiger/avocado samples, a helmet, lantern and plane; it has no bed, desk, chair
or wardrobe assets. `teen-room-plan.json` describes the corresponding
scene-tool inputs. The base room, both doors, window, existing cabinet and
outside-room test object are preserved exactly. New upright objects leave a
2 m central passage and each door's 1.6 m wide, 2 m deep approach clear. These
are placement clearances, not door swing or navigation-mesh acceptance.

Run `python3 scripts/furnish_shared_room.py --socket /tmp/katla-editor.sock`
only in the disposable prepared editor. It captures the view, queries objects
beyond the frustum and available GLTF/GLB project models, places the blockout through
actual `spawn_entity` calls, checks rendered bounds, undoes every addition and
checks the original entity IDs, then places it again. PNGs, frame/submission
metadata and a receipt go to `/tmp/katla-teen-room-proof`. It does not save or
replace a project scene file. A failed first placement unwinds the additions.
This deterministic journey validates tools, not a live model's design quality.

For an isolated child instead of an existing socket:

```sh
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 python3 scripts/furnish_shared_room.py \
  --output /tmp/katla-teen-room-proof \
  --stdio-command <cargo-target>/debug/katla --headless --frames 10000 \
  --scene assets/scenes/shared-room.katla --screenshot /tmp/katla-teen-room.png
```

`--stdio-command` consumes the remaining arguments. The bounded frame budget
allows several asynchronous tool/readback requests; EOF and timeouts fail the
client. A successful headless GPU run uses the real editor/render path and native
image readbacks. Its PNGs and committed frame/submission metadata establish the
rendered output's provenance. The furnishing journey checks all 15 placements
against rendered bounds, undoes them back to the original entity IDs and camera,
then places them again. Run with backend API validation enabled and inspect the
images alongside the receipt; CPU fixture and pipe protocol tests establish
different parts of the contract.

This validates native rendering and deterministic scene-tool behavior. It does
not certify real OS input, a live model's room understanding or design quality,
or attachment to the actual desktop Codex conversation. Those require separate
end-to-end validation, including approval ownership in the real host.
