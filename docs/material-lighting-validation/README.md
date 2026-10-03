# Complete scene PBR lighting acceptance

The native fixture executes the unmodified `model_pbr.wgsl` and
`model_pbr_skinned.wgsl` entry points and reads linear RGBA16F output. On
2026-10-03, Intel Iris Plus Graphics (ICL GT2) Vulkan passed 120 comparisons with
an independent double-precision CPU lighting reference. The maximum absolute
RGB difference was **0.000191217**. The per-channel bound is
`0.00005 + abs(reference) * 0.0006`, allowing half-float output rounding and
floating-point shader arithmetic. Output alpha must equal one.

Three matrices (identity, nonuniform/sheared and mirrored nonuniform/sheared)
combine with four geometry paths: CPU-baked static vertices, live static model
transforms, skinned joint transforms, and composed model/joint transforms. Ten
lighting cases cover perceptual roughness 0.1/0.4/0.7/1, dielectric/metal/mixed
surfaces, two distinct point lights, directional shadow visibility, ambient AO,
normal scales 0/1/2/-1 and linear HDR emission. The normal/tangent attributes are
oblique and have negative tangent handedness. Rasterization uses no culling to
isolate frame and lighting correctness; separate strict native coverage fixtures
verify reflected culling and two-sided faces.

The initial fixture against the RGBA8 fallback from `8f5286b4` exposed a real
missing-map bias. At normal scale zero, a pixel was
`[0.3659668, 0.15588379, 0.06439209, 1]`; at scale one it was
`[0.36669922, 0.15612793, 0.06451416, 1]`. The fallback now uses exact
`(0.5, 0.5, 1, 1)` values in RGBA16F. The fixture requires bit-identical output
between normal scale zero and one whenever the normal map is missing, across
all twelve matrix/geometry combinations.

Reproduce on native hardware:

```bash
cargo test -p katla_app --lib material_tests::lighting_tests -- --ignored --nocapture
# On physical Metal 4 hardware:
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 cargo test -p katla_app --lib material_tests::lighting_tests -- --ignored --nocapture
```

[The failing fallback trace](biased-fallback.log) and
[the final eight-fixture material run](accepted.log) retain the original evidence.
The full PBR Vulkan fixture disables Khronos validation because this Intel driver
crashes compiling the full scene shader with that layer enabled. These readbacks
establish numeric rendering acceptance; they do not establish full-shader
validation-layer acceptance. The smaller primitive, UV, sampler and coverage
fixtures run with strict Vulkan API validation. Physical Metal acceptance is
unavailable on this Linux machine.

The headless fixture logs `MCP server error: connection closed: initialize request`
when stdin reaches EOF without an MCP initialization request. This is outside
rendering acceptance and is retained in the traces. The biased-fallback run also
logs implicit renderer cleanup after the deliberate failing assertion.
