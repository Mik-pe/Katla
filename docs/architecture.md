# Katla architecture

The Odin engine owns scene authoring, retained editor UI, gameplay and native
Metal/Vulkan rendering. The [editor contract](odin_editor.md) defines application
acceptance; the [build contract](odin_build.md) defines native dependency builds
and executable publication. Source defines the current API.

## Package boundaries

| Package | Ownership |
| --- | --- |
| `odin/math` | Vectors, column-major matrices, quaternion/TRS operations, bounds and intersections |
| `odin/ecs` | Generational identities, sparse storage, queries, resources, events and scoped systems |
| `odin/editor` | Registered metadata/codecs, atomic restoration, owned history and correlated mailboxes |
| `odin/ui` | Retained nodes, layout, focus/input, text editing, draw lists and docking |
| `odin/gfx` | Generic GPU identities, resource lifetime, frame ownership and compiled graph execution |
| `odin/gfx/metal`, `odin/gfx/vulkan` | Native execution of the generic GPU contract |
| `odin/gfx/shader` | Validated external compiler artifacts and source/options/compiler identity cache |
| `odin/script` | Owned Luau VMs, protected native calls, typed host operations and lifecycle hooks |
| `odin/physics/box3d` | Native bodies, exact collider geometry, constraints, contacts and spatial queries |
| `odin/audio` | Codecs, clips/streams, voices, mixing, scheduling and device lifetime |
| `odin/resources` | Retained directory capabilities and confined filesystem operations |
| `odin/agent` | Validated tools, live scene admission and selected external conversation transport |
| `odin/app` | Scene/document/asset composition, gameplay services and reversible authored commands |
| `odin/app/render` | Textured/skinned models, lighting, shadows, particles, editor overlays and UI composition |
| `odin/app/editor` | Panel consumers, selection, camera/navigation, gestures and committed viewport services |
| `odin/katla` | Canonical startup, stationary owners and the once-per-frame loop |

Math and ECS remain independent. UI produces GPU-independent draw lists. Audio
does not import ECS, the renderer or application policy. Script's host boundary
uses typed values rather than borrowing application entities or VM references.
The GPU core does not import app, ECS, math, UI or scene composition. Backends
execute generic frames without interpreting editor policy.

Native dependencies cross explicit bounded C ABIs. Builds pin their sources and
compile artifacts for the selected host and instrumentation mode. Luau, Box3D,
miniaudio, font codecs, TOML and portable window services are explicit
dependencies. Image codecs and decompression are Odin packages. The standalone Naga compiler is a separate build process, never
linked into the running editor. The runtime has no Rust engine or layout FFI.

## Scene and publication ownership

One `Authoring` owns the live world, registry and shared command history.
Inspector, gestures, scene tools and external agent calls use that owner.
Commands prepare owned proposals and all native participants before publishing
a revision. Failed decoding, validation or native admission preserves the
accepted scene and history.

Existing-entity history changes only the components edited by its command.
Unchanged live components and animation clocks remain current. Create/Delete
owns full rows and maps references to fresh identities when restoring entities.
Editing-mode deletion includes incoming joint and trigger cleanup in the same
reversible command. Runtime deletion retains authored rules with diagnostic
stale targets; Stop restores the authored baseline.

Scene format v3 uses stable document-local keys, explicit asset origins and
registered component codecs. Parsing/migration and staging precede replacement.
File assets retain the explicitly selected parent directory and a confined child
name. Resource and project assets retain their installed roots. Decoders receive
bounded bytes and cannot reopen unrestricted filenames.

Scene/editor composition belongs in the application. Viewport pixels, picking
and metadata describe the same accepted GPU submission. Frustum intersection
means a geometric candidate, not proven pixel visibility. External conversation
attachment uses a selected existing conversation.

## Rendering and math

`Mat4` stores four columns: `m[col][row]`. Never transpose for a backend.
Quaternion, matrix and TRS rotation use one convention. Hierarchy composition
retains exact matrices, including shear. See [math](math_odin.md) and
[native graphics](gfx_odin.md).

Scene rendering uses linear HDR, real lights/shadows and one exposed tone-map
and transfer stage. Editor colors use sRGB authoring values and linear alpha
composition. Public GPU resource removal retires its identity while accepted
work keeps immutable references until completion.

WGSL compilation runs outside rendering through validated cached artifacts.
Graph compilation/submission owns execution; encoding does not spawn a shader
compiler. See [HDR/lighting](render_features_odin.md), [UI/picking](odin-ui-rendering.md),
[animation](animation_odin.md), [particles](particles_odin.md) and [audio](audio_odin.md).

## Validation

Strict Odin checks and meaningful CPU tests validate owned application state.
Native output, depth/picking readback, lifetime and failure/recovery validate GPU
behavior. Enable `MTL_DEBUG_LAYER=1` and `METAL_DEVICE_WRAPPER_TYPE=1` before Metal
launch. Builds, screenshots and cross typechecks do not establish unavailable
hardware or OS-input behavior. Metal CI uses exactly `macos-26` on Apple Silicon.
