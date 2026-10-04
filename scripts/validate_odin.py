#!/usr/bin/env python3
"""Build the canonical Odin editor and validate its owners and selected native adapters."""

from pathlib import Path
import argparse
import os
import platform
import subprocess
import sys

from build_katla_odin import cpu_test_environment, output_path
from odin_validation_manifest import validation_manifest

ROOT = Path(__file__).resolve().parents[1]


def run(arguments, environment=None):
    print("Running:", " ".join(map(str, arguments)), flush=True)
    subprocess.run(list(map(str, arguments)), cwd=ROOT, env=environment, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-metal", action="store_true")
    parser.add_argument("--native-vulkan", action="store_true")
    parser.add_argument("--native-physics", action="store_true", help="Require source-pinned Box3D and direct Luau native acceptance")
    parser.add_argument("--native-surface", action="store_true", help="Require windowed Vulkan acquire/resize/present acceptance")
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument("--build-manifest", type=Path, help="Reuse a verified canonical source build; omit dependency and editor rebuilds")
    parser.add_argument("--vulkan-library", type=Path)
    parser.add_argument("--vulkan-icd", type=Path)
    parser.add_argument("--glslc", default="glslc")
    args = parser.parse_args()
    if args.native_metal and (platform.system() != "Darwin" or platform.machine() != "arm64"):
        parser.error("--native-metal requires macOS arm64 hardware")
    if (args.vulkan_library or args.vulkan_icd) and not args.native_vulkan:
        parser.error("Vulkan loader/ICD options require --native-vulkan")
    if args.native_surface and not args.native_vulkan:
        parser.error("--native-surface requires --native-vulkan")
    instrument = ["--sanitize"] if args.sanitize else []
    manifest_path = args.build_manifest
    if manifest_path is None:
        run([sys.executable, "scripts/build_katla_odin.py", "--tests", *instrument])
        manifest_path = output_path(args.sanitize) / "build.json"
    manifest_path = manifest_path.resolve()
    manifest = validation_manifest(parser, manifest_path, args.sanitize, [])
    environment = cpu_test_environment(os.environ, manifest_path.parent, args.sanitize)
    mcp = Path(manifest["executable"]).parent / ("katla-mcp-stdio.exe" if platform.system() == "Windows" else "katla-mcp-stdio")
    if str(mcp) not in manifest["artifact_sha256"] or not mcp.is_file():
        parser.error("Build manifest does not verify its canonical MCP stdio executable; rebuild")
    # These exercise actual protocol/process boundaries in addition to the package suites.
    for driver in ("validate_odin_mcp.py", "validate_odin_llm.py", "validate_odin_assistant.py",
                   "validate_odin_assistant_scene.py"):
        flags = [*instrument, "--build-manifest", manifest_path] if driver != "validate_odin_mcp.py" else ["--binary", mcp]
        run([sys.executable, ROOT / "scripts" / driver, *flags], environment)
    run([sys.executable, ROOT / "scripts/validate_odin_mcp_socket.py", *instrument,
         "--build-manifest", manifest_path], environment)
    for driver in ("validate_odin_host.py", "validate_odin_mcp_proxy.py"):
        flags = [*instrument, "--slow"] if driver == "validate_odin_mcp_proxy.py" else instrument
        run([sys.executable, ROOT / "scripts" / driver, *flags], environment)
    if args.native_physics:
        for driver in ("validate_odin_box3d.py", "validate_odin_luau.py"):
            run([sys.executable, ROOT / "scripts" / driver])
    if args.native_metal or args.native_vulkan:
        arguments = [sys.executable, "scripts/validate_odin_gpu.py", "--glslc", args.glslc,
                     "--target-dir", Path(manifest["paths"]["SHADER_COMPILER"]).parent.parent, *instrument]
        for enabled, flag in ((args.native_metal, "--native-metal"), (args.native_vulkan, "--native-vulkan"),
                              (args.native_surface, "--native-surface")):
            if enabled:
                arguments.append(flag)
        for value, flag in ((args.vulkan_library, "--vulkan-library"), (args.vulkan_icd, "--vulkan-icd")):
            if value:
                arguments.extend([flag, value.resolve()])
        run(arguments)


if __name__ == "__main__":
    main()
