#!/usr/bin/env python3
"""Build and launch the canonical Odin editor with explicit dependency paths."""
import argparse
import ctypes.util
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import subprocess
import sys

from build_katla_odin import ROOT, architecture, output_path


def verified_manifest(directory, sanitize):
    filename = directory / "build.json"
    if not filename.is_file():
        raise RuntimeError(f"No completed build manifest: {filename}; run build_katla_odin.py first")
    manifest = json.loads(filename.read_text())
    if (manifest.get("version") != 1 or manifest.get("root") != str(ROOT)
            or manifest.get("host") != platform.system() or manifest.get("architecture") != architecture()
            or manifest.get("sanitize") != sanitize):
        raise RuntimeError("Build manifest belongs to another checkout, host, architecture or sanitizer mode")
    if not manifest.get("executable"):
        raise RuntimeError("Manifest has no editor executable; a dependencies/objects-only build cannot launch the editor")
    for filename, digest in manifest["artifact_sha256"].items():
        path = Path(filename)
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise RuntimeError(f"Built artifact has changed or is missing: {path}; rebuild before launching")
    if not Path(manifest["executable"]).is_file():
        raise RuntimeError("Built editor executable is missing")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__, epilog="Pass application arguments after --, e.g. -- --frames 100 --scene scenes/demo.ron. Native --help lists the application's current flags.")
    parser.add_argument("--build-dir", type=Path)
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument("--no-build", action="store_true", help="Reuse only a verified completed build; source changes require a new build")
    parser.add_argument("--odin", default="odin")
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--backend", choices=("metal", "vulkan"), default="metal" if platform.system() == "Darwin" else "vulkan")
    parser.add_argument("--project", type=Path, default=ROOT)
    parser.add_argument("--resources", type=Path, default=ROOT / "resources")
    parser.add_argument("--vulkan-loader")
    parser.add_argument("--dry-run", action="store_true", help="Print the verified invocation without starting a device")
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    options = parser.parse_args()
    directory = (options.build_dir or output_path(options.sanitize)).resolve()
    if not options.no_build:
        build = [sys.executable, str(ROOT / "scripts/build_katla_odin.py"), "--output", str(directory), "--odin", options.odin, "--cargo", options.cargo]
        if options.sanitize:
            build.append("--sanitize")
        subprocess.run(build, cwd=ROOT, check=True)
    manifest = verified_manifest(directory, options.sanitize)
    paths = manifest["paths"]
    if not paths.get("SHADER_ROOT"):
        raise RuntimeError("Manifest has no shipped canonical shader sources; rebuild before launching")
    arguments = options.arguments
    if arguments[:1] == ["--"]:
        arguments = arguments[1:]
    command = [manifest["executable"], "--backend", options.backend,
        "--project", str(options.project.resolve()), "--resources", str(options.resources.resolve()),
        "--shader-compiler", paths["SHADER_COMPILER"], "--shader-root", paths["SHADER_ROOT"], "--font-library", paths["FONT_LIBRARY"],
        "--luau-library", paths["LUAU_LIBRARY"], "--box-library", paths["BOX3D_LIBRARY"]]
    if "WINDOW_LIBRARY" in paths:
        command += ["--window-library", paths["WINDOW_LIBRARY"]]
    if options.backend == "vulkan":
        loader = options.vulkan_loader or os.environ.get("KATLA_VULKAN_LIBRARY") or ctypes.util.find_library("vulkan")
        if not loader:
            raise RuntimeError("Vulkan loader unavailable; pass --vulkan-loader explicitly")
        command += ["--vulkan-loader", loader]
    command += arguments
    print("+ " + shlex.join(command), flush=True)
    if options.dry_run:
        return 0
    environment = os.environ.copy()
    if platform.system() == "Windows" and "WINDOW_LIBRARY" in paths:
        environment["PATH"] = str(Path(paths["WINDOW_LIBRARY"]).parent) + os.pathsep + environment.get("PATH", "")
    if options.backend == "metal":
        environment["MTL_DEBUG_LAYER"] = "1"
        environment["METAL_DEVICE_WRAPPER_TYPE"] = "1"
    return subprocess.call(command, cwd=ROOT, env=environment)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (RuntimeError, ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"Katla launch failed: {error}", file=sys.stderr)
        raise SystemExit(1)
