# TODO

## Task Sizing Convention

Individual tasks should be small enough to complete in a single focused session. For large features (new subsystems, architectural changes, cross-cutting refactors), the TODO item is scoped as **exploration, ideation, and architecture** — research patterns, evaluate alternatives, and produce a concrete implementation plan as smaller TODO items. The output of such a task is a breakdown, not working code.

## Odin port

- [ ] Complete the full [Odin editor contract](docs/odin_editor.md), including canonical application integration and real user-visible acceptance.
- [ ] Port retained declarative UI, flex/grid layout, text/focus/IME, docking and captured pointer gestures to Odin.
- [ ] Integrate hierarchy, generic inspector, documents, preferences, assets, timeline, console, mixer and viewport transforms in the canonical Odin editor.
- [x] Replace runtime shader compiler bindings with an external build process and validated source/options/compiler cache refresh.
- [x] Port audio PCM/codec/metadata loading with real file fixtures before composing voices.
- [x] Port voices/resampling/pooling, the category mixer and scheduling/streaming, reusing the completed `odin/audio/dsp` layer.
- [x] Add native Odin audio output with real playback and callback/lifecycle acceptance.
- [x] Port agent CPU JSON validation, selected scene observations and synchronized rate admission on top of the Odin editor mailbox/undo owner. See `docs/agent_odin.md`.
- [x] Port typed material presets/inspect/set with an application-owned scene consumer, atomic 1..256-object batches and shared exact undo/redo.
- [x] Integrate Odin surface factors with native material uploads, rendered PBR output, inspector controls and shared grouped gesture undo/redo.
- [ ] Prove native pointer capture and gesture completion outside a dragged material slider.
- [x] Port agent application-owned animation, events, prefab/behavior and resource requests with real application consumers.
- [x] Preserve registered component entity references through hierarchy, prefab, trigger, joint and simulation restoration with fresh runtime identities.
- [x] Preserve agent request correlation through a bounded mailbox with queued cancellation, closed admission, reserved response capacity and concurrent application-owner acceptance.
- [x] Add optional Odin MCP 2026-07-28 stdio framing/discovery/tool calls with real scene/material pipes, cancellation, deadlines, EOF and output-failure acceptance.
- [x] Attach the Odin MCP adapter to the live windowed owner/private editor transport and port committed viewport observation/selection/camera plus remaining tool registration.
- [x] Port LLM configuration/HTTP/streaming/orchestration with real local HTTP/TLS provider and conversational acceptance.
- [x] Port gfx typed resource storage, exact frame/submission ownership and buffer-range graph compilation; prove the compiled compute/transfer workload on native Metal 4. See `docs/gfx_odin.md`.
- [x] Port executable gfx buffer packets with native resource/reflection preflight and matching Metal/Vulkan consumers; validate concurrent uniforms, shared allocation hazards and retained native lifetimes.
- [x] Port shared WGSL shader compilation, complete reflection and asynchronous replacement with native Metal/Vulkan acceptance.
- [x] Port gfx images/subresources, graphics packets, allocation/aliasing, retained readback and window/surface lifecycle with native output and ownership tests.
- [x] Port fixed sampled/storage texture arrays, including 4096 material slots and native resource lifetime on both backends.
- [x] Import owned glTF/GLB geometry, materials, skinning and morph animation through confined asset roots.
- [x] Load/save scene files and capture/instantiate/remove prefabs atomically through canonical agent tools.
- [x] Add explicit Box3D primitive/mesh/hull bodies, body-only descriptions and native collider replacement acceptance.
- [x] Compose native textured models and GPU particles with staged scene replacement and windowed agent services.
- [x] Complete Box3D native joint ownership and whole-batch scene synchronization acceptance.
- [x] Add Box3D heightfields and spatial queries before exposing those application operations.
- [x] Unify GPU particle color composition with the native model linear/HDR path.
- [x] Port remaining application rendering, including shadows, point lights and postprocessing, with native acceptance.
- [ ] Move complete agent/gfx application consumers to Odin and remove the superseded Rust subsystems after native Vulkan and Metal acceptance.

## Further engineering

- [ ] Measure large trigger workloads before expanding predicates, stay events or sensor sweep detection.
- [ ] Design a character controller with slopes, stairs, jumping and explicit authored ownership.
- [ ] Extend state-machine/property and fuzz testing for storage, deferred commands, scene reference maps and confined resource operations.
- [ ] Run ThreadSanitizer on supported platforms for parallel ECS workers and audio control publication.
- [ ] Measure streaming callback deadlines and background decode needs on representative hardware.

The [historical roadmap](docs/archive/rust-roadmap-at-odin-cutover.md) preserves
previous implementation research and the superseded Rust-specific tasks. Its
completed work is not a second runtime, and its proposals are not parity claims.
