# glTF parser dependency

The repository contains the cgltf 1.14 snapshot and ABI-matched Odin bindings
bundled with Odin 2026.09. The exact unmodified `cgltf.h` SHA-256 is
`19db3bdf85f6aa991a3b9d056641bcb39a2ef11cd5211e7a9f6b6c307d9ebc64`.
The header retains its MIT license and upstream attribution. `cgltf.c` selects
the parser implementation; the application does not compile the writer or use
cgltf's unconstrained file loader.

Build from the checked-in source, independent of installed vendor libraries:

```sh
odin run tools/build -- --dependency gltf --output target/odin-cgltf
odin test odin/app -all-packages -vet -strict-style
```

The builder uses the native `CC` and `AR` commands when supplied. Defaults are
`cc`/`ar` on Unix and `clang`/`llvm-ar` on Windows. It requires a C99 compiler and
creates `target/odin-cgltf/libcgltf.a`. `odin/deps/cgltf` imports that repository
artifact by default. A different output location must be selected explicitly
with the exact `-define:CGLTF_LIBRARY` argument printed by the builder on every
consumer build. That path is relative to the binding package, independently of
the shell's current directory.
Cross-platform Odin typechecks can use this declaration, but running or linking
a different target requires building the parser for that target as well.

For instrumented parser and Odin ownership acceptance, select a C compiler
whose AddressSanitizer runtime ABI matches Odin. On the tested macOS host this
is upstream LLVM 21.1.8; Apple Clang 21 has a distinct incompatible runtime:

```sh
CC=/path/to/llvm/clang odin run tools/build -- --dependency gltf --output target/odin-cgltf --sanitize
odin test odin/app -all-packages -vet -strict-style -sanitize:address \
  -define:CGLTF_LIBRARY=../../../target/odin-cgltf-asan/libcgltf.a \
  -define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true
```

The application importer installs a bounded allocator,
preflights accessor storage before C validation/unpacking, and reads every
external URI through the retained resource root. Data URI buffers, sparse
accessors and normalized integer attributes pass through the actual parser and
accessor converter. No glTF content or engine behavior is implemented in Rust.

Upstream: [cgltf](https://github.com/jkuhlmann/cgltf).
Application contract: [Odin glTF ownership](../../docs/gltf_odin.md).
