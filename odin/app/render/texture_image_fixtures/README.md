The authored 2×2 fixtures contain red, green, blue and white pixels. RGBA TIFFs
retain the PNG fixture's straight alpha values (255, 128, 64, 0); BMP is RGB24.
The TIFF encodings exercise uncompressed, Deflate, LZW, PackBits, BigTIFF and
Orientation 6. `oversize.tiff` is a dimension admission failure fixture.

`upstream-jpeg.tiff` and `upstream-gray16.tiff` are unchanged LibTIFF 4.7.2
`test/images/32bpp-None-jpeg.tiff` and `minisblack-1c-16b.tiff`. Attribution and
license are retained in `odin/image/licenses/libtiff.md`.
