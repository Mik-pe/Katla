# Odin agent and application authoring

`odin/agent` implements typed scene calls, the shared mailbox, MCP transports and
external-host connection over Odin ECS/editor ownership. The current editor is
`odin/katla`; [agent authoring](agent-authoring.md) and [shared viewport](shared-editor-view.md)
describe its operating contract. [TODO](../TODO.md) records unresolved work.

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
the accepted scene mutation was rolled back. `agent_abandon(ticket)` releases only
that caller: queued work is removed, completed replies are destroyed and an
executing reply is suppressed when its action completes. Executing credits
remain reserved until completion. Other producers stay admitted.
`agent_take_result_for(ticket)` transfers only the caller's reply, preserving
unrelated replies and their order. Tickets are never reused within a
harness, and exhaustion explicitly rejects new work. Destroy transferred replies
with `agent_response_destroy`, which frees both the correlation string and result
using captured allocators. Harness destruction owns all remaining queued requests
and unread responses.

The generic scene tool names include `spawn_entity`, `destroy_entity`, `duplicate_entity`,
`set_field`, `query_entities`, `list_available_components`, `add_component`,
`remove_component` and `get_component_attributes`. All reject unknown fields.
Entity IDs are decimal strings, including IDs larger than JavaScript's exact
integer range. Overflow, signs, whitespace and numeric JSON IDs are rejected.
Spawns accept `name` and finite three-element `position`, `rotation` and `scale`
arrays; scale defaults to one. Application services create canonical SceneName,
SceneTransform and persistent SceneKey components. Public spawn rotation is XYZ
Euler degrees, converted once to radians and composed as Qz*Qy*Qx. Query limits
default to 64; omitted/null uses that default and unsigned values clamp to 1–256.
Duplicate offsets, shapes and parenting are not silently ignored.

The application owner explicitly installs `authoring_services_init` and confined
asset roots. Typed `material`, `material_asset`, `animation`, `simulation`, `behavior`, `trigger`,
`prefab`, `load_scene`, `save_scene`, `search_assets`, `list_resources` and
`read_resource`, `create_resource` and `write_resource` calls validate
before admission, then execute on that same owner. The canonical
`agent.TOOLS_JSON` owns the sorted 28 schemas; `tools_select` copies only named
schemas supported by a concrete consumer and rejects unknown/duplicate names.
Mesh/model instantiation loads the actual confined Resource, Project or explicit
File source, prepares CPU/native resources before publication and records the
canonical owned undo command. Prefab
capture writes a complete subtree with local document keys; removal retains the
owned subtree for undo. Load prepares a full replacement before publication,
then clears history referencing the old world. Save publishes an atomic confined
`.katla` file before committing new document keys or scene origin. Full unsigned
64-bit document keys and registered entity references round-trip without floating
point conversion; overflow identifiers fail before admission.

Undo and redo use the shared owned command history. Read-only and failed calls
remain recorded but are skipped by undo; they preserve an existing redo branch.
Only a new reversible command abandons that branch. `agent_can_undo` and
`agent_can_redo` are the common UI authority, and restored entity generations
remap both command stacks and embedded registered references.

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

The explicit provider library implements HTTP/TLS/SSE, configuration,
conversational orchestration and cancellable jobs in [the provider package](../odin/agent/llm/README.md).
Its local subprocess acceptance covers actual sockets, verified TLS, streaming
progress, correlated owner-thread tool results and cancellation. The canonical
Co-Creator uses the selected existing external conversation through
[`odin/agent/host`](../odin/agent/host/README.md); the provider library remains an
explicit embedding/validation API, not the native editor default. Local fixtures
do not establish paid-provider behavior or actual desktop conversation attachment.

## MCP scene transport

`odin/agent/mcp` handles [MCP 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/basic)
through one existing `Agent_Harness`. Its single protocol caller never receives a
World or application pointer; only the mailbox crosses to the scene owner. The
adapter consumes only its admitted tickets through `agent_take_result_for`, so
LLM and MCP consumers can share the owner's mailbox. `server_receive`
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
the canonical 28 scene/application tools described above.
Missing tool names/protocol metadata yield protocol errors; invalid tool input
and scene failures yield `isError: true` tool results with
`structuredContent: {success:false,message:...}` as well as matching text. Successful results expose
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
destruction abandons only this connection's tickets, including unread or
executing replies, without closing the shared mailbox. The standalone stdio
consumer explicitly closes its own harness at EOF. Join owner/transport threads
before destroying their mailbox.

The optional headless consumer supplies a real `app.Authoring`, editable name
and transform components, material/gameplay services and confined asset roots.
Optional arguments are `[project-root resource-root]`; defaults are `.` and
`resources`. Source-only component codecs retain prepared CPU values in undo
and simulation snapshots, so restoring a prepared scene does not reload a changed
file from disk. Its input thread owns no scene
pointer; a four-frame queue crosses to the application owner. Stdout contains
only newline-delimited protocol messages. EOF closes admission, drains accepted
requests/replies, joins input and releases all tracked allocation. Unrecoverable
input/output failure terminates the process promptly; it does not wait forever
for an input thread whose client left stdin open.

```sh
odin run tools/build -- --output target/katla-agent --tests
odin run tools/build -- validate mcp
odin test odin/agent/mcp -all-packages -out:target/odin-mcp-tests -vet -strict-style
```

The pipe acceptance executes actual spawn/component/material/query/destroy calls,
fragmented UTF-8, integer/string correlation, pipelining, cancellation races,
oversized-frame recovery, immediate EOF after 40 accepted requests, partial/empty
EOF and broken stdout. Native-thread tests also stall an owner during accepted
execution, expire its real monotonic deadline and prove its late reply is
suppressed while the mutation remains undoable. Captured-allocator tests cover
pending, cancelled, unread and transferred ownership.

This CPU stdio owner is independent of the running native editor and has no GPU
viewport. The current [private editor attachment](shared-editor-view.md) uses
`katla-mcp-proxy` and the native editor's `View_Service`, sharing its scene,
selection, camera and completed image/ID captures. Protocol fixtures and native
viewport journeys validate those separate consumers.

## Material requests and the application owner

`material` accepts `presets`, `inspect`, `set`, `set_sampling` and
`set_texture` through the same `submit_call` entry point. `decode_material`
produces an owned typed `Material_Op`; it rejects unknown fields per action,
malformed or equivalent duplicate IDs, empty/null-only patches, nonfinite or
out-of-range numbers and batches outside 1–256. Entity IDs remain exact decimal
strings above JavaScript's integer range. Raw unsigned UV, anisotropy and image
index tokens are checked before floating parsing; overflow and fractional
aliases cannot wrap into a valid small index. Image indices are bounded to u32,
matching the prepared application's index representation.

`base_color` contains sRGB RGB and linear alpha in 0–1. Metallic, roughness,
AO and occlusion strength are linear factors in 0–1. Emission is nonnegative
linear RGB, including HDR values above one. Normal scale is signed and finite;
zero flattens a normal map. Coverage has explicit opaque/mask/blend mode,
nonnegative cutoff and double-sided choice. Alpha alone preserves the render
mode. A preset establishes all surface factors before supplied patches; omitted
or null properties preserve existing values. The six named presets are plaster,
oak, concrete, ceramic, brushed metal and fabric. They start opaque and
single-sided with normal scale/occlusion strength one and cutoff 0.5; they do not
install texture images or directional brushing.

`Material_Set_Sampling` patches one of the five roles: albedo, normal,
metallic_roughness, occlusion or emission. UV sets are 0/1, translation and scale
have two finite channels, and rotation uses radians. Negative or zero scale is
legal. Six minification policies, two magnification policies, independent U/V
wrap modes and requested anisotropy 1–16 are explicit. Presence bits distinguish
omission from zero. Target UV availability and the complete sampler policy are
validated by the application before mutation; partial patches cannot validate
against invented defaults in the producer.

`Material_Set_Texture` chooses inherit, neutral, a file, or a selected glTF
image. The asset reference explicitly chooses Resource, Scene or File; path
resolution and File capability checks belong to the retained application roots.
The decoded source owns its path, and `decoded_material_destroy` releases it and
the entity array through their captured allocator even if the current allocator
changes. Producers carry no World or GPU resource pointer.

`material_asset` has six strict typed actions: describe, read, validate, write,
capture and apply. Its `.katmat` document workflow and material/image ownership
are described in [material contracts](material_contracts.md). Query rows and
frozen viewport candidates report `material_editable` using the same actual
prepared primitive policy as material authoring; group/model controllers are
not advertised as independently editable surfaces. `editor_view observe` with
`limit: 0` retains the committed PNG, camera, picking/provenance and total/count/
truncated metadata while returning empty candidate arrays.

The mailbox clones the validated request's tool name and payload. Its owner can
supply `editor.Application_Executor` explicitly to `agent_tick`, `agent_execute`
or `agent_run_sync`. The executor's state stays on the owner thread and is never
stored in the producer mailbox. A missing executor returns `Application_Owned`
for application requests, rather than claiming success.

`odin/app.Authoring` composes World, the registry and the existing agent history.
`authoring_tick` dispatches material calls to the application service and ordinary
scene calls to `editor.scene_execute`. It guards targeted editor-hidden entities
and restricts mutations to editing mode. Preview transitions use the application
simulation owner and restore authored identity through registered snapshots.

`Surface_Material` contains optional linear tint, PBR factors and explicit
surface/sampling override presence. Owned portable image assignments and prepared
image generations remain separate from geometry and GPU handles. The material tool reports sRGB
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
the owned material state for that operation, preserving unrelated position and
mesh state. Factors, sampling, image assignment and complete material application
retain their respective prepared before/after owners.
Scene history restoration prepares owned component clones and validates all
registered references/native participants before publishing. Prepared history does
not reload a changed source file or replay transient particle queues.
Successful undo propagates replacement IDs to earlier command targets, so a
spawn → material edit → destroy chain can be undone without creating extra
entities. Registered component codecs explicitly own and remap hierarchy, trigger and joint
references during snapshot, prefab and simulation restoration. Unknown component
payloads remain preserved data and do not acquire implicit entity-reference semantics.

The real consumer joins a host producer before applying a two-object material
batch, then undoes both surfaces while retaining an unrelated position edit:

```sh
odin run odin/examples/material_authoring -out:target/odin-material-authoring -vet -strict-style
odin test odin/app -all-packages -out:target/odin-app-tests -vet -strict-style
```

This example is CPU scene authoring acceptance. Rendered PBR output, native
material uploads, inspector gestures and simulation are implemented by the Odin
application consumers below; their native acceptance is separate evidence from
this mailbox example.

## Native application consumer

`odin/katla` owns the platform loop and mounts the retained `odin/app/editor`
shell with real document, preferences, assets, inspector, code and audio services.
The Co-Creator captures a committed viewport and forwards a question to an
explicitly selected already loaded external conversation. Connection and approval
ownership are described in [the shared viewport guide](shared-editor-view.md).
`app.Assistant` remains an explicitly selected provider-library consumer used by
its examples and local HTTP/TLS/SSE tests; it is not the native editor's default
conversation owner. Both consumers submit scene actions through the same mailbox.

`app/render.Native_Consumer` prepares actual World `Scene_Mesh` and `Scene_Model`
components through the canonical WGSL compiler/adapter and Metal/Vulkan APIs.
Its native stage participant prepares complete candidate mesh/model resources
before scene or asset publication. Failed GPU allocation preserves the old
World, generations, identity, cache and pixels, and releases partial candidates.
Save/load, captured prefab insertion/removal, undo and redo exercise the same
participant; a successful load publishes prepared resources with fresh entity IDs.

Model batches select the active glTF scene and retain exact authored matrices,
primitive material factors, vertex colors and five transformed UV sets. Native
uploads decode the actual bounded PNG/JPEG bytes, distinguish sRGB color textures
from linear data textures, generate mip chains and retain authored sampler state.
Per-frame acquired-slot geometry streams sampled morph/skin vertices; transforms
and material edits update object buffers without recreating native pipelines.
Both metallic/roughness and specular/glossiness workflows render through native
opaque, masked and blended variants. Winding variants follow the determinant of
the complete node/entity matrix, and tangent handedness follows its reflection.
Scene meshes/models, particles, lighting, transparent phases and editor overlays
compose in application-owned linear/HDR attachments. The display transform
encodes once into the native output; scene composition and UI blending do not
blend authored sRGB values into an already encoded scene. Directional and point
lights, cascaded directional shadows, environment lighting, grid, outline and
postprocessing are ordinary render graph consumers. Independent viewport camera,
color, depth and picking owners support four editor views. Native image and
same-submission ID readbacks validate the affected Metal and Vulkan paths; see
[graphics contracts](graphics_core.md), [GPU particles](particles_odin.md) and
[shared viewport provenance](shared-editor-view.md).

Actual readbacks verify PBR edits, shared gesture/agent undo and redo, empty-scene
clear/resume, staged failure/retry and native model animation on both backends.
Source models include Box, DamagedHelmet, Fox and Tiger. Native window acceptance
verifies acquire/present, retina resize and retained pre-resize readback. Run
[the native application validator](../tools/build/validation.odin):

```sh
odin run tools/build -- validate render --native-metal --native-vulkan \
  --native-surface --sanitize \
  --vulkan-library /path/to/libvulkan.dylib --vulkan-icd /path/to/icd.json \
  --particles --build-manifest target/katla-odin/darwin-arm64/asan/build.json
```

Use matching sanitizer artifacts for a sanitized run. The validator snapshots
actual repository resources into its owned output project and builds source-pinned
parser/image dependencies. Combined physics/script/particle acceptance uses the
direct Luau ABI 2 and Box3D ABI 8 dependencies; no retired Rust scene-runtime
library is involved. GPU launches retain ASan address checks and Odin allocation
tracking while excluding external driver process-exit leak reporting. CPU tests
retain leak checks. On Darwin, narrow external CFPreferences initialization
suppressions do not suppress application/native-parser allocation leaks.

The persistent native editor installs all current tools from
[`odin/agent/tools.json`](../odin/agent/tools.json), including hierarchy, generic
component edits, primitives/models, materials, resource creation/replacement,
scene files, prefabs, animation, triggers, behavior, simulation and committed
viewport actions. External tools and native controls use the same application
executor, generational reference remapping and shared undo/redo history. Dirty
document confirmation, code drafts and native preparation precede accepted edits.
Use [the source-pinned build/launcher](odin_build.md) and [agent authoring
journeys](agent-authoring.md) to operate the current editor.

Headless native readbacks, local host fixtures and CPU protocol tests prove their
specified paths. Complete desktop interaction still requires the actual OS-input
journeys and their receipts; a compiled shell or screenshot alone does not prove
all editor flows. Existing desktop conversation attachment and paid-provider
behavior remain separate host acceptance boundaries.

## Validation

Strict Odin checks, AddressSanitizer and captured allocator tracking cover the
mailbox, owned snapshots, native provider transport and application consumers.
Tests exercise executing/unread credits, ticket-selective replies, independent
connection shutdown, native physics mesh contacts and whole-batch failure,
confined asset reads and exact prepared revision undo. The MCP acceptance script
uses real process pipes, instantiates a disk mesh, creates/inspects a trigger and
verifies recovery, EOF draining and output failure. Native Metal/Vulkan render
acceptance is tracked separately in [graphics](gfx_odin.md); it is not inferred
from CPU or local provider fixtures. The canonical build supplies pinned native
parser, image, font, physics and script dependencies. Historical Rust comparison
results describe migration evidence; the current scene/editor operating path and
its acceptance use the Odin consumers.

## Descriptive resource generation

`generate_resource` preserves the working local assistant resource-creation feature
through the canonical MCP/LLM registry. It creates a new project-relative file
from `resource_type` (`particle_system` or `scene`) and a bounded UTF-8 description.
Keyword priority follows the original descriptive templates. It uses confined,
exclusive atomic publication, creates missing parents, rejects existing files and
requires editing mode. File creation does not add a scene undo operation.

Generated particles are actual `behavior set_particles` descriptors, including
the chosen lifetime range, velocity, colors and size progression. Generated scenes
use the current version3 RON codec and load through `load_scene`; night, sunset,
interior and daytime presets produce a configured directional light. This repairs
the older generator's unusable version1 scene/settings output. Generation preserves
the current world; loading the resulting scene is an explicit separate action.
The real stdio fixture attaches all nine particle keyword templates and loads all
four lighting templates before reading their actual authored state.
