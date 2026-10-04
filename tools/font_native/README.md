# Native font dependency

The application shapes Roboto and ForkAwesome from `resources/fonts` using a
direct C ABI over source-pinned FreeType, HarfBuzz, SheenBidi and libunibreak.
The font dependency contains no engine, UI, ECS or Rust bridge.

`python3 tools/font_native/build.py` produces a dynamic library and the licensed
Noto fallback fonts in `target/font-native`. The build uses Clang directly and
records exact source commits. Fallback fonts come from one pinned Google Fonts
commit and each font is checked against its committed SHA256 before use. Fonts
are required data next to the library in `fonts/`; missing dependency data fails
initialization instead of inventing metrics or silently substituting glyphs.

The C ABI owns FreeType faces and immutable layouts. Shaping resolves visual
Unicode bidi levels and script runs, chooses a fallback for whole graphemes,
applies HarfBuzz kerning/ligatures and returns original UTF8 cluster offsets.
UAX14 line opportunities and UAX29 grapheme boundaries drive wrapping and
logical deletion. Each final wrapped line is reshaped and checked against its
width. The same layouts implement measurement, primary caret positions, visual
arrow navigation, pointer hits and glyph drawing. Empty text occupies one line;
line height is font size × 1.2. Layout caching is bounded to 128 entries / 64 MiB;
individual layouts larger than 16 MiB are shaped without being cached.

The atlas stores real grayscale FreeType coverage in RGBA alpha. CPU atlas
revisions produce immutable native texture versions. Old accepted submissions
retain their exact texture; a later atlas change cannot replace it in flight.
Offscreen glyphs avoid raster/upload work. A full atlas first evicts old glyphs,
then grows up to 8192² while rebuilding the whole unpublished mesh so its UVs
stay coherent. Oversized visible glyph sets fail explicitly.

The byte-only primary caret interface chooses one visual position at ambiguous
bidi/wrap boundaries. It does not expose a separate leading/trailing caret
affinity. Color emoji are rendered by the pinned monochrome Noto Emoji fallback;
full color emoji font rasterization is not implemented.

## Validation

Run all font CPU checks plus actual native UI/picking acceptance:

```sh
python3 scripts/validate_odin_ui_gpu.py \
  --shader-compiler /path/to/katla-shader-compiler \
  --vulkan-loader /path/to/libvulkan.dylib \
  --vulkan-icd /path/to/MoltenVK_icd.json
```

`--backend metal` or `--backend vulkan` selects one native path; `--cpu-only`
checks the font dependency and generic graph contract on other hosts. The
current paired native harness requires macOS on Apple Silicon and reports
other hosts as unavailable instead of passing unexecuted GPU checks.

For `--sanitize`, use C/C++ compilers with the **same ASan runtime as Odin**.
For Homebrew Odin linked to LLVM 22, add:

```sh
--sanitize --cc /opt/homebrew/opt/llvm@22/bin/clang \
--cxx /opt/homebrew/opt/llvm@22/bin/clang++
```

CPU tests run with LeakSanitizer enabled. Only actual GPU executables disable
process-global leak detection at the external CF/ObjC ownership boundary;
Odin tracking still requires every application allocation to be released.
Apple's different ASan runtime cannot be mixed with the LLVM runtime while
claiming LeakSanitizer coverage; the validator fails such a mismatch.

Font/source license notices accompany the dependency and downloaded fallback
fonts. FreeType is distributed under the FreeType License; HarfBuzz, SheenBidi
and libunibreak retain their upstream notices. Noto fonts retain their SIL OFL.
