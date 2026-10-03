# Material authoring with an independent agent

The [baseline study](baseline/observations.md) records one independent LLM session
using the real editor MCP interface for material discovery, edits, native viewport
verification, invalid-batch recovery, undo and scene persistence. Its 28 transport
calls, scratch scene and five native frames are preserved alongside the report.
This is evidence from one session, not a general preference survey or a visual
quality benchmark.

The disposable editor binary was built from `ab9c6681` on Linux/Vulkan using the
material-studio scene and a private socket. Shader edits were underway in the
same checkout during the session; the asset watcher could reload them. The
report distinguishes observed native appearance from reproducible shader
acceptance, which requires isolated fixtures and numeric GPU readback.

The study found useful existing behavior: action-discriminated discovery, decimal
entity IDs, presets plus partial overrides, atomic target preflight, undo and
save/load. It also found accepted alpha values without visible transparency,
unavailable emission controls, a brushed-metal name without directional brushing,
and inconsistent error envelopes. These findings guide the material semantics,
authoring and agent-interface work tracked in [TODO](../../TODO.md#material-correctness).

A follow-up should repeat the task against an isolated binary after those changes,
use a fresh agent with schema discovery, and preserve its transcript and native
frames. Numeric GPU checks remain the rendering correctness gate; the agent run
measures discoverability, authoring and recovery in the actual interface.

The [interface follow-up](interface/observations.md) used the same agent with its
prior study context and a private editor built from `90621117` plus the material
interface changes committed with this report. Shader sources stayed fixed. Its
15 calls and four native frames verify capability discovery, before/current edit
receipts, editable target flags, structured failures and zero-candidate images.
This is a contextual follow-up, not a fresh participant or controlled comparison.

The separate native [authoring acceptance](interface/acceptance.json) passed:
material edits changed 616 pixels; undo restored the exact original image; invalid
batches preserved valid members; seven room parts and scene save/load succeeded.
The recorded paths and submission IDs identify this disposable session.

The [surface-factor study](surface/observations.md) used a fresh independent agent
with a private editor built from `a517de0f` plus the surface-factor implementation
committed with this report. Shader sources stayed fixed. Its 45 participant calls
and 15 native images verify HDR linear emission, separate normal and occlusion
controls on an imported textured model, rejection and recovery, undo, and numeric
persistence. The before/after scene reload images are pixel-identical. A lighting
isolation attempt was inconclusive and is preserved as such. Texture provenance
and field-specific errors remain useful interface improvements; the ambiguous
factor-range message observed in this session was corrected with the implementation.

The [coverage follow-up](alpha/observations.md) used the surface-study participant
with its prior context and a private editor built from `6c905a1b` plus the coverage
implementation committed with this report. Shader sources stayed fixed. Its 24
participant calls and seven native images demonstrate transparent foreground
composition, mask rejection, coverage-aware picking, field-specific errors,
recovery, undo, and save/load with fresh entity IDs. The restored native frames
are byte-identical. This is one contextual session; multi-layer compositing,
depth, shadow coverage and face orientation are validated separately through
numeric native static and skinned fixtures.
