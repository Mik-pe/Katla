# Materials and surface parameters

`GpuRenderer::compile_material(&PipelineDescriptor)` creates a generational
`MaterialHandle` on either Vulkan or Metal. The graphics core owns pipeline
state and resource bindings; the application owns PBR semantics, imported assets,
neutral textures and editable surface values. WGSL is canonical, translated to
SPIR-V or MSL. See [graphics ownership](../../../docs/graphics_core.md).

## Create and share a material

```rust
use katla_gfx::{GpuRenderer, ImageFormat, PipelineDescriptor};

let descriptor = PipelineDescriptor::pbr(resources.shader_path("model_pbr.wgsl")
    .to_string_lossy().into_owned())
    .with_color_format(ImageFormat::R16G16B16A16Sfloat);
let material = renderer.compile_material(&descriptor)?;
```

Use `pbr`, `skinned`, `ui`, `simple` or `depth_only` for the corresponding vertex
layout and initial state. Generated geometry uses `VertexLayout::empty()`.
Custom entry points use `with_graphics_entries`. Depth, stencil, culling,
blending and attachment state belong to the descriptor; shaders and explicit
pass bindings must match it. See the
[graph API](../render_graph/API.md) for binding packets and drawing phases.

Compilation creates an independently owned handle each time. Share that handle
across meshes when shader, render state and texture bindings are the same.
Per-object color, metallic, roughness and AO belong to `DrawCall` instances:

```rust
use katla_gfx::DrawCall;

let draw = DrawCall::new(mesh, material)
    .with_color([0.8, 0.2, 0.1, 1.0]) // linear RGBA
    .with_pbr(0.75, 0.25, 1.0);
```

Scene pipelines target the graph's HDR attachment, not the swapchain. Concrete
formats compile immediately on Vulkan. `ImageFormat::Auto` defers Vulkan
pipeline compilation until the pass format is known. Metal prepares supported
color variants during material creation; frame encoding requires an existing
variant and does not compile. Cached variants are scoped to the material.
Invalid initial compilation publishes no handle. Destroy handles through
`GpuRenderer::destroy_material`; submitted work retains the native resources
until retirement.

## Texture roles and color space

```rust
use katla_gfx::MaterialTextures;

renderer.set_material_textures(material, MaterialTextures {
    albedo,
    normal,
    metallic_roughness,
    occlusion,
});
```

Handles retain generation checks. Backends resolve them to bindless slots only
when preparing draws. `TextureHandle::NONE` and stale handles resolve to slot
zero, the core's generic white fallback. This protects against sampling a
recycled slot; it does not supply PBR-specific defaults. The scene service binds
an explicit flat normal and neutral metallic/roughness texture.

| Role | Transfer function | Scene fallback | Shader use |
| --- | --- | --- | --- |
| Albedo | sRGB RGB, linear alpha | White | RGB × linear base color; alpha × base alpha |
| Normal | Linear | `(128,128,255,255)` in RGBA8 | Tangent-space normal |
| Metallic/roughness | Linear | White | B × metallic; G × roughness |
| Occlusion | Linear | White | R × per-object AO |
| Emission | sRGB RGB | No emission | Add sampled RGB to linear HDR lighting |

Neutral MR channels are both one. Object defaults are metallic `0`, roughness
`0.5`, AO `1`; the texture must preserve those values. Bindless slots are GPU
addresses, never persistent asset identities. Emission is currently a separate
`DrawCall::with_emission(TextureHandle)` binding with no color/intensity factor.

## glTF import contract and limits

The application imports the default scene, or the first scene if no default is
specified. Each selected node primitive retains its own material, skin and u32
triangle indices. Unreferenced document materials do not affect appearance.
A primitive with no material uses glTF defaults: white base color, metallic `1`,
roughness `1`, emission factor zero.

A single primitive spawns one drawable. Multiple primitives spawn a model
controller and independent child drawables, each with editable factors and owned
textures/material state. Scene capture expands whole-model sources into explicit
`GltfGroup`/`GltfPrimitive` origins, so reload reconstructs each surface once.
Children inherit the nearest animation player, with per-child overrides. Each
primitive uses its node's skin; unselected playback evaluates the rest pose.
Group mesh colliders combine model geometry independently of surface drawing.

Accessor iterators preserve offsets, strides, sparse data and normalized skin
weights. Triangle strips/fans convert to triangle lists; other topologies reject
import. Missing normals use flat triangle normals. Missing tangents with UVs use
MikkTSpace and split vertices at corner seams; supplied skinned tangents survive
import. Static positions preserve exact node matrices, including shear, and baked
negative determinants reverse triangle winding as well as tangent handedness.

Base color factors are already linear and are copied without sRGB conversion.
Metallic and roughness factors initialize the drawable. Albedo and emissive
textures upload as sRGB; normal, MR and occlusion upload as linear UNORM. A single
image used for color and data roles receives separate uploads. Grayscale images
expand to RGB, grayscale-alpha retains alpha, and 16-bit integer channels
quantize to RGBA8. Malformed and floating-point images fail explicitly and log
an optional-texture fallback; failed uploads retain the role's existing fallback
and add no handle to resource tracking.

This is not full glTF material support. Alpha modes/cutoffs, double-sided shading, UV sets and
transforms, per-texture samplers, normal scale, occlusion strength, emissive
factors and material extensions need dedicated support. Scene serialization
preserves editable base color and PBR factors; texture assignment and standalone
material assets are not yet editable/persisted. Unresolved work and acceptance
criteria live in [TODO](../../../TODO.md#material-correctness).

## Surface lighting and transforms

Roughness is perceptual roughness throughout import, authoring and object data.
The shared GGX shader clamps it to `0.04..1`, uses `alpha = roughness²` and
`alpha² = roughness⁴`, and evaluates height-correlated Smith visibility.
Directional and point lights each evaluate their own Fresnel and diffuse energy
partition. The direct-light BRDF returns zero below either surface hemisphere.

Static and skinned shaders transform normals with the affine inverse transpose,
transform tangents forward, then orthogonalize and normalize the surface frame.
Negative determinants reverse tangent handedness. Skinned frames use the
combined model and blended joint transform. Static glTF baking follows the same
contract. Singular transforms produce a finite fallback frame; they do not
define a physically meaningful surface. Transforming the tangent frame does not
change runtime triangle winding or culling state. Static import baking separately
reverses winding for reflected node matrices.

## Shader reload

Materials own independently editable texture sets and variant handles. Vulkan and
Metal reuse immutable native pipelines across materials with identical expanded
source, selected entry points, vertex layout, attachments and render state.
Source equality is exact, so an include edit invalidates reuse even when its path
is unchanged. Restoring unchanged source can reuse retained last-good pipelines.
Canonical reload dependencies remain per material. Weak cache ownership permits
retirement when the final material and submitted work release a pipeline; Vulkan
descriptor layouts share that lifetime. Metal serializes cache lookup/build across
reload workers to avoid duplicate creation. Failed preparation publishes no new
material state. See the [measured reuse study](../../../docs/material-pipeline-study/README.md).

`recompile_materials_for_shader` matches canonical source and transitive include
paths. Equal filenames in separate directories are distinct; shared includes
reload only their dependents. Both backends use one include resolver, suppress
duplicate includes and reject include cycles or malformed directives.

Each replacement uses one expanded source snapshot for every live variant,
including instanced UI. Pipelines, reflected bindings and dependency identities
publish together after successful preparation. A failed replacement leaves the
last working material intact, including texture bindings and handle identity.
Submitted work retains the old native pipelines; Vulkan descriptor layouts retire
with their pipelines. The return value counts affected materials, including
failed or queued replacements, rather than successful compilations.

Vulkan prepares replacements synchronously during the reload request. Metal
prepares them on a worker using the renderer's device and shared compiler archive;
it creates no additional context, surface or queue. Frame preparation polls ready
replacements. A newer request supersedes a pending Metal result. Compile shaders
and create materials during asset preparation to avoid Vulkan frame-path stalls.

## Verification

Portable parsing and image regressions:

```bash
cargo test -p katla_app --lib gltf_ --locked
```

Native import/default texture regression, on the platform backend:

```bash
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 cargo test -p katla_app --lib material_tests --locked -- --ignored --test-threads=1
```

The native fixture spawns a real glTF asset and checks exact linear drawable
factors. GPU sampling/readback verifies neutral MR channels preserve those
factors, normal defaults are flat, and shared color/data images decode
according to their respective transfer functions. It also checks that malformed
optional images publish no tracked texture. Vulkan PBR compilation in this
fixture disables the validation layer because of the documented Intel compiler
crash; Metal runs with the validation environment above. Physical Metal
acceptance requires a Metal 4 device.

`cargo test -p katla_gfx --lib render_graph::native_compute_tests::materials -- --nocapture`
executes the actual shared shader helpers and compares GPU readback with
independent scalar BRDF results and affine static/skinned frame references.
Shader-interface validation covers both complete scene shaders. CPU baking
regressions run with `cargo test -p katla_app --lib test_static_material_frame`.

The native compute suite also runs `material_reloads`: last-good pixels after
syntax failure, shared-include edits, duplicate filenames, binding-interface
replacement, submitted work across replacement, and atomic plain/instanced UI
preparation. The platform selects its native renderer; Metal requires the debug
environment and Metal 4 hardware described above.

Native per-primitive acceptance runs with:
`cargo test -p katla_app --lib material_tests --all-features -- --ignored --nocapture --test-threads=1`.
The static/skinned fixtures read independent texture/color/metallic/roughness
pixels before and after scene reload, select the node's second skin through the
real GPU pose/copy/draw path, reject missing primitive identities atomically,
expand whole-model material overrides, preserve group colliders and retire all
owned resources. These probes complement the shared BRDF arithmetic checks;
they are not full-scene visual quality benchmarks. Physical Metal acceptance
requires its native hardware and debug environment.
