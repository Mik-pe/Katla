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

The current application loader flattens a selected glTF scene into one drawable.
It uses the material assigned to the first primitive in depth-first node order,
from the default scene or first scene. Unreferenced document materials do not
select the drawable's appearance. A primitive with no material uses glTF defaults:
white base color, metallic `1`, roughness `1`, emission factor zero.

Base color factors are already linear and are copied without sRGB conversion.
Metallic and roughness factors initialize the drawable. Albedo and emissive
textures upload as sRGB; normal, MR and occlusion upload as linear UNORM. A single
image used for color and data roles receives separate uploads. Grayscale images
expand to RGB, grayscale-alpha retains alpha, and 16-bit integer channels
quantize to RGBA8. Malformed and floating-point images fail explicitly and log
an optional-texture fallback; failed uploads retain the role's existing fallback
and add no handle to resource tracking.

This is not full glTF material support. Additional primitives currently share
the first material. Alpha modes/cutoffs, double-sided shading, UV sets and
transforms, per-texture samplers, normal scale, occlusion strength, emissive
factors and material extensions need dedicated support. Scene serialization
preserves editable base color and PBR factors; texture assignment and standalone
material assets are not yet editable/persisted. Unresolved work and acceptance
criteria live in [TODO](../../../TODO.md#material-correctness).

## Shader reload

`recompile_materials_for_shader` keeps material handle identity, but its return
value counts affected materials, not completed successful compilations. Vulkan
invalidates variants and rebuilds at their next use. Metal queues background
replacements, retaining the previous pipelines if compilation fails. Atomic,
last-good reload behavior across backends and include dependencies remain open
work. Compile shaders and create materials during asset preparation to avoid
Vulkan frame-path compilation stalls.

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
