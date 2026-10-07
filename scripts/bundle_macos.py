#!/usr/bin/env python3
"""Assemble an unsigned local Katla.app with the canonical desktop icon."""

import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile


def compile_icon(root, resources):
    with tempfile.TemporaryDirectory(prefix="katla-icons-") as scratch:
        scratch = Path(scratch)
        catalog = scratch / "Assets.xcassets"
        icons = catalog / "AppIcon.appiconset"
        icons.mkdir(parents=True)
        info = {"version": 1, "author": "xcode"}
        images = []
        for size in (16, 32, 128, 256, 512):
            for scale in (1, 2):
                filename = f"icon_{size}x{size}" + ("@2x" if scale == 2 else "") + ".png"
                pixels = str(size * scale)
                subprocess.run([
                    "sips", "-z", pixels, pixels, str(root / "assets/katla-icon.png"),
                    "--out", str(icons / filename),
                ], check=True, stdout=subprocess.DEVNULL)
                images.append({
                    "idiom": "mac", "size": f"{size}x{size}",
                    "scale": f"{scale}x", "filename": filename,
                })
        (catalog / "Contents.json").write_text(json.dumps({"info": info}))
        (icons / "Contents.json").write_text(json.dumps({"images": images, "info": info}))
        icon_metadata = scratch / "icon-info.plist"
        subprocess.run([
            "xcrun", "actool", str(catalog), "--compile", str(resources),
            "--platform", "macosx", "--minimum-deployment-target", "26.0",
            "--app-icon", "AppIcon", "--output-partial-info-plist", str(icon_metadata),
        ], check=True, stdout=subprocess.DEVNULL)
        return plistlib.loads(icon_metadata.read_bytes())


def main():
    if sys.platform != "darwin":
        raise SystemExit("Katla.app must be assembled on macOS")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--skip-build", action="store_true", help="package an existing binary")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    cargo = os.environ.get("CARGO", "cargo")
    cargo_metadata = json.loads(subprocess.check_output(
        [cargo, "metadata", "--format-version", "1", "--no-deps", "--locked"], cwd=root
    ))
    target = Path(cargo_metadata["target_directory"])
    profile = "release" if args.release else "debug"
    if not args.skip_build:
        command = [cargo, "build", "-p", "game", "--bin", "katla", "--locked"]
        if args.release:
            command.append("--release")
        subprocess.run(command, cwd=root, check=True)
    binary = target / profile / "katla"
    if not binary.is_file():
        raise SystemExit(f"Missing binary: {binary}")
    bundle = root / "target" / "macos" / profile / "Katla.app"
    if bundle.exists():
        shutil.rmtree(bundle)
    contents = bundle / "Contents"
    macos = contents / "MacOS"
    resources = contents / "Resources"
    macos.mkdir(parents=True, exist_ok=True)
    resources.mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, macos / "katla-bin")
    icon_metadata = compile_icon(root, resources)
    shutil.copytree(root / "resources", resources / "resources", dirs_exist_ok=True)
    shutil.copytree(root / "assets/scenes", resources / "scenes", dirs_exist_ok=True)
    with (root / "packaging/macos/Info.plist").open("rb") as source:
        metadata = plistlib.load(source)
    metadata.update(icon_metadata)
    metadata["CFBundleShortVersionString"] = next(
        package["version"] for package in cargo_metadata["packages"]
        if package["name"] == "game"
    )
    with (contents / "Info.plist").open("wb") as destination:
        plistlib.dump(metadata, destination)
    launcher = macos / "Katla"
    launcher.write_text(
        '#!/bin/sh\nset -eu\n'
        'katla_macos_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\n'
        'export KATLA_RESOURCES_PATH="$katla_macos_dir/../Resources/resources"\n'
        'cd "$katla_macos_dir/../Resources"\n'
        'exec "$katla_macos_dir/katla-bin" "$@"\n'
    )
    launcher.chmod(0o755)
    print(bundle)


if __name__ == "__main__":
    main()
