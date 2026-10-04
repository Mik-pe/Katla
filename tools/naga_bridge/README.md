# Naga compiler dependency

This standalone dependency helper imports no Katla engine crate. Odin owns shader
artifacts, queues, publication, native resources and rendering. Rust is retained
here only to call the pinned Naga WGSL compiler; removing a Rust engine subsystem
does not remove this external compiler dependency.

Build reproducibly with the helper's lockfile and an independent target directory:

```sh
cargo build --locked --manifest-path tools/naga_bridge/Cargo.toml --target-dir target/odin-naga-compiler
python3 scripts/validate_odin_shader.py --sanitize
```

The result is `libkatla_naga_compiler.dylib` on macOS,
`libkatla_naga_compiler.so` on Linux, or `katla_naga_compiler.dll` on Windows under
the target directory's `debug` subdirectory. Pass its explicit path to
`shader.compiler_init`; the application does not discover another compiler or
fall back to fixed shaders.

The versioned C ABI consists of `katla_naga_abi`, `katla_naga_compile` and
`katla_naga_free`. ABI 1 exchanges length-delimited UTF-8 JSON. A request contains
`abi`, WGSL `source`, selected `{name,stage}` entries, and named/numeric override
`constants`. A reply contains `abi`, the exact `compiler` identity, typed `error`,
owned `message`, and selected `entries`. Requests are limited to 8 MiB, 16 entries
and 1,024 constants. Odin also bounds replies to 128 MiB. Release each returned
buffer exactly once with the originating dependency; no exception may unwind
across the ABI. Tests exercise actual compilation and the allocation boundary.

Each selected entry is validated, has overrides resolved, and is emitted as
SPIR-V and MSL 3.0 from the same Naga module. Reflection preserves logical groups,
binding numbers, entry IO, actual read/write/query use, declaration permissions,
buffer spans and strides, image scalar/dimension/format, comparison samplers,
native argument indices and runtime-array size indices. MSL function names come
from the compiler's translation result and can differ from WGSL/SPIR-V names.

The binding ABI reserves Metal buffer 8 for runtime sizes and buffer 10 for vertex
attributes. Existing live ABI bindings retain group 0/binding 0 at buffer 0,
group 0/binding 1 at buffer 1, group 1/binding 0 at buffer 9, and group 1/binding 1
at sampler 0. Other live resources are assigned distinct available indices in
logical order. Binding arrays use Metal resource-ID argument buffers and report
their distinct native kind. Runtime-array size indices retain Naga's global
declaration ordering. Runtime-array minimum sizes include one element.

Buffer, ordinary index and image-load accesses use Naga's `ReadZeroSkipWrite`
bounds policy. Resource binding arrays use `Restrict`, because this Naga version
cannot emit zero/skip texture-array bounds handling. Both output backends use the
same policy. Runtime-sized resource binding arrays and multiplane external
textures fail explicitly. Native adapter limitations are documented separately
in [the Odin shader contract](../../docs/gfx_shader_odin.md).
