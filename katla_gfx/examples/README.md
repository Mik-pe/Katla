# Native graphics validation

Native acceptance uses the same ordinary resource, pipeline and graph APIs as applications. Run the canonical fixtures instead of standalone executables with their own scheduling, synchronization and feature initialization.

The [graphics contract suite](../tests/contract/main.rs) validates meshes, textures, material state, queued replacements, readback and frame ownership. On a Vulkan host with validation layers:

```sh
cargo test -p katla_gfx --test contract -- --test-threads=1
```

The neutral compute fixtures in [render_graph/native_compute_tests.rs](../src/render_graph/native_compute_tests.rs) exercise the real animation and particle WGSL kernels, declared buffers, dispatches and completed GPU results. Run those fixtures on both backends, using the exact test names listed by:

```sh
cargo test -p katla_gfx --lib -- --list
```

[Shadow cascade tests](../tests/shadow_cascades.rs) cover CPU split ordering, camera coverage, matrix validity, texel size and light normalization. The corresponding shader integration is checked through the application-owned editor composition: its scene capture covers shadows and outline appearance, and its interaction suite checks exact picking results.

On a Metal 4 device, set both native validation variables before launch:

```sh
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 cargo test -p katla_gfx --test contract -- --test-threads=1
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 cargo run -- --headless -s --screenshot /tmp/katla-scene.png
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 cargo run -- --interaction-test /tmp/katla-interactions
```

See [native Metal validation](../../docs/metal4_validation.md) and [render graph capture](../../docs/render_graph_capture.md) for evidence and diagnostic comparison. A capability rejection does not establish native GPU acceptance.
