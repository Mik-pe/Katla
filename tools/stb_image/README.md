# Image decoder dependency

The application decodes bounded PNG, JPEG, BMP and TIFF byte streams into owned,
top-to-bottom RGBA8. GPU color-space decisions stay in composition.

STB image 2.27 is the unmodified header distributed with Odin 2026.09. Its
SHA-256 is `ff16c825d20839a15a21ad833da821d640d04c2cc97a0f744afcd94511cff298`.
The header retains upstream attribution and the MIT/public-domain license. The
C11 implementation enables PNG, JPEG and BMP memory decoding; STB symbols stay
private. The five `katla_image_*` functions expose dimensions, RGBA decoding,
deallocation, thread-local orientation and fixed-output zlib inflate.

TIFF uses pinned LibTIFF 4.7.2 and IJG JPEG 9f. Source archives are downloaded
from their official release sites into `target/image-codec-source` and their
SHA-256 values and extracted source files are verified on every build:

| Source | SHA-256 |
| --- | --- |
| `https://download.osgeo.org/libtiff/tiff-4.7.2.tar.xz` | `4996f0c4f93094719b1ca5c6279b20e588773ba8a247533e486416fb662ddb88` |
| `https://www.ijg.org/files/jpegsrc.v9f.tar.gz` | `04705c110cb2469caa79fb71fba3d7bf834914706e9641a4589485c1f832565b` |

Attribution and distribution terms are retained in `licenses/libtiff.md` and
`licenses/ijg-jpeg.txt`. The JPEG dependency supports compressed TIFF; standalone
JPEG continues through STB's existing strict framing path. TIFF supports its
standard built-in codecs, including LZW, PackBits, JPEG and Deflate. The final
static library contains the pinned JPEG and TIFF implementations. Unix hosts use
the platform zlib linker dependency; Windows builds also include pinned zlib 1.3.2
(SHA-256 `bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16`).

The application caps encoded streams at 32 MiB, each dimension at 8192, and
pixels at 16 million (64 MiB RGBA). PNG validates CRC, Adler and exact inflated
scanline size; JPEG validates framing; BMP validates headers, palettes, masks
and padded rows. TIFF uses only a caller-supplied memory stream. Per-handle
LibTIFF allocation limits are 4 MiB for metadata admission and 64 MiB during
decode. Exceeding metadata limits is an explicit admission failure even if
LibTIFF could otherwise ignore the offending optional tag. TIFF normalizes all
eight orientations and returns straight alpha. RGB8 scanline decoding retains
unassociated transparent RGB samples exactly. No filename loader or global
orientation setter is exposed.

```sh
python3 scripts/build_odin_image.py
odin test odin/app/render -vet -strict-style
```

The builder needs Python, a C11 compiler and an archive tool. Unix hosts use
configure/make; Windows hosts use Clang, LLVM ar, CMake and Ninja.
It honors `CC` and `AR` and creates `target/odin-stb-image/libkatla_image.a`.
Compiler/version/flags are recorded so changing instrumentation rebuilds native
dependencies. For another output location use the `-define:STB_IMAGE_LIBRARY`
argument printed by the builder, relative to `odin/deps/stb_image`.

Odin cross-platform typechecks use the same declarations. The Windows CMake
path builds the verified IJG, TIFF and zlib sources and merges native `.obj`
objects into the same decoder archive. Native Windows runtime image acceptance
requires execution on Windows hardware.

For combined native decoder and Odin AddressSanitizer instrumentation:

```sh
python3 scripts/build_odin_image.py --sanitize
odin test odin/app/render -vet -strict-style -sanitize:address -debug \
  -define:STB_IMAGE_LIBRARY=../../../target/odin-stb-image-asan/libkatla_image.a
```

The builder discovers a Clang major matching the live `odin report` LLVM backend.
Supply `CC` when automatic discovery cannot find a matching runtime. All native
libraries used by the consumer must share that sanitizer runtime.

Upstream: [STB](https://github.com/nothings/stb),
[LibTIFF](https://libtiff.gitlab.io/libtiff/), [IJG](https://www.ijg.org/).
