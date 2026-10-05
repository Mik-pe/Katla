# Native production material lighting

This executable compiles the canonical `model.wgsl` vertex and fragment entries
through the ordinary offline compiler. It renders linear RGBA16F and compares
actual readback against independent double-precision arithmetic. Both Metal and
Vulkan run with API validation. Each submission loses all public input texture,
buffer, pipeline and sampler handles before waiting; backend owners must retain
accepted work. The Odin allocation tracker must be empty after teardown. Native capture is enabled for both numerical
submissions; actual pipeline, descriptor, argument-table, attachment and
synchronization facts must match independent prepared-packet/reflection
expectations with zero divergence and real completed submission feedback.

The first matrix covers five roughnesses, four metallic values and six view/light
arrangements, including back-facing illumination and grazing views. Each of its
120 cases renders a normal-map-disabled pixel and an exact RGBA16F neutral-normal
pixel; the half-float bits must match. Each RGB value must be within
`max(abs(half(reference)) * 0.002, 0.0000002)` of the independently rounded reference.
This relative bound includes one half-float rounding step at large specular peaks.

The second matrix covers three transforms (identity, nonuniform shear and mirrored
nonuniform shear), four geometry paths and ten complete lighting arrangements.
The geometry paths are CPU-baked static geometry, a live model matrix, actual
`gltf_deform_geometry` with two weighted joints, and those same joints composed
with a residual model transform. Geometry is rendered by the production model
pipeline. The two distinct joint matrices blend to the independently referenced
transform. Mirrored winding preserves the authored front face.

The ten arrangements cover dielectric, metallic and mixed surfaces, roughness
0.1/0.4/0.7/1, two distinct point lights, ambient AO, normal scales 0/1/2/-1,
oblique normals, negative tangent handedness and linear HDR emission. Actual GPU
clears initialize lit and occluded D32 shadow atlases; production comparison
sampling must distinguish them. The complete lighting bound is
`0.00005 + abs(reference) * 0.0006`, matching the original upstream numerical
acceptance. Missing normal maps at scale zero and one must be bit-identical in
all twelve transform/geometry combinations. Both matrices also compare accepted
Metal and Vulkan half-float values directly.

Run through `odin run tools/build -- validate render --native-metal --native-vulkan` with
explicit compiler and Vulkan paths, or build this package with `-vet
-strict-style` and pass the compiler executable and Vulkan loader as its two
arguments. Set `MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1` before launch.
ASan runs preserve address checks and native validation; only external Apple
framework/driver process-exit leak detection is disabled for GPU launches. This
numerical test does not replace ordinary scene, alpha-coverage, texture-sampling,
skinning, window or editor acceptance.
