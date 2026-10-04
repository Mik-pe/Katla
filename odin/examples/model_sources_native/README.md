# Native independent imported primitives

This fixture loads the checked glTF through `scene_action_execute(Spawn_Model)`
and the ordinary native scene admission participant. The controller owns one
immutable source revision and animation player. Its two primitive children have
different materials, exact source selectors and independently editable factors.
They use the production model shader and material preparation path.

Both Metal and Vulkan check real pixels and native validation:

- Sampler allocation failure preserves the empty scene and publishes no entity.
- A left-child material edit changes its pixels while every right-half pixel is
  unchanged. Shared undo and redo restore the exact previous images.
- Controller Play and Seek deform both selected children through a genuine skin.
  Their accepted native cache and pipelines remain unchanged.
- Repeating the same sampled clock produces identical pixels and no new geometry
  revision. Native material inspection reports the actual accepted fallback
  receipts. The Odin allocation tracker is empty after teardown.

Build `odin/examples/model_sources_native` with the canonical dependency defines,
`-vet -strict-style`, and optionally `-sanitize:address`. Pass the offline shader
compiler executable and Vulkan loader as the two arguments. Set
`MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1` before launch. Combined C/Odin
ASan launches keep address checks; only external Apple framework/driver
process-exit leak detection is disabled for the GPU process. CPU tests retain
LSan. This fixture proves canonical source selection and consumer lifecycle,
alongside the separate numerical lighting and alpha-coverage suites.
