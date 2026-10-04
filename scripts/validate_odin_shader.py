#!/usr/bin/env python3
"""Build the pinned compiler dependency and validate actual Odin WGSL/reflection/replacement calls."""

import argparse
import os
from pathlib import Path
import platform
import subprocess

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "tools/naga_bridge/Cargo.toml"


def run(command, **kwargs):
    print("Running:", " ".join(map(str, command)), flush=True)
    subprocess.run(list(map(str, command)), cwd=ROOT, check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target/odin-naga-compiler")
    parser.add_argument("--sanitize", action="store_true", help="Also run Odin ownership checks with AddressSanitizer")
    parser.add_argument("--native", action="store_true", help="Verify actual WGSL replacement rendering through Metal and Vulkan on Darwin arm64")
    parser.add_argument("--native-leaks", action="store_true", help="Also audit process-exit leaks in Apple/driver libraries during native ASan acceptance")
    parser.add_argument("--vulkan-library", type=Path, help="Explicit Vulkan loader library for native acceptance")
    parser.add_argument("--vulkan-icd", type=Path, help="Explicit Vulkan ICD manifest for native acceptance")
    args = parser.parse_args()
    if args.native_leaks and not (args.native and args.sanitize):
        parser.error("--native-leaks requires --native --sanitize")
    system = platform.system()
    if args.native:
        if system != "Darwin" or platform.machine() != "arm64":
            parser.error("The dual-backend native shader consumer requires Darwin arm64 with Metal 4")
        if not args.vulkan_library or not args.vulkan_icd:
            parser.error("--native requires --vulkan-library and --vulkan-icd")
        if not args.vulkan_library.is_file() or not args.vulkan_icd.is_file():
            parser.error("The explicit Vulkan loader and ICD paths must exist")
    target = args.target_dir.resolve()
    cargo = ["cargo", "--locked", "--manifest-path", MANIFEST, "--target-dir", target]
    # Per-helper output avoids the shared Rust workspace/runner build locks.
    run(["cargo", "fmt", "--manifest-path", MANIFEST, "--check"])
    for operation in ("test", "clippy", "build"):
        command = [cargo[0], operation, *cargo[1:]]
        if operation == "clippy":
            command += ["--all-targets", "--", "-D", "warnings"]
        run(command)
    name = {"Darwin": "katla-shader-compiler", "Linux": "katla-shader-compiler", "Windows": "katla-shader-compiler.exe"}.get(system)
    if name is None:
        parser.error(f"Unsupported native compiler executable platform: {system}")
    library = target / "debug" / name
    if not library.is_file():
        raise RuntimeError(f"Compiler build did not produce {library}")
    output = ROOT / "target/odin-shader-tests"
    output.parent.mkdir(exist_ok=True)
    command = ["odin", "test", "odin/gfx/shader_tests", "-all-packages", f"-define:SHADER_COMPILER={library}", f"-out:{output}", "-vet", "-strict-style", "-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true"]
    run(command)
    if args.sanitize:
        if system == "Windows":
            parser.error("AddressSanitizer acceptance requires a supported Odin sanitizer target")
        native_env = os.environ.copy()
        native_env["ASAN_OPTIONS"] = "detect_leaks=1:abort_on_error=1"
        run([*command, "-debug", "-sanitize:address"], env=native_env)
    if args.native:
        native_env = os.environ.copy()
        native_env.update(MTL_DEBUG_LAYER="1", METAL_DEVICE_WRAPPER_TYPE="1", VK_ICD_FILENAMES=str(args.vulkan_icd.resolve()))
        native_output = ROOT / "target/odin-gfx-shader-native"
        native_command = ["odin", "build", "odin/gfx_shader_native", f"-out:{native_output}", "-vet", "-strict-style"]
        run(native_command)
        run([native_output, library, args.vulkan_library.resolve()], env=native_env)
        if args.sanitize:
            # Native ownership is checked by the Odin tracker. Apple's compiler/driver
            # worker threads retain process-global TLS/class-init allocations at exit.
            # The separate opt-in preserves that broader external-library audit.
            native_env["ASAN_OPTIONS"] = f"detect_leaks={int(args.native_leaks)}:abort_on_error=1"
            print("Native ASan: address checks and exact Odin ownership tracking; process-exit external-library leak audit:", args.native_leaks, flush=True)
            run([*native_command, "-debug", "-sanitize:address"])
            run([native_output, library, args.vulkan_library.resolve()], env=native_env)
    print(f"Verified compiler dependency: {library}", flush=True)


if __name__ == "__main__":
    main()
