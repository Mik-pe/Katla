# Odin scene rendering features

The application owns feature collection, shader descriptors, native resource
parents and graph composition. The generic GPU packages receive ordinary
buffers, textures, render phases, compute dispatches and transfers. They do not
select scene features or material policy.

`Scene_Pipelines` contains the material, feature and display descriptors compiled
from the same selected WGSL entry metadata. `Native_Scene` allocates independent
mutable resources for every native frame slot. Immutable model and material
owners remain in the model consumer.

## Authored illumination and shadows

`Scene_Point_Light` uses the exact hierarchy world position, linear RGB,
intensity and range. The application admits at most 256 point lights before
prospective scene publication. A GPU dispatch builds each 16 by 16 pixel tile's
list, bounded to 128 lights. Material fragments consume the generated lists.
Hidden entities do not contribute. Point lights require a valid transform.

The lowest stable visible `Scene_Directional_Light` identity selects the single
shadow-casting sun. Its normalized direction describes rays shining toward the
scene; material shading uses the opposite direction toward the light.
`Scene_Environment` supplies linear ambient RGB times intensity. A supplied frame
sun is used when the scene has no authored sun, preserving standalone viewport
lighting without creating synthetic scene entities.

Four PSSM cascades use lambda 0.65, a maximum distance of 100 metres and a shared
2 by 2 D32 atlas. Each cascade freezes its own projection-index constant and
viewport. Quantized square extents and light-space texel snapping stabilize the
atlas. Depth bounds follow the actual frustum corners, with a one-metre near
pancake. Column-major matrices remain unchanged between backends; the vertex
entry applies the explicit clip-Y sign. Material shading uses 16 comparison
samples with a 5 percent transition between adjacent cascades.

`Feature_Settings.shadow_size` is configurable from 32 through 8192, with even
dimensions. Changing it requires `native_scene_resize` before preparation so
all replacement slot allocations are staged together. The canonical default is
2048 on both Metal and Vulkan. Polygon bias is zero; comparison bias is
1.5 divided by atlas size. Previous backend-specific exposure and atlas defaults
are replaced by the same explicit settings on both backends.

## Linear composition and display

Scene materials, model textures, environment, grid and particle billboards write
RGBA16 floating-point radiance. Texture color factors preserve their declared
sRGB or linear roles. Authored particle UNORM RGB is decoded into linear values
before billboard blending. No material shader performs early tone mapping.

The camera-derived environment uses the runtime sky gradient and sun disc/halo.
The grid draws 42 thin real floor line instances over a 20 metre extent in two
draw calls, at one-metre spacing with ten-metre major lines. Its upper surface
sits half a line thickness above the authored ground height and tests scene depth
without writing it.

Selection draws the actual primitive or model geometry. Visible stencil bit 1,
depth-failed occlusion bit 2 and an expanded silhouette produce the orange
outline. An R8 indicator mask carries occluded selected pixels into the final
display pass. Optional wallhack blends orange at alpha 0.4 after tone mapping.
Outline width scales by `0.004 * 1080 / viewport_height`.

One final pass applies exposure and the selected ACES, Reinhard, Tony McMapface
or Linear operator. Exposure defaults to 1. The final RGBA/BGRA8 UNORM texture
contains exactly one sRGB transfer and is readable and sampleable by later
application passes. It is separate from the HDR attachment. Later UI composition
must respect this encoded output contract; it must not tone-map the viewport a
second time. The unused Rust gamma setting and disabled SSAO/contact-shadow
shader paths do not create runtime controls or no-op feature packets here.

## Shared graph and transactional preparation

`scene_graph_append` declares one view directly into the caller's stationary
`gfx.Graph`. Its namespace keeps pass identities distinct; each view owns its
camera uploads, HDR, depth, final output, tile lists and shadow atlas. The
standalone initializer calls the same builder with its own graph.

The host acquires one token, calls `native_scene_prepare` sequentially for each
actual view, appends UI and picking passes, compiles the combined graph and
submits once. `Native_Prepared` owns its CPU input arrays until
`native_scene_prepared_accept` or `native_scene_prepared_abort` consumes them.
An unclosed preparation prevents a second preparation, resize or destruction.
Abort rolls back staged composition while preserving the acquired token for a
whole-frame retry. Accepted shared graph waits and exports belong to the host.
The host releases exports and truncates the graph before reusing declaration
roles; native accepted packets and queued readback tickets retain separate
owners.

The leading particle composition simulates once and owns the accepted burst
prefix. `Particle_View` followers reference those same generated particle,
survivor and indirect-command graph roles, but upload independent camera buffers
per view and native slot. Followers publish only their own view state. They do
not simulate again or consume another burst.

Four render consumers can use `install_participant=false`; one host participant
calls their public prepare hooks before committing any candidate. Candidate
publication preserves current feature settings and surviving selected entities.
Prospective light validation uses the transaction's selected entity membership,
including removals and whole-document replacement.

## Native evidence

`odin/examples/render_features` compiles real scene shaders with the explicit
offline compiler executable. It renders an authored floor, sphere, cube and
occluded sphere, plus directional and point components. It reads the generated
depth atlas, the actual HDR scene texture, final pixels and selection mask.

The normal and AddressSanitizer runs passed on this Apple M5 host with Metal API
validation and Vulkan validation enabled. Both backends produced the same
results: 55,986 populated shadow texels, 22,533 point-light pixels, 28,575 shadow
pixels, 413 grid pixels, 19,857 environment pixels, 1,010 outline pixels and 333
occluded mask pixels. All four tone operators checked 589,824 channels against
their actual HDR inputs. Each backend additionally checked 196,608 pixels over
four independently positioned camera outputs and four frame cycles in one
submission per cycle. Thirty-two genuine GPU particles appeared in all four
views; survivor counters, a 192-vertex indirect command, lifetime increments and
one consumed burst prove one simulation per accepted frame. Aborting all four
preparations before retry preserved that burst and reused the same token.

```sh
odin build odin/examples/render_features -vet -strict-style \
  -sanitize:address -out:/tmp/katla-render-features-asan
ASAN_OPTIONS=detect_leaks=0:halt_on_error=1 \
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 \
VK_ICD_FILENAMES=/usr/local/share/vulkan/icd.d/MoltenVK_icd.json \
  /tmp/katla-render-features-asan \
  /tmp/katla-odin-naga-build/debug/katla-shader-compiler \
  /usr/local/lib/libvulkan.dylib
```

The executable asserts zero Vulkan validation errors and zero Odin tracked
allocations. GPU ASan keeps address checks; process-exit leak detection is
disabled for external Apple framework/driver worker state. CPU ownership tests
retain their normal leak checks. These receipts establish these rendering paths;
they do not establish completion of editor input, every overlay or full product
feature parity.

## Explicit camera depth and planar receiver stability

`Depth_Sense` is app frame state, supplied explicitly to `native_scene_prepare` or `native_scene_render`. Forward is the default: scene/model depth compares use Less, depth-tested grid/outline/particles/overlays use LessEqual, and the attachment clears to 1. Reverse uses Greater/GreaterEqual and clear 0. Native owners prepare both raster variants from the same reflected shader artifacts before acquisition. This does not select an authored camera or alter editor interaction. The independent orthographic shadow atlas always uses forward depth and a LessEqual comparison sampler.

Finite and infinite cameras unproject an interior clip depth for sky, tile rays and cascade bounds, avoiding the zero homogeneous W at an infinite far endpoint. Cascades preserve the explicit near plane and a bounded shadow distance. Their existing `split_texel.z` word carries world-texel size converted into light depth. The 4×4 PCF receiver bias includes the surface normal's light slope and the complete filter footprint, so lit planar receivers do not show sampled self-shadow stripes. Cast shadows remain present.

Visible, stencil, outline, shadow and integer-picking shaders calculate `world = model * position` followed by `clip = view_projection * world`. A matrix-matrix-first expression can round depth differently from the visible pass and make equal-depth stencil marks intermittently fail. Identical operation order preserves the selected face and captured nearest-object identity.

The editor overlay native executable also renders a real primitive cube, a source Box.gltf model, GPU particles, grid, sky, cascaded shadows, contour and gizmos through forward → infinite reverse → forward frames. It verifies nearest-depth conversion, untouched clear values, exact integer object IDs and exact restored forward pixels. An additional 128 front/top planar samples compare shadows off/on and selection off/on: lit face interiors remain stable while actual floor cast shadows and outside contour pixels change. Both native adapters run under validation; GPU recordings retain removed public parents.
