# Odin agent and application authoring

`odin/agent` builds on the existing Odin ECS/editor ownership. The remaining
agent/application migration is tracked in [TODO](../TODO.md#odin-port).

## Scene calls, context and admission


`decode_call` validates a borrowed `Tool_Call` and returns an owned `Decoded_Call`.
The operation's strings borrow its parsed JSON tree; destroy the decoded owner
once after execution or submission. `submit_call` returns `(ticket, Call_Error)`
and copies the operation and caller's `Tool_Call.id` into `editor.Agent_Harness`
before destroying the decode tree. A nonzero ticket identifies accepted work;
rejection returns zero. Replies retain that ticket, an owned `call_id` and a
separate monotonic session action `id`. Undo does not recycle action IDs.
The existing editor thread executes operations and owns undo history.
Host threads never receive a World pointer. Supply the mailbox a thread-safe
allocator and join producer/consumer threads before its destruction.

Mailbox capacity defaults to 256 and is configurable through `agent_harness_init`
or the application owner's `agent_capacity` parameter. One slot remains reserved
for each queued, executing or unread response. A full mailbox rejects admission
without assigning a ticket or mutating the scene; taking a response releases its
slot. Capacity bounds the number of pending envelopes, not the size of each
payload or retained undo history. A transport must separately bound message sizes.

`agent_finish` closes admission and allows the owner to drain accepted work.
Later submissions return `Mailbox_Closed` rather than asserting. A paused owner
retains requests until resumed. `agent_cancel(ticket)` removes only queued work,
frees its slot and produces no response or history entry. It returns false once
the owner has taken the request, even while application execution is in progress;
transport cancellation after that point must suppress delivery without pretending
the accepted scene mutation was rolled back. Tickets are never reused within a
harness, and exhaustion explicitly rejects new work. Destroy transferred replies
with `agent_response_destroy`, which frees both the correlation string and result
using captured allocators. Harness destruction owns all remaining queued requests
and unread responses.

The CPU tool names are `spawn_entity`, `destroy_entity`, `duplicate_entity`,
`set_field`, `query_entities`, `list_available_components`, `add_component`,
`remove_component` and `get_component_attributes`. All reject unknown fields.
Entity IDs are decimal strings, including IDs larger than JavaScript's exact
integer range. Overflow, signs, whitespace and numeric JSON IDs are rejected.
Spawns accept `name` and finite three-element `position`, `rotation` and `scale`
arrays; scale defaults to one. These are the existing editor's registered CPU
component fields, not mesh creation. Query limits default to 256 and must be
1–256 when supplied. Duplicate offsets, shapes, model loading, parenting and
application-owned animation/behavior/resource operations remain outside
this subset and are not silently ignored.

`scene_context` captures sorted registered component counts and optional selected
component JSON on the editor thread. A stale generational selection yields no
selected data. The snapshot owns its arrays and JSON; its type names borrow the
registry, which must remain alive. It includes the registered serialized state;
it is not a public/private host filtering boundary. Adapter-specific data policy
must precede transporting this observation.

`Rate_Limiter` owns a mutex and preallocated rolling minute of admitted timestamps.
Pass monotonic nonnegative elapsed time to `rate_admit`; backwards clocks fail
without changing admission history. Only `Allowed` records a call. `Wait` and
`Exceeded` return the remaining duration; a delayed caller must retry the same
atomic admission operation, rather than recording unconditionally after a sleep.
This deliberately avoids the Rust bridge's delayed-caller admission race.
Maximum count is at least one and interval at least zero. Keep a shared limiter
at a stable address and join callers before destruction.

The runnable consumer submits JSON on a real host thread, joins it, verifies the
World remained untouched, ticks the mailbox on its owner thread, checks the
result/selection and undoes the scene mutation while preserving a pre-existing entity:

```sh
odin run odin/examples/agent_scene -out:target/odin-agent-scene -vet -strict-style
odin test odin/agent -all-packages -out:target/odin-agent-tests -vet -strict-style
```

The concurrent mailbox consumer sends and receives 512 correlated calls on a
host thread while the application owner executes them. A three-slot capacity
forces backpressure; a queued spawn is cancelled, later admission is rejected
after closure, every accepted result is consumed, and shared undo preserves a
pre-existing entity. Captured-allocator tracking is empty after teardown:

```sh
odin run odin/examples/agent_mailbox -out:target/odin-agent-mailbox -vet -strict-style
```

LLM HTTP/streaming/configuration, conversational orchestration, asset/resource
tools and remaining typed application requests still need migration. The optional
MCP adapter below opens no network or provider connection.

## Optional MCP transport

`odin/agent/mcp` handles [MCP 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/basic)
through one existing `Agent_Harness`. Its single protocol caller never receives a
World or application pointer; only the mailbox crosses to the scene owner. The
adapter is the exclusive consumer of that mailbox's replies. `server_receive`
returns an owned immediate response or queues a validated tool request;
`server_poll` drains correlated scene results or expires a stopped-owner request.
Both return strings to free with the server's captured allocator. Keep protocol
state stationary and serialize its calls; its mutex-protected mailbox can be
executed independently on the application thread.

Every request includes `params._meta` with
`io.modelcontextprotocol/protocolVersion` and an object-valued
`io.modelcontextprotocol/clientCapabilities`. `server/discover` advertises the
supported version and tools capability; there is no initialization handshake or
connection-derived capability state. An unsupported version returns `-32022`
with supported/requested versions. Results include `resultType: "complete"` and
server identity metadata. `ping`, `tools/list`, `tools/call` and
`notifications/cancelled` are supported. The deterministic tool list contains
only the nine CPU scene tools and application-owned `material` described above.
Missing tool names/protocol metadata yield protocol errors; invalid tool input
and scene failures yield `isError: true` tool results. Successful results expose
lossless decimal `entity_ids` and actual component/material JSON in
`structuredContent`, repeated as text for clients using textual content.

String IDs and signed 64-bit integer IDs remain distinct, including integers
above JavaScript's exact range. Empty string IDs are valid. Null/fractional IDs,
duplicate in-flight IDs and integer overflow are rejected; a rejected duplicate
has no ID to avoid delivering a second reply for the original accepted request.
A JSON frame is capped at one MiB and 64 container levels. Input must be UTF-8,
strict JSON with one complete value, unique nonempty object keys and supported
finite numeric values. Trailing values, embedded NUL and invalid numeric syntax
are rejected before decoding can silently normalize them. The blocking line
reader retains bounded storage and discards oversized input until the next
newline. EOF in a partial frame reports an error without executing it.

Cancellation removes queued work and sends no reply. If the owner has already
taken the operation, the adapter suppresses its eventual response and leaves the
accepted mutation/history intact. The default 15-second deadline applies to
requests without a ready result. A stopped queued owner is cancelled; an already
executing operation retains a tombstone until its result is drained. A deadline
never retries a mutation. Closing admission drains accepted work, while adapter
destruction cancels any remaining queued requests. Join owner/transport threads
before destroying their mailbox.

The optional headless consumer supplies a real `app.Authoring`, editable name
and position components and the material service. Its input thread owns no scene
pointer; a four-frame queue crosses to the application owner. Stdout contains
only newline-delimited protocol messages. EOF closes admission, drains accepted
requests/replies, joins input and releases all tracked allocation. Unrecoverable
input/output failure terminates the process promptly; it does not wait forever
for an input thread whose client left stdin open.

```sh
odin build odin/mcp_stdio -out:target/katla-odin-mcp -vet -strict-style
python3 scripts/validate_odin_mcp.py
odin test odin/agent/mcp -all-packages -out:target/odin-mcp-tests -vet -strict-style
```

The pipe acceptance executes actual spawn/component/material/query/destroy calls,
fragmented UTF-8, integer/string correlation, pipelining, cancellation races,
oversized-frame recovery, immediate EOF after 40 accepted requests, partial/empty
EOF and broken stdout. Native-thread tests also stall an owner during accepted
execution, expire its real monotonic deadline and prove its late reply is
suppressed while the mutation remains undoable. Captured-allocator tests cover
pending, cancelled, unread and transferred ownership.

This headless scene transport does not yet attach to the running windowed editor,
create mesh geometry, publish viewport PNGs or expose the remaining application
services. The existing [private editor attachment](shared-editor-view.md) remains
a separate migration requirement; this consumer never replaces a live editor's
scene or claims viewport/GPU acceptance.

## Material requests and the application owner

`material` accepts `presets`, `inspect` and `set` through the same `submit_call`
entry point. `decode_material` produces a typed `Material_Op`, rejecting unknown
fields per action, malformed IDs, duplicate IDs (including equivalent decimal
spellings), empty patches, out-of-range factors and batches outside 1..256.
Entity IDs remain decimal strings even above JavaScript's exact integer range.
Optional null preset/factors are absent patches. `base_color` needs four finite
channels in 0..1, and `metallic`, `roughness` and `ao` need finite factors in 0..1.
An optional preset establishes values before explicit patches. The six presets
match the Rust material library: plaster, oak, concrete, ceramic, brushed metal
and fabric. Decoded numeric operations retain no borrowed JSON strings; the
entity array has an explicit captured-allocator destruction operation.

The mailbox clones the validated request's tool name and payload. Its owner can
supply `editor.Application_Executor` explicitly to `agent_tick`, `agent_execute`
or `agent_run_sync`. The executor's state stays on the owner thread and is never
stored in the producer mailbox. A missing executor returns `Application_Owned`
for application requests, rather than claiming success.

`odin/app.Authoring` composes World, the registry and the existing agent history.
`authoring_tick` dispatches material calls to the application service and ordinary
scene calls to `editor.scene_execute`. It guards targeted editor-hidden entities
and restricts mutations to editing mode. These mode guards do not implement the
simulation lifecycle; that port remains pending.

`Surface_Material` contains only optional linear tint and numeric PBR factors;
mesh/texture/native handles remain separate. The material tool reports sRGB
values. Preset/color edits convert to linear once; a factors-only patch preserves
the exact previous linear channels and the absence of a tint. Material fields
are hidden from generic `set_field` so that path cannot bypass material validation.
Every target and proposed value is checked, and result JSON is serialized before
any live object changes. The service rejects missing/stale entities, missing
surfaces, protected entities and mutations outside editing mode.

The common editor `Undo_Group` now owns one typed command and its affected
identity array, replacing its previous single-entity snapshot representation.
Scene commands and material batches use the same session stack.
`agent_record_action` consumes/zeros an already applied result and undo owner,
clones its borrowed operation and assigns one monotonic action ID without
reexecuting the mutation. `agent_execute` uses that same recording path; grouped
material gestures can record their exact first-before/last-after command once. Application
commands must validate all targets before mutation, own their state, release it
using the captured allocator, and remap stored targets after restoration creates
fresh entity generations. Material undo/redo preflights all targets and changes
only the surface component, preserving unrelated position/mesh/texture state.
Scene restoration decodes all component snapshots before touching the world.
Successful undo propagates replacement IDs to earlier command targets, so a
spawn → material edit → destroy chain can be undone without creating extra
entities. Remapping references inside arbitrary serialized component payloads is
not implemented by this target-remapping mechanism; future hierarchy/event/prefab
commands must own those references explicitly.

The real consumer joins a host producer before applying a two-object material
batch, then undoes both surfaces while retaining an unrelated position edit:

```sh
odin run odin/examples/material_authoring -out:target/odin-material-authoring -vet -strict-style
odin test odin/app -all-packages -out:target/odin-app-tests -vet -strict-style
```

This is CPU scene authoring acceptance. It does not establish rendered PBR output,
native material uploads, inspector controls, live dragging, play simulation or
complete windowed application integration. Those consumers remain in Rust until
migration and native validation complete.

## Validation

The application-inclusive suite passes 84 tests across app, agent, editor, ECS
and math, with strict vet/style checks, optimized and native AddressSanitizer
execution and allocator leak tracking. Mailbox tests cover eight competing
producers, unique tickets, exact capacity, unread/executing reply credits,
queued cancellation, closure during application execution, terminal ID exhaustion
and all response ownership paths. Tests also cover a real 256-object batch,
invalid transport rejection before queue mutation, missing/stale/protected targets,
edit-mode guards, exact absent/linear tint restoration, atomic failed batch undo,
failed scene snapshot decoding and replacement-generation history. The actual
host-thread material consumer passes sanitizer execution with an empty allocation
tracker at teardown. The concurrent 512-call mailbox consumer also passes native
AddressSanitizer and typechecks for Linux and Windows. The portable validation
script runs the application suite and both consumers. Rust reference all-target
check/Clippy and fmt checks pass; its complete agent suite passes 125 tests,
ECS passes 259 tests with one separately ignored test, and the material-filtered
application suite passes nine CPU tests with two native GPU tests separately
ignored. These remain separate evidence; provider,
graphics and native full application acceptance remain pending.
