# Katla architecture

Katla is a Rust playground for real-time graphics and game development: Vulkan
and Metal, ECS, PBR, GLTF assets, a dockable editor, Luau scripts, Rapier physics
and audio. Manifests and source define the exact dependencies and public types.

## Workspace

```
katla_gfx    — Vulkan/Metal wrapper, render graph, materials, shaders (WGSL via naga)
katla_ecs    — Custom ECS: sparse storage, typed queries/systems, scoped parallel scheduler
katla_math   — SIMD math: Vec2/3/4, Mat2/3/4, Quat, Transform, AABB, Frustum
katla_ui     — Declarative retained-mode UI on top of immediate-mode core (Taffy layout)
katla_app    — Application framework, editor, systems bridging all crates
katla_physics — Rapier3D wrapper with ECS components
katla_audio  — Standalone audio pipeline (cpal + hound + lewton)
katla_script — Luau scripting via mlua, entity lifecycle hooks, sandboxed
katla_derive — Proc-macro crate: #[derive(Component)] with #[inspect(...)] attributes
katla_icons  — ForkAwesome icon constants for the UI
```

## Dependency boundaries

```
katla_math    → (nothing — zero internal deps)
katla_ecs     → katla_derive
katla_derive  → (nothing — proc-macro only)
katla_ui      → katla_math, katla_gfx, katla_icons
katla_physics → katla_ecs, katla_gfx, katla_math, rapier3d
katla_audio   → (nothing — zero internal deps)
katla_script  → katla_ecs, katla_math, katla_derive, mlua
katla_gfx     → (nothing — zero internal deps)
katla_app     → katla_gfx, katla_ecs, katla_math, katla_ui, katla_physics, katla_audio, katla_script
```

Preserve these boundaries when changing manifests. The GPU core must not depend
on application, ECS, math or UI crates; scene composition belongs in katla_app.

External libraries do not relax these internal boundaries. katla_ecs uses
katla_derive for component macros and Rayon for scoped workers. The proc-macro
crate has no runtime dependency. katla_math has no internal dependencies.

## Ownership and detailed contracts

| Area | Canonical contract |
| --- | --- |
| GPU core versus app features, frame/readback ownership | [Graphics ownership](graphics_core.md) |
| Metal native execution and capability limits | [Metal backend](metal_backend.md), [CI](ci.md) |
| Typed accesses, hazards, imported image states | [Graph synchronization](render_graph_synchronization.md) |
| Transient lifetimes and physical aliasing | [Transient storage](transient_storage.md) |
| Compute and animation/particle/light dependencies | [Graph compute](render_graph_compute.md) |
| Passive compiled/native diagnostics | [Frame capture](render_graph_capture.md) |
| Sparse storage, queries, systems, lifecycle | [ECS](ecs.md) |
| UI rendering, state and docking | [Declarative UI](declarative_ui_design.md) |
| Editor appearance and inspiration | [Editor visual design](editor_ui_design.md) |
| Physics ownership and integration | [Rapier decision](physics-engine-adr.md) |
| Thread-affine scripting and deferred commands | [Luau integration](katla_script_architecture.md) |

## Shader and asset pipeline

WGSL is canonical. Naga produces SPIR-V for Vulkan and MSL for Metal. Native
compilation, prepared variants and shader reload follow the
[Metal pipeline contract](metal_pipeline_cache.md); encoding does not compile.
Private Metal texture publication uses [staged uploads](metal_texture_uploads.md).

Static `.katmesh` recipes compile named parts into shared geometry; `.katprefab`
templates expand into editable scene subtrees with fresh entity references.
Mesh compilation, caching, persistence and AI authoring remain app-owned. See
[mesh and prefab contracts](prefabs.md).

GLTF assets provide meshes, PBR materials, skins and animation clips. Background
loading and material templates are application services.
ResourceManager::discover() locates resources/; use its path helpers. Persistent
scene format v3 uses stable document-local entity keys, explicit resource roots
and versioned game component codecs. Parsing and validation precede staged scene
replacement; older documents migrate through isolated readers. See the
[scene reference](../katla_app/src/scene/README.md) for schemas and lifecycle rules.

## Math and color

Mat4 stores four Vec4 columns: m[col][row], with m[0] denoting column zero.
This is the Vulkan/GLSL convention; do not transpose it for Metal. Transform
stores position, rotation and scale and composes them through make_mat4().
Application hierarchy resolution multiplies these matrices exactly; rendering
and bounds retain shear instead of recomposing an approximate world TRS.
Vec2/Vec3 are scalar; Vec4/Mat4/Quat use SSE on x86/x86_64. Hot operations are
inline. Spawned colors are sRGB and convert to linear before rendering. The
[type exports](../katla_math/src/lib.rs) define the current math inventory.

## Component inspection

The component derive macro emits Inspect implementations for editor builds.
Field annotations control skip, color, range, drag speed, display name, enum,
nested struct, vector/list and entity-reference behavior. Consult the
[derive API documentation](../katla_derive/src/lib.rs) before changing attributes.
