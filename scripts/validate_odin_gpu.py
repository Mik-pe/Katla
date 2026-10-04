#!/usr/bin/env python3
"""Validate the canonical Odin GPU contracts and explicitly selected native backends."""

import argparse
import os
from pathlib import Path
import platform
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "tools/naga_bridge/Cargo.toml"


def run(command, *, env=None, timeout=600):
    arguments = list(map(str, command))
    print("Running:", shlex.join(arguments), flush=True)
    subprocess.run(arguments, cwd=ROOT, env=env, check=True, timeout=timeout)


def asan_environment(environment, leaks):
    result = environment.copy()
    options = [option for option in result.get("ASAN_OPTIONS", "").split(":")
               if option and not option.startswith(("detect_leaks=", "abort_on_error="))]
    result["ASAN_OPTIONS"] = ":".join([*options, f"detect_leaks={int(leaks)}", "abort_on_error=1"])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-metal", action="store_true", help="Require Metal4/Tier2 native tests and canonical material arrays")
    parser.add_argument("--native-vulkan", action="store_true", help="Require Vulkan1.3 and Khronos synchronization validation")
    parser.add_argument("--native-surface", action="store_true", help="Include the Cocoa Vulkan acquire/resize/present fixture")
    parser.add_argument("--sanitize", action="store_true", help="Instrument every Odin test/native executable with AddressSanitizer")
    parser.add_argument("--skip-shader-checks", action="store_true", help="Reuse an already verified Naga library; omit Cargo and shader CPU checks")
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target/odin-gpu-naga", help="Isolated Naga Cargo target directory")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/odin-gpu-validation")
    parser.add_argument("--odin", default="odin")
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--glslc", default="glslc")
    parser.add_argument("--vulkan-library", type=Path, help="Explicit native Vulkan loader library")
    parser.add_argument("--vulkan-icd", type=Path, help="Explicit native Vulkan ICD manifest")
    args = parser.parse_args()
    system = platform.system()
    apple = system == "Darwin" and platform.machine() == "arm64"
    if args.native_metal and not apple:
        parser.error("--native-metal requires macOS arm64 with a real Metal4 device")
    if args.native_surface and (not args.native_vulkan or not apple):
        parser.error("--native-surface requires --native-vulkan and a macOS arm64 Cocoa session")
    if (args.vulkan_library or args.vulkan_icd) and not args.native_vulkan:
        parser.error("Vulkan loader/ICD options require --native-vulkan")
    if args.sanitize and system == "Windows":
        parser.error("This Odin AddressSanitizer acceptance requires Darwin or Linux")
    for option in (args.vulkan_library, args.vulkan_icd):
        if option and not option.is_file():
            parser.error(f"Explicit Vulkan path does not exist: {option}")
    library_name = {"Darwin": "libkatla_naga_compiler.dylib", "Linux": "libkatla_naga_compiler.so", "Windows": "katla_naga_compiler.dll"}.get(system)
    if library_name is None:
        parser.error(f"Unsupported Naga library platform: {system}")
    target = args.target_dir.resolve()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    library = target / "debug" / library_name
    if args.skip_shader_checks:
        if not library.is_file():
            parser.error(f"--skip-shader-checks requires an already built compiler: {library}")
        print("Explicitly reusing verified shader checks:", library, flush=True)
    else:
        run([args.cargo, "fmt", "--manifest-path", MANIFEST, "--check"])
        for operation in ("test", "clippy", "build"):
            command = [args.cargo, operation, "--manifest-path", MANIFEST, "--locked", "--target-dir", target]
            if operation == "clippy":
                command.extend(["--all-targets", "--", "-D", "warnings"])
            run(command)
        if not library.is_file():
            raise RuntimeError(f"Naga build did not produce {library}")

    extension = ".exe" if system == "Windows" else ""
    flags = ["-vet", "-strict-style"]
    if args.sanitize:
        flags.extend(["-debug", "-sanitize:address"])
    cpu_env = asan_environment(os.environ, True) if args.sanitize else os.environ.copy()
    gpu_env = os.environ.copy()
    if apple:
        gpu_env.update(MTL_DEBUG_LAYER="1", METAL_DEVICE_WRAPPER_TYPE="1")
    if args.vulkan_icd:
        gpu_env["VK_ICD_FILENAMES"] = str(args.vulkan_icd.resolve())
    if args.sanitize:
        gpu_env = asan_environment(gpu_env, False)
        print("CPU ASan retains leak checks. Native GPU executable launches disable only process-exit external CF/ObjC/driver leak detection; address checks and Odin ownership trackers remain enabled.", flush=True)

    def build(package, name, *, tests=False, extra=()):
        executable = output / (name + extension)
        command = [args.odin, "build", package, f"-out:{executable}", *flags]
        if tests:
            command.extend(["-build-mode:test", "-all-packages", "-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true"])
        run([*command, *extra])
        return executable

    for package, name in (("odin/gfx", "core-tests"), ("odin/gfx/spirv", "spirv-tests")):
        executable = build(package, name, tests=True)
        run([executable], env=cpu_env, timeout=60)
    if not args.skip_shader_checks:
        executable = build("odin/gfx/shader_tests", "shader-tests", tests=True, extra=[f"-define:NAGA_LIBRARY={library}"])
        run([executable], env=cpu_env, timeout=60)

    if args.native_metal:
        executable = build("odin/gfx/metal", "metal-tests", tests=True)
        run([executable], env=gpu_env, timeout=60)
        executable = build("odin/gfx_native", "metal-native")
        run([executable, library], env=gpu_env, timeout=60)

    default_loader = {"Darwin": "libvulkan.dylib", "Windows": "vulkan-1.dll"}.get(system, "libvulkan.so.1")
    loader = args.vulkan_library.resolve() if args.vulkan_library else default_loader
    if args.native_vulkan:
        binaries = {}
        sources = {
            "fill": "odin/gfx_native/shaders/fill.comp",
            "params": "odin/gfx_native/shaders/params.comp",
            **{name: f"odin/gfx_vulkan_native/shaders/{name}.{stage}" for name, stage in
               (("triangle", "vert"), ("color", "frag"), ("mesh", "vert"), ("tint", "frag"),
                ("sample", "frag"), ("image", "comp"), ("volume", "comp"))},
        }
        for name, source in sources.items():
            binary = output / f"{name}.spv"
            run([args.glslc, "--target-env=vulkan1.3", ROOT / source, "-o", binary], timeout=60)
            binaries[name] = binary
        for name, stage in (("array", "Fragment"), ("storage_array", "Compute")):
            basename = output / name
            run([sys.executable, ROOT / "scripts/compile_odin_shader.py", ROOT / f"odin/gfx_shader_native/shaders/{name}.wgsl",
                 "--library", library, "--entry", "main", "--stage", stage, "--output", basename], timeout=60)
            binaries[name] = basename.with_suffix(".spv")
        executable = build("odin/gfx_vulkan_native", "vulkan-native")
        command = [executable, *(binaries[name] for name in ("fill", "params", "triangle", "color")), loader,
                   "--volume", binaries["volume"], "--images", binaries["sample"], binaries["image"],
                   "--mesh", binaries["mesh"], binaries["tint"], "--arrays", binaries["array"],
                   "--storage-arrays", binaries["storage_array"]]
        if args.native_surface:
            command.append("--surface")
        run(command, env=gpu_env, timeout=60)
    if args.native_metal and args.native_vulkan:
        executable = build("odin/gfx_shader_native", "shader-native-reload")
        run([executable, library, loader], env=gpu_env, timeout=60)
    print("Odin GPU validation passed for the explicitly selected checks and backends.", flush=True)


if __name__ == "__main__":
    main()
