# Native window dependency

The Linux/Windows editor uses the direct C ABI in `bridge.h`, backed by SDL
3.2.28 at commit `7f3ae3d57459e59943a4ecfefc8f6277ec6bf540`. The source archive
SHA-256 is `7a8347c770b90b33daac2352858ca03f9c9a2ccc8ce711054870361d1a6b32e5`.
The builder downloads and checks that exact source; it never selects a system
SDL installation. SDL's zlib license is retained in the output as
`SDL-LICENSE.txt`. The bridge has no Rust dependency.

```sh
python3 tools/window_native/build.py --output target/window-native --build-smoke
```

Use `--cc` to select the compiler, `--cmake` for the CMake executable, and
`--sanitize` for matching AddressSanitizer instrumentation. Windows defaults
to the Ninja generator; `--generator` overrides it. Pass the produced library
to the editor's `--window-library` argument. The SDL runtime library must remain
beside the bridge. Output names are `libkatla_window_native.so`,
`katla_window_native.dll`, or `libkatla_window_native.dylib`.

Linux requires development packages for **both** X11 and Wayland. On Debian/
Ubuntu these include `libx11-dev libxext-dev libxrandr-dev libxcursor-dev
libxfixes-dev libxi-dev libxss-dev libwayland-dev libxkbcommon-dev
wayland-protocols libdbus-1-dev libibus-1.0-dev pkg-config cmake ninja-build`.
The source build fails if either native video driver or the D-Bus/IBus/Fcitx
IME paths were omitted. Optional
`libdecor-0-dev` enables decorated Wayland windows on applicable compositors.

The bridge creates Vulkan-capable, resizable windows and exposes borrowed
Xlib, Wayland or Win32 native handles. The generic GPU surface descriptor
identifies the native kind explicitly; the application owns input and editor
policy. Native window events are dispatched into independent per-window owned
queues, so polling one window preserves the others' text and close events.
Destroy all windows before unloading the bridge.

Native clipboard UTF-8 bytes are borrowed until the next clipboard read. Event
text is borrowed until the next poll of that window. The Odin input owner copies
both into its own frame storage. SDL text-editing cursor/selection indices count
Unicode scalars; the Odin adapter converts them to UTF-8 byte offsets before
producing UI preedit. Text commits are actual native text events, separate from
physical key events. IME activation and candidate rectangles are passed to SDL.
See [SDL text editing](https://wiki.libsdl.org/SDL3/SDL_TextEditingEvent) and
[text input areas](https://wiki.libsdl.org/SDL3/SDL_SetTextInputArea).

Window state reports physical drawable size, display scale and pixel density.
Pointer/IME coordinates convert between SDL window coordinates and editor logical
coordinates using display scale divided by pixel density. This covers the
different Windows/X11 and Wayland coordinate conventions described by
[SDL high DPI](https://wiki.libsdl.org/SDL3/README-highdpi). Held keys use the
same normalized UI key set as the existing Darwin implementation. The existing
Darwin window/input backend remains canonical on macOS.

`--build-smoke` builds `katla_window_smoke` without launching it. Run it with an
explicit Vulkan loader path, for example:

```sh
target/window-native/katla_window_smoke /usr/lib/x86_64-linux-gnu/libvulkan.so.1
```

The smoke opens real windows, resizes, obtains native handles, saves/restores the
native clipboard and sets the IME candidate area. Injected SDL editing/commit
events prove owned UTF-8 queues and independent window lifetimes; they do **not**
prove operating-system IME interaction. It asserts zero SDL-owned allocations
after final shutdown. An unavailable native display/Vulkan loader returns 77;
that is unavailable hardware, not a passing acceptance run.

Validation on the implementation host includes strict full-editor type checks
for Darwin, Linux amd64 and Windows amd64, plus UTF-8/key CPU tests with ASan and
LeakSanitizer. Actual Linux/Windows source linking, IME, clipboard and input
interaction require their native CI/desktop hosts. The Cocoa dependency smoke
is an additional source/ABI/lifecycle check. Its AddressSanitizer run uses an
explicit external AppKit/QuartzCore/RunningBoard process-global leak boundary;
the zero SDL allocation assertion remains active. Font and ordinary CPU tests
retain full leak detection.
