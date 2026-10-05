# Direct C audio dependencies

The engine, voices, mixer, spatial consumers, schedules, cues and streaming worker
are Odin. This directory supplies only the device/codec ABI, with no Rust runtime
or audio-engine bridge.

Sources are copied without modification from the miniaudio `0.11.25` tag:

| Source | SHA-256 |
| --- | --- |
| `miniaudio.h` | `ac7af4de748b7e26b777f37e01cee313a308a7296a3eb080e2906b320cc55c89` |
| `extras/stb_vorbis.c` | `4c7cb2ff1f7011e9d67950446b7eb9ca044f2e464d76bfbb0b84dd2e23e65636` |

[Upstream release](https://github.com/mackron/miniaudio/releases/tag/0.11.25),
[device/decoder manual](https://miniaud.io/docs/manual/index.html).
Miniaudio's public-domain/MIT alternatives are in `LICENSE`; stb_vorbis's
public-domain/MIT license is retained in its source footer.

`audio_native.c` defines opaque ABI version 1. Native decoders borrow immutable
encoded bytes and produce f32 PCM. WAV, MP3 and FLAC select their actual native
decoders; Ogg Vorbis uses stb_vorbis's memory decoder for exact lengths and seeks.
No native file-loader is exposed. Each decoder has a 256 MiB allocation budget;
Vorbis uses a 16 MiB bounded arena. Source bytes are limited to 64 MiB. Device
creation opens paused stereo f32 output, and device teardown joins callbacks.

`audio_route_darwin.c` is a validation-only helper, linked from its separate
archive member only by the native example. It creates a temporary aggregate
backed by the current physical output and switches the real default output.
The test restores the original default and destroys the aggregate through
explicit cleanup. It is not part of the engine's production API.

Build reproducibly with `odin run tools/build -- --dependency audio --output target/odin-audio`. `CC` and `AR` may
select native compiler/archive tools. The ASan build resolves Clang matching the
installed Odin LLVM backend through the existing Box3D compiler resolver. The
builder verifies both pinned source hashes. The archive is a generated target
artifact; Linux and Windows compile the same source on their native toolchains.
