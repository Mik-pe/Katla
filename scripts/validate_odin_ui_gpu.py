#!/usr/bin/env python3
"""Validate source-pinned fonts and actual paired native UI/picking output."""
import argparse
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import tempfile

from build_katla_odin import cpu_test_environment, foreign_defines
from odin_validation_manifest import validation_manifest

ROOT = Path(__file__).resolve().parents[1]


def run(command, env=None, timeout=600):
    print("+ " + shlex.join(map(str, command)), flush=True)
    subprocess.run(list(map(str, command)), cwd=ROOT, env=env, check=True, timeout=timeout)


def asan_environment(native_gpu=False):
    env = os.environ.copy()
    options = [part for part in env.get("ASAN_OPTIONS", "").split(":") if part and not part.startswith("detect_leaks=")]
    # GPU frameworks retain CF/ObjC process globals outside application ownership.
    # Font/core CPU acceptance keeps full LeakSanitizer enabled.
    env["ASAN_OPTIONS"] = ":".join([*options, "detect_leaks=" + ("0" if native_gpu else "1")])
    if native_gpu:
        env.update(MTL_DEBUG_LAYER="1", METAL_DEVICE_WRAPPER_TYPE="1")
    return env


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--odin", help="Odin compiler; defaults to the verified build compiler when a manifest is supplied")
    parser.add_argument("--build-manifest", type=Path, help="Reuse exact verified canonical build.json dependencies and sanitizer mode; skip dependency builds")
    parser.add_argument("--shader-compiler", type=Path)
    parser.add_argument("--font-library", type=Path, help="Use an already built library with pinned fallback data in its sibling fonts directory")
    parser.add_argument("--image-library",type=Path,help="Pinned image decoder static library, instrumented when --sanitize")
    parser.add_argument("--vulkan-loader", type=Path)
    parser.add_argument("--vulkan-icd", type=Path)
    parser.add_argument("--backend", choices=("metal", "vulkan", "both"), default="both")
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument("--cc", help="Native C compiler; sanitized libraries must use the same ASan runtime as Odin")
    parser.add_argument("--cxx", help="Native C++ compiler matching --cc")
    parser.add_argument("--cpu-only", action="store_true")
    parser.add_argument("--skip-font-checks", action="store_true")
    args = parser.parse_args()
    manifest = validation_manifest(parser, args.build_manifest, args.sanitize, [
        ("--font-library", "FONT_LIBRARY", args.font_library),
        ("--image-library", "STB_IMAGE_LIBRARY", args.image_library),
        ("--shader-compiler", "SHADER_COMPILER", args.shader_compiler),
    ])
    if manifest:
        if args.cc or args.cxx:
            parser.error("--cc/--cxx configure dependency builds and cannot accompany --build-manifest")
        args.font_library = Path(manifest["paths"]["FONT_LIBRARY"])
        args.image_library = Path(manifest["paths"]["STB_IMAGE_LIBRARY"])
        args.shader_compiler = Path(manifest["paths"]["SHADER_COMPILER"])
    args.odin = args.odin or (manifest["odin"] if manifest else shutil.which("odin") or "odin")
    with tempfile.TemporaryDirectory(prefix="katla-ui-gpu-") as temporary:
        output = Path(temporary)
        library = args.font_library
        if library is None:
            font_output = output / "font"
            command = ["python3", ROOT / "tools/font_native/build.py", "--output", font_output]
            if args.cc:
                command += ["--cc", args.cc]
            if args.cxx:
                command += ["--cxx", args.cxx]
            if args.sanitize:
                command.append("--sanitize")
            run(command)
            name = {"Darwin": "libkatla_font_native.dylib", "Linux": "libkatla_font_native.so", "Windows": "katla_font_native.dll"}[platform.system()]
            library = font_output / name
        library = library.resolve()
        if not library.is_file():
            parser.error(f"Font dependency is unavailable: {library}")
        flags = ["-vet", "-strict-style"]
        if args.sanitize:
            flags.append("-sanitize:address")
        if manifest:
            flags += foreign_defines(manifest["paths"])
        if not args.skip_font_checks:
            for package, extra in [("odin/gfx", []), ("odin/deps/font_native", [*([] if manifest else [f"-define:FONT_LIBRARY={library}"]), f"-define:FONT_RESOURCES={ROOT / 'resources'}"])]:
                executable = output / ("core-test" if package == "odin/gfx" else "font-test")
                run([args.odin, "build", package, "-build-mode:test", "-define:ODIN_TEST_THREADS=1", "-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true", *flags, *extra, f"-out:{executable}"])
                run([executable], cpu_test_environment(os.environ, output, args.sanitize))
        if args.cpu_only:
            return
        if platform.system() != "Darwin" or platform.machine() not in ("arm64", "aarch64"):
            parser.error("Native UI/picking acceptance harness requires macOS Apple Silicon; requested GPU hardware proof is unavailable on this host")
        if args.shader_compiler is None or not args.shader_compiler.is_file():
            parser.error("Pass --shader-compiler pointing to the built offline executable")
        if args.backend in ("vulkan", "both"):
            if args.vulkan_loader is None or not args.vulkan_loader.is_file():
                parser.error("Vulkan native validation requires an explicit --vulkan-loader")
            if args.vulkan_icd is None or not args.vulkan_icd.is_file():
                parser.error("Vulkan native validation requires an explicit --vulkan-icd")
        image_library=args.image_library or ROOT / "target" / ("odin-stb-image-asan" if args.sanitize else "odin-stb-image") / "libkatla_image.a"
        if not image_library.is_file():
            parser.error(f"Build the pinned BMP/TIFF decoder or pass --image-library: {image_library}")
        executable = output / "ui-picking-native"
        image_flags = [] if manifest else [f"-define:STB_IMAGE_LIBRARY={os.path.relpath(image_library.resolve(),ROOT / 'odin/deps/stb_image')}"]
        run([args.odin, "build", "odin/app_render_ui_native", *flags, *image_flags, f"-out:{executable}"])
        env = asan_environment(native_gpu=True)
        if args.vulkan_icd:
            env["VK_ICD_FILENAMES"] = str(args.vulkan_icd.resolve())
        run([executable, args.shader_compiler.resolve(), library, ROOT / "resources", args.vulkan_loader.resolve() if args.vulkan_loader else "", "--" + args.backend], env)
        thumbnails = output / "thumbnails-native"
        run([args.odin, "build", "odin/examples/thumbnails_native", *flags, *image_flags, f"-out:{thumbnails}"])
        run([thumbnails, args.shader_compiler.resolve(), library, ROOT / "resources", args.vulkan_loader.resolve() if args.vulkan_loader else "", "--" + args.backend], env)
    print("Native UI/picking and visible image thumbnail acceptance passed.", flush=True)


if __name__ == "__main__":
    main()
