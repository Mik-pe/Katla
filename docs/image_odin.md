# Odin image decoding

`odin/image` owns bounded encoded-byte admission, native-endian RGBA samples and
allocator-captured destruction on every platform. It has no native FFI,
LibTIFF/IJG/stb/zlib binary, platform-specific codec builder or system decoder.
PNG uses Odin's core decoder after Katla's framing and expansion checks.
BMP uses bounded Odin row, palette and bit-mask decoding with preserved alpha.

JPEG uses one Odin Huffman decoder for sequential and progressive scans.
It tracks coefficient initialization and refinement per component/band,
noninterleaved and interleaved MCUs, EOB runs, restart sequences, quantization,
IDCT and centered chroma upsampling. Grayscale, RGB/YCbCr and CMYK/YCCK produce
RGBA8 without applying a transfer function. Arithmetic and lossless JPEG are
explicitly unsupported. Working coefficient/plane memory is bounded to 256 MiB.

Classic TIFF and BigTIFF are parsed directly from a bounded byte slice. Metadata
is limited to 4 MiB before allocating output. Strips, tiles, planar samples,
byte order, all eight orientations, associated alpha and integer/float precision
are owned in Odin. Uncompressed, Deflate, TIFF LZW, PackBits and JPEG compression
are admitted. RGB, grayscale, palette and CMYK samples are expanded explicitly;
unsupported sample encodings or compression return `Unsupported`.
Integer horizontal and floating-point predictors retain native precision.

Encoded input is limited to 32 MiB, either dimension to 8192, pixel count to
16 million and expanded RGBA output to 64 MiB. Deflate writes into the exact
preflighted output slice and checks stream termination and Adler-32. It never
resizes output in response to compressed input. PNG also checks chunk CRCs,
framing, contiguous IDAT and exact filtered/interlaced expansion.

PNG/TIFF 16-bit channels remain RGBA16; TIFF IEEE float channels remain
RGBA32_Float. Nonfinite float samples and values outside the half-float range
are rejected. Display-preview conversion is a separate owned operation and
preserves the original material data. Rendering owns role-specific transfer
functions and GPU upload formats.

```sh
odin test odin/image -vet -strict-style \
  -define:ODIN_TEST_THREADS=1 -define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true
odin run tools/build -- --tests
```

Progressive fixtures cover 4:4:4, 4:2:2, 4:2:0 and restart markers, compared with
independently decoded reference pixels. Other tests exercise real embedded
glTF images, TIFF compression/endian/planar/tile/orientation and HDR precision,
malformed/truncated streams, expansion limits and failed allocations.
Native texture upload/readback acceptance remains separate in the
[render contract](render_features_odin.md).
