# Odin shader compilation and replacement

WGSL is the canonical shader input. `odin/gfx/shader` owns compiler requests,
immutable binaries/reflection, bounded asynchronous jobs and replacement
publication. It imports no gfx, scene, ECS or application package. The pinned
[Naga dependency helper](../tools/naga_bridge/README.md) provides only compilation
through an explicit, versioned dynamic-library ABI.

`compiler_init` accepts an explicit dependency path and validates all mandatory
symbols, ABI and compiler identity. `compile` selects stage/name pairs and
override constants, returning an owned `Compiled` and a typed error. Destroy
even failed results with `compiled_destroy` to release their diagnostic. No
failure returns a partial ready artifact. `find_entry` selects a reflected entry;
WGSL/SPIR-V and translated MSL names remain separate. Compiler owners remain
stationary and outlive their service and active callers.

Reflection includes actual selected-entry resource use and separate declaration
permissions. A write-only entry using a `read_write` storage declaration requires
graph `Write`, while a query-only resource has `None` access and still retains
resource ownership. Buffer minimum spans, runtime-array offset/stride, selected
IO, sampled/storage image properties and per-stage native argument indices are
mandatory metadata. Compiler artifacts support constant resource binding arrays;
their native Metal representation can differ from their logical resource kind.

`odin/gfx/shader_adapter` is the single compiler-to-gfx descriptor mapping.
`compute` maps an immutable compute entry. `graphics` combines selected vertex
and fragment entries with application-authored raster state, validates vertex
inputs, stage linkage and color output types, and merges logical resources while
preserving per-stage Metal indices and runtime-size indices. Adapter owners own
descriptor arrays and borrow binary/source/raster arrays until synchronous native
pipeline preparation finishes. Native renderers validate actual compiled shader
interfaces before accepting descriptors.

The current native descriptor model accepts individual D2 images, including
arrayed/depth D2 images, and individual samplers. The adapter explicitly rejects
resource binding arrays, other image dimensions and multisampled shader images.
Storage image formats map only RGBA8 Unorm, RGBA16 Float and R32 Uint. Other formats
are explicit errors. It does not publish a partially supported pipeline.

`spirv.reflect_entry` decodes the actual selected SPIR-V stage for native Vulkan
validation. It follows selected function calls and resource pointer/handle use,
checks buffer layout spans and permission decorations, and reflects separate
images/samplers, comparison use, image numeric scalar types and storage formats.
Query-only operations preserve `None`. The existing compute-buffer `reflect`
API adapts this same decoder and rejects shapes it cannot represent. The decoder
is a bounded resource/interface decoder, not a complete semantic SPIR-V
validator; Vulkan validation remains mandatory. Combined image/sampler
declarations, runtime-sized resource binding arrays and pointer origins it cannot
prove fail explicitly.

`Service` has bounded outstanding work, including queued jobs, active compilation
and unconsumed results. Accepted requests receive monotonically increasing
revisions. Newer requests supersede queued or completed older work for the same
key. `service_close` ends admission, and `service_destroy` joins the worker and
releases all accepted work. Taken `Replacement` owners must be destroyed before
service destruction. Shader compilation never occurs during native encoding.

`Registry` publishes on its owner thread through mandatory native `prepare` and
`destroy` callbacks. Preparation creates a complete real native pipeline from a
candidate. A final revision check and publication are atomic with service
admission. Compiler errors, native preparation errors and superseded candidates
preserve the current pipeline. Native candidates that lose publication are
destroyed. Snapshots have exact release tokens; retired pipelines remain owned
until the last snapshot is released. Native frame owners independently keep a
submitted pipeline alive until GPU completion. Registry destruction returns
`Busy` while snapshots remain.

Run compiler, reflection, adapter, queue and publication ownership acceptance:

```sh
python3 scripts/validate_odin_shader.py --sanitize
```

On Darwin arm64 with Metal 4 and an available Vulkan loader/ICD, execute real
dual-backend rendering and pipeline replacement under native validation:

```sh
python3 scripts/validate_odin_shader.py --native --sanitize \
  --vulkan-library /usr/local/lib/libvulkan.dylib \
  --vulkan-icd /usr/local/share/vulkan/icd.d/MoltenVK_icd.json
```

The native consumer compiles arbitrary WGSL/overrides, renders two pipeline
versions, releases an old CPU owner while GPU work remains pending, verifies
1,024 exact RGBA pixels per backend, and preserves the current pipeline across
both compiler failure and an actual missing-entry native preparation failure.
The command enables `MTL_DEBUG_LAYER=1`, `METAL_DEVICE_WRAPPER_TYPE=1`, Vulkan
validation and Odin ownership tracking; `--sanitize` repeats Odin execution with
AddressSanitizer. CPU sanitizer tests enable process-exit leak detection. Native
sanitizer runs check addresses and the exact Odin allocation owner map; add
`--native-leaks` to audit external Apple/driver libraries as well. On the current
native host that broader audit reports Apple CoreFoundation TLS and Objective-C
class-initialization allocations retained by Metal/MoltenVK worker threads at
process exit, despite zero Odin-owned allocations. Native rendering acceptance
does not assert that external driver/framework process caches are leak-free.
These commands require the stated hardware and loader. CPU
tests alone do not establish native shader or rendering acceptance.
