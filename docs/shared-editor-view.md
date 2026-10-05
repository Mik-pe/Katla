# Shared editor viewport over MCP

The running Odin editor owns the scene, viewport cameras, selection and shared
undo history. MCP transports submit owned commands into its mailbox; they never
borrow a World or GPU frame. `odin/app/editor/View_Service` admits view actions
and replies after their accepted native color/object-ID capture completes.

## Launch and connect

Use the [canonical build and launcher](odin_build.md):

```sh
odin run tools/build -- --output target/katla-shared-view
katla_socket_dir=$(mktemp -d "${TMPDIR:-/tmp}/katla-editor.XXXXXX")
chmod 700 "$katla_socket_dir"
odin run tools/build -- run --build-dir target/katla-shared-view --no-build -- \
  --scene assets/scenes/shared-room.katla --gpu-validation \
  --mcp-socket "$katla_socket_dir/editor.sock"
```

The Unix endpoint parent is private to its owner; the socket is 0600. Existing
endpoints are rejected without unlinking them. An empty `--mcp-socket` leaves
external socket admission disabled. The editor does not read `KATLA_MCP_SOCKET`
to configure its listener and does not become a stdio server when the flag is
omitted.

Configure an external MCP client with the built stdio proxy:

```sh
target/katla-shared-view/bin/katla-mcp-proxy "$katla_socket_dir/editor.sock"
```

The proxy also accepts `KATLA_MCP_SOCKET` when no endpoint argument is given.
Stdout belongs to JSONL MCP and diagnostics belong to stderr. Disconnecting a
client releases its requests, not the shared scene or other clients. Endpoint
cleanup removes only the socket inode created by this listener. Unix socket
transport currently reports unsupported on Windows; this is distinct from native
Windows editor, asset filesystem and CPU stdio support.

`katla-mcp-stdio [project-root resource-root]` is a separate CPU Authoring owner.
It can validate scene services and files, but does not attach to the running
editor or provide its native viewport. To use a room script's `--stdio-command`
with the existing viewport, supply `katla-mcp-proxy ENDPOINT`, not the editor
executable or CPU owner.

The protocol uses `server/discover`, `tools/list` and `tools/call`, with version
`2026-07-28` and client capabilities in each request's `_meta`. The canonical
schemas are [`odin/agent/tools.json`](../odin/agent/tools.json). Ordinary successful
tool replies expose `{entity_ids, data}` as `structuredContent`; view replies
instead expose the committed metadata directly plus separate PNG image content.
The [Odin client](../tools/wire/wire.odin) implements that contract.

## View actions and provenance

`editor_view` accepts these actions:

- `observe`: optional/null limit defaults to 64; unsigned values clamp to 1–256.
- `set_camera`: world `position` and `target` arrays.
- `select`: decimal `entity_id`, or null/omission to clear selection.
- `focus`: entity ID and optional `select`, default false.
- `undo` / `redo`: the shared authoring history.

Observe works during Play/Pause; the other actions require Editing. Camera changes
apply immediately to the active editor viewport. They do not modify an authored
`ScenePerspective` camera or scene file. Focus frames the entity's drawable
subtree, using its actual transformed bounds. Manual navigation can immediately
take over. Scene failures return a tool error; owner requests have a 15-second
deadline. A timed-out reply is not proof that an already accepted mutation was
rolled back, so do not retry mutations blindly.

PNG color and object-ID samples belong to the same completed GPU submission.
Camera, hierarchy and selection metadata are frozen for that accepted capture.
`frame_id`, `capture_serial`, `submission` and provenance generations are exact
decimal strings. Physical `width`/`height` and `image_size` match the PNG IHDR.
`gpu_provenance` identifies the submission, color/ID resource generations and
sampled pixels. There is no invented return-frame value or metadata sampled from
a later live world.

`center_pick` and optional `pointer_pick` use that captured ID image's mapping to
generational scene IDs. They identify the foremost mapped pickable draw at the
sampled pixel. Raw `center_pick_sample`/`pointer_pick_sample` explain encoded values:
zero is background, while an unmapped nonzero value can be editor overlay
geometry. Entity ID zero remains valid because encoded object IDs are a separate
mapping. Pointer coordinates use native image rows; projected rectangles use
normalized top-left image coordinates. A cursor position does not establish gaze
or user intent.

`frustum_candidates`/`candidates` are numerically sorted by exact entity ID before
truncation. They derive from transformed drawable bounds and the captured camera;
conservative rectangles clip bounds at the near plane. Their visibility label is
`frustum_candidate_occlusion_unknown`. Frustum intersection does not establish
occlusion visibility, room membership or semantic understanding. Total/truncation
counts are explicit. Use hierarchy and spatial queries for objects outside the
image and for entities without drawable bounds.

## Prepared room journeys

`assets/scenes/shared-room.katla` contains two doors, walls, a window, a low cabinet
and an object behind the initial camera. Start inside at `[0,1.6,1]`, looking at
`[0,1.2,-6]`, without selection. `tools/author shared-view` checks selection-free
observation, truncation, off-camera spatial queries, resource discovery, focus and
GPU center picking. It widens the left door through `SceneTransform.local`, checks
captured bounds and undoes the edit; it also places and undoes a primitive. It
never saves the scene, but its initial load replaces the disposable editor's world
and history.

The [authoring guide](agent-authoring.md) covers room construction and the prepared
teen-room furnishing recipe. The furnishing script adds 15 geometric proxies,
verifies their bounds, undoes to the original entity IDs/camera, then places them
again. PNGs and JSON receipts document actual tool and render behavior. These
journeys do not certify navigation, furniture-model quality or a live model's
design choices.

## Existing external conversation

The Co-Creator panel sends a question together with a committed viewport PNG and
metadata to one explicitly selected, already loaded Codex conversation.
Preferences → Connection stores the private host endpoint and conversation ID.
Storing or loading preferences does not connect. Connect and reconnect are explicit actions;
a failed connection has no provider or thread fallback.

`odin/agent/host` uses a direct nonblocking private Unix JSONL stream. It initializes
that chosen connection, checks the loaded-thread list, resumes the exact selected
thread and verifies identity. Idle threads receive `turn/start`; active threads
receive `turn/steer` with the exact active turn ID. Katla creates or forks no
thread, starts no daemon, reads no authentication file and supplies no model,
approval or sandbox override. Progress retains turn/item IDs. Disconnect closes
only Katla's stream and unsent questions; accepted external turns remain alive.
Explicit cancellation can interrupt the last accepted turn.

Host requests for approvals, tools or authentication require attention in the main
host; Katla does not answer them. The host conversation's MCP tool configuration
must separately point to `katla-mcp-proxy` for this running editor. Question
transport and scene-tool transport use distinct endpoints and responsibilities.

`odin run tools/build -- validate host --sanitize` exercises local private-socket
fixtures for loaded-thread identity, idle start, active steer, progress, attention,
interrupt, EOF and cancellation. These fixtures do not prove attachment to the
user's actual desktop conversation, paid-provider behavior or multi-client
approval ownership. An existing desktop login does not establish that its host
exposes the required control endpoint; actual host configuration and end-to-end
acceptance remain separate evidence.
