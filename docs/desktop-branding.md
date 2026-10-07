# Desktop branding

The README uses `assets/katla-cover.png`. The desktop icon's canonical source is
`assets/katla-icon.png`, a 1024px RGBA image with transparent margins. Both share
Katla's angular dragon/K silhouette, obsidian surfaces and red-orange glow.
Generation prompts are retained in `output/imagegen/`.

The `game` executable embeds the PNG and passes its decoded RGBA pixels to
`ApplicationBuilder::with_window_icon`. The framework accepts an app-owned
`winit::window::Icon`; it does not impose Katla branding on other applications.
On macOS, winit does not set a Dock icon. The executable installs its embedded
multi-resolution `assets/katla-icon.icns` through AppKit after creating the
window. Headless runs do not initialize desktop icons.

The local bundle compiles the canonical PNG into `Assets.car` and `AppIcon.icns`
with Xcode's `actool`. `packaging/macos/Info.plist` names `AppIcon` so Finder can
display the icon before launch. The asset catalog preserves the dark icon without
an extra system background tile. Create an unsigned local development bundle with:

```bash
python3 scripts/bundle_macos.py
open target/macos/debug/Katla.app
```

Use `--release` for a release build, or `--skip-build` to package a binary already
built with the same Cargo target directory. Set `CARGO` to select a local Cargo
executable if needed. Packaging requires Xcode's command-line tools. The bundle
includes runtime resources and scenes, and its
launcher resolves them relative to the bundle instead of the working directory.
This local bundle still requires installed Vulkan/MoltenVK libraries where used;
runtime bundling, signing, notarization and installers remain release work.

The ICNS includes 16, 32, 128, 256 and 512 point images at 1x and 2x resolution.
After changing the source PNG, resize copies with `sips` into these standard
`icon_<size>x<size>[@2x].png` filenames in an `.iconset` directory, then run
`iconutil -c icns <directory> -o assets/katla-icon.icns`.

Validation covers embedded PNG decoding/alpha, window icon construction, ICNS
decoding in AppKit, the icon rendered by `NSWorkspace` from the bundle, bundle
metadata and a real windowed macOS launch.
