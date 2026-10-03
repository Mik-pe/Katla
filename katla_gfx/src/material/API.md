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

`DrawCall::with_textures(MaterialTextures { ... })` overrides the four bindings
for that draw and all its instances while leaving the material unchanged.
`FrameContext` exposes the same operation through `DrawBuilder`. Omission uses
the material's bindings. Emission remains a separate draw texture handle.
Prepared object rows retain their resolved slots across sorting, pass selection
and frame-slot reuse. Submitted native bindings retain retired image allocations.

Handles retain generation checks. Backends resolve them to bindless slots only
when preparing draws. `TextureHandle::NONE` and stale handles resolve to slot
zero, the core's generic white fallback. This protects against sampling a
recycled slot; it does not supply PBR-specific defaults. The scene service binds
an explicit flat normal and neutral metallic/roughness texture.

| Role | Transfer function | Scene fallback | Shader use |
| --- | --- | --- | --- |
| Albedo | sRGB RGB, linear alpha | White | RGB × linear base color; alpha × base alpha |
| Normal | Linear | Exact `(0.5,0.5,1,1)` in RGBA16F | Decode, scale X/Y, normalize tangent-space normal |
| Metallic/roughness | Linear | White | B × metallic; G × roughness |
| Occlusion | Linear | White | `mix(1, R, strength)` × per-object AO, applied to ambient light |
| Emission | sRGB RGB | White × zero factor | Sampled linear RGB × linear emissive factor, added to HDR lighting |

Neutral MR channels are both one. Object defaults are metallic `0`, roughness
`0.5`, AO `1`; the texture must preserve those values. Bindless slots are GPU
addresses, never persistent asset identities. Emission's texture remains a
`DrawCall::with_emission(TextureHandle)` binding. Its RGB factor, normal scale and
occlusion strength belong to app `MaterialSurface`, independently of GPU handles.
`FrameContext::take_submission` returns geometry and surface values indexed by the
same object slots. Sorting/filtering preserves those indices. The scene composition
binds immutable `SurfaceParameters` bytes at group 0, binding 2; both backends own
their submission lifetime through ordinary pass packets. No scene-specific core
uniform fields or methods are required. Missing emission samples the white fallback;
zero emissive RGB means no emission even when an emissive texture is installed.

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
MikkTSpace with the transformed normal texture coordinates and split vertices at
corner seams. Missing normals also discard supplied tangents as required by glTF;
otherwise authored tangents survive import. Static positions preserve exact node matrices, including shear, and baked
negative determinants reverse triangle winding as well as tangent handedness.

Base color factors are already linear and are copied without sRGB conversion.
Metallic, roughness, normal scale, occlusion strength and emissive RGB factors
initialize the drawable. Eight-bit albedo/emissive images use hardware sRGB
sampling; normal, MR and occlusion images use linear UNORM. Grayscale expands to
RGB and grayscale-alpha retains alpha. Sixteen-bit data uses RGBA16 UNORM without
8-bit quantization. Sixteen-bit color decodes RGB into linear RGBA16F before
filtering; alpha remains linear. Decoded float images already contain linear
light and use RGBA16F in every role, preserving HDR and signed data within the
finite half-float range. Nonfinite values and values outside -65504..65504 reject
with pixel/component context rather than clipping. Half-float storage rounds to
its native precision.

Imports allocate the complete filtered mip chain. Both backends generate it after
base upload and regenerate it on complete image updates. The Vulkan device must
support filtered blits for the requested format; unsupported generation rejects
before allocating. Color mip filtering occurs in linear light.

An immutable decoded glTF asset shares uploads by canonical asset path, image
index and transfer function, including reuse between roles and primitives.
Decoded floats share one linear upload across color/data roles. Each drawable
tracks one reference per distinct image; editing factors or destroying one
owner preserves other owners. The cache holds generational handles without
pinning GPU allocations, rejects stale handles and reuses live uploads during
scene reconstruction. Sampler/UV choices do not change image identity. Malformed
optional images log a fallback, retain the role's existing default and add no
tracked handle.

Every texture role retains its image reference, UV set and sampler independently.
UV0/UV1 are supported; referenced missing or higher sets reject import with role
context. `KHR_texture_transform` applies scale, rotation in radians and translation,
including its UV-set override, on all five roles. Negative/zero scales are legal.
All six glTF minification modes, magnification and S/T address modes are preserved;
omitted filtering chooses linear mip interpolation and repeat with anisotropy one.
Material samplers occupy group 5, bindings 0..4 (albedo, normal, MR, AO, emission).
Depth, shadow and picking use the same transformed albedo coordinates and sampler.
Geometry phases group only consecutive objects with matching sampler policies to
preserve transparent compositing order.

The app owns `MaterialSampling` independently of `MaterialSurface`. Scene capture
persists both; omitting sampling preserves the imported source settings, including
when an older scene supplies surface factors. Agent inspection exposes image provenance, mesh UV availability and native fallback status; `set_sampling` patches one role atomically with undo and persistence. Scene staging rejects overrides selecting coordinates missing from a referenced image's mesh. The 208-byte `SurfaceParameters` row
contains five 32-byte coordinate transforms after its three existing vec4 fields.
If an authored normal UV differs from the coordinates used to generate tangents,
fragment derivatives reconstruct its frame; degenerate UVs retain the finite mesh
basis. Authored glTF tangents retain their supplied basis under UV transformation.
Other glTF material extensions require dedicated support. Scene serialization
preserves editable base color, PBR factors, surface multipliers and sampling; per-role image assignment now supports standalone files, embedded glTF images,
neutral defaults and original bindings with atomic history and portable references.
Standalone reusable material assets are not yet available. Unresolved work and acceptance
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
change runtime triangle winding. Application raster variants account for runtime
reflections; static import baking separately reverses winding for reflected node matrices.

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
The application `material_tests::lighting_tests` fixture additionally executes
both complete scene shaders, with HDR readback compared against an independent
double-precision lighting reference. Its 120 cases combine identity/nonuniform/
mirrored matrices with CPU baking, model transforms, joint transforms and their
composition. They cover roughness, metallicity, two distinct point lights,
directional shadow visibility, ambient occlusion, signed normal scale and HDR
emission. Missing normal maps use exact RGBA16F neutral values; scale zero and one
must produce bit-identical readbacks. The absolute-plus-relative tolerance accounts
for half-float output precision. Full-scene Vulkan compilation/execution disables
validation on the affected Intel driver; these pixels establish numeric rendering
acceptance, not validation-layer acceptance. Physical Metal execution remains
outstanding. See the [numeric acceptance evidence](../../../docs/material-lighting-validation/README.md).
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
The same static/skinned probes sample scaled normal maps, occlusion strength,
textureless emission and sRGB emissive textures multiplied by linear RGB factors,
before and after scene reconstruction. Native document tests cover HDR factor
editing, gesture grouping, undo/redo and capture without sRGB conversion of emission.

## Coverage and compositing

The application preserves glTF `alphaMode`, `alphaCutoff` and `doubleSided`
according to the [glTF coverage contract](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#alpha-coverage).
OPAQUE ignores texture/factor alpha and writes full coverage. MASK discards
sampled alpha times base alpha below its finite nonnegative cutoff, including
cutoffs above one; accepted fragments write full coverage. BLEND discards zero
alpha, composites straight-alpha color and coverage with the over operator,
tests scene depth and leaves it unchanged. Opaque draws precede transparent
draws; transparent instance centers sort far to near in view space without
integer distance quantization. Intersecting transparent triangles are not
order-independent transparency.

Color and auxiliary passes share coverage helpers. Depth and binary shadow maps
omit blended surfaces. Picking renders nonzero-alpha blend fragments into its
own depth attachment, selecting the nearest surface without changing scene
depth. Double-sided materials disable culling and reverse back-face shading
normals; opposite runtime transform handedness uses independent raster variants.
Static and skinned shaders use the same ambient and directional-shadow terms.
The editor and agent tool expose coverage modes, cutoff and double-sided state
through the same validated undo/persistence path as numeric factors.

Native static/skinned alpha fixtures use the actual depth, picking and shadow
shader sources with a small coverage/color probe. Readback verifies texture and
factor cutoff holes, cutoff above one, opaque alpha suppression, two overlapping
blend layers, alpha accumulation, unchanged scene depth, nearest picking, shadow
holes, reversed normals and mirrored culling. Vulkan validation errors fail the
fixture. Metal runs the same fixture with its required debug environment when
native hardware is available.

Portable `.katmat` definitions and complete-material history belong to
`katla_app`; GPU core materials remain pipeline handles with generic images.
Authoring capture resolves inherited image choices, and apply copies factors,
sampling and images without replacing mesh or pipeline identity. File writes
leave live copies unchanged. [Authoring workflow](../../../docs/agent-authoring.md#reusable-surfaces).
