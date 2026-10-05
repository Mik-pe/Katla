# Native window dependency

The Linux/Windows editor uses the direct C ABI in `bridge.h`, backed by SDL
3.2.28 at commit `7f3ae3d57459e59943a4ecfefc8f6277ec6bf540`.
The builder fetches and verifies that exact clean Git revision; it never selects a system
SDL installation. SDL's zlib license is retained in the output as
`SDL-LICENSE.txt`. The bridge has no Rust dependency.

```sh
odin run tools/build -- --dependency window --output target/window-native
```

Set `CC`, `CXX` and `AR` to select the native compiler and archiver. Use
`--sanitize` for matching AddressSanitizer instrumentation. The Odin tool
compiles the pinned source directly with checked-in host configurations;
Python, CMake and Ninja are not build dependencies. Pass the produced library
to the editor's `--window-library` argument. The SDL runtime library must remain
beside the bridge. Output names are `libkatla_window_native.so`,
or `katla_window_native.dll`. macOS uses Katla's Cocoa window backend.

Linux requires development packages for **both** X11 and Wayland. On Debian/
Ubuntu these include `libx11-dev libxext-dev libxrandr-dev libxcursor-dev
libxfixes-dev libxi-dev libxss-dev libwayland-dev libxkbcommon-dev
libdbus-1-dev libibus-1.0-dev libegl1-mesa-dev pkg-config`.
The source build fails if either native video driver or the D-Bus/IBus/Fcitx
IME paths were omitted. Wayland protocol sources come from the pinned SDL tree
and are generated with `wayland-scanner` from `libwayland-dev`.

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

Strict full-editor checks for Linux and Windows verify their typed platform
boundaries. Source linking requires the target SDK and libraries; window/input
acceptance additionally requires a native desktop or Xvfb with a Vulkan ICD.
Configured CPU ASan suites retain leak detection.
