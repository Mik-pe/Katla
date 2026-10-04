# PNG/JPEG decoder dependency

The repository pins the unmodified STB image 2.27 header distributed with Odin
2026.09. Its SHA-256 is
`ff16c825d20839a15a21ad833da821d640d04c2cc97a0f744afcd94511cff298`.
The header retains upstream attribution and the MIT/public-domain license.

The C11 implementation enables PNG and JPEG memory decoding only. STB symbols
stay private; five `katla_image_*` exports provide real dimensions, RGBA
decoding, deallocation, thread-local orientation and fixed-output zlib inflate.
No file loader, image writer, global orientation setter or color-space policy
is exposed. The application validates encoded/decoded budgets and PNG framing,
CRC, Adler and exact inflated scanlines before the decoder allocates pixels.

Build native code from the checked-in source:

```sh
python3 scripts/build_odin_image.py
odin test odin/app/render -vet -strict-style
```

The builder requires a C11 compiler and archive tool. It honors `CC` and `AR`,
using `cc`/`ar` on Unix or `clang`/`llvm-ar` on Windows. It creates
`target/odin-stb-image/libkatla_image.a`; Odin imports this artifact explicitly.
For another output location use the exact `-define:STB_IMAGE_LIBRARY` argument
printed by the builder, relative to `odin/deps/stb_image`.

Cross-platform Odin typechecks use the same declarations. Linking or running
a different target requires building its native library for that target with
the corresponding compiler/archive tools; installed Odin vendor binaries are
not required. Missing native dependencies fail the build explicitly.

For native decoder and Odin AddressSanitizer instrumentation:

```sh
python3 scripts/build_odin_image.py --sanitize
odin test odin/app/render -vet -strict-style -sanitize:address -debug \
  -define:STB_IMAGE_LIBRARY=../../../target/odin-stb-image-asan/libkatla_image.a
```

The sanitizer builder discovers a Clang major version matching the live
`odin report` LLVM backend. Supply `CC` explicitly when automatic discovery
cannot find a matching runtime. Apple Clang is not automatically selected for
this combination. Other native dependencies used by the consumer must also
use a matching sanitizer runtime.

Upstream: [STB](https://github.com/nothings/stb).
