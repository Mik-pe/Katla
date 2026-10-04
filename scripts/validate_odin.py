#!/usr/bin/env python3
"""Validate the progressive Odin port and explicitly selected native GPU backends."""

from pathlib import Path
import argparse
import os
import platform
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-metal", action="store_true", help="Require the Metal 4 buffer-graph acceptance consumer on macOS arm64")
    parser.add_argument("--native-vulkan", action="store_true", help="Require Vulkan 1.3 with Khronos synchronization validation")
    parser.add_argument("--vulkan-library", help="Explicit Vulkan loader library path")
    parser.add_argument("--vulkan-icd", help="Explicit Vulkan ICD manifest path")
    parser.add_argument("--glslc", default="glslc", help="GLSL compiler for Vulkan acceptance shaders")
    args = parser.parse_args()
    if args.native_metal and (platform.system() != "Darwin" or platform.machine() != "arm64"):
        parser.error("--native-metal requires macOS arm64 hardware")
    if (args.vulkan_library or args.vulkan_icd) and not args.native_vulkan:
        parser.error("Vulkan loader/ICD options require --native-vulkan")
    (ROOT / "target").mkdir(exist_ok=True)
    commands = [
        ["odin", "test", "odin/editor", "-all-packages", "-out:target/odin-ecs-editor-tests", "-vet", "-strict-style"],
        ["odin", "test", "odin/app", "-all-packages", "-out:target/odin-app-tests", "-vet", "-strict-style"],
        ["odin", "test", "odin/gfx", "-out:target/odin-gfx-tests", "-vet", "-strict-style"],
        ["odin", "test", "odin/gfx/spirv", "-out:target/odin-spirv-tests", "-vet", "-strict-style"],
        ["odin", "run", "odin/examples/agent_scene", "-out:target/odin-agent-scene", "-vet", "-strict-style"],
        ["odin", "run", "odin/examples/agent_mailbox", "-out:target/odin-agent-mailbox", "-vet", "-strict-style"],
        ["odin", "run", "odin/examples/material_authoring", "-out:target/odin-material-authoring", "-vet", "-strict-style"],
        ["odin", "test", "odin/math", "-out:target/odin-math-tests", "-vet", "-strict-style"],
        ["odin", "test", "odin/icons", "-out:target/odin-icons-tests", "-vet", "-strict-style"],
        ["odin", "test", "odin/audio/dsp", "-out:target/odin-audio-dsp-tests", "-vet", "-strict-style"],
        ["odin", "run", "odin/examples/movement", "-out:target/odin-movement", "-vet", "-strict-style"],
        [sys.executable, "scripts/compare_math_port.py"],
        [sys.executable, "scripts/check_icon_port.py"],
        [sys.executable, "scripts/compare_audio_dsp.py"],
    ]
    for command in commands:
        print("Running:", " ".join(command), flush=True)
        subprocess.run(command, cwd=ROOT, check=True)

    if args.native_metal:
        subprocess.run(["odin", "build", "odin/gfx_native", "-out:target/odin-gfx-native", "-vet", "-strict-style"], cwd=ROOT, check=True)
        native_env = os.environ.copy()
        native_env.update(MTL_DEBUG_LAYER="1", METAL_DEVICE_WRAPPER_TYPE="1")
        subprocess.run([str(ROOT / "target/odin-gfx-native")], cwd=ROOT, env=native_env, check=True, timeout=60)

    if args.native_vulkan:
        binaries = []
        for shader in ("fill", "params"):
            binary = ROOT / f"target/odin-gfx-{shader}.spv"
            subprocess.run([args.glslc, "--target-env=vulkan1.3", str(ROOT / f"odin/gfx_native/shaders/{shader}.comp"), "-o", str(binary)], cwd=ROOT, check=True, timeout=60)
            binaries.append(str(binary))
        subprocess.run(["odin", "build", "odin/gfx_vulkan_native", "-out:target/odin-gfx-vulkan-native", "-vet", "-strict-style"], cwd=ROOT, check=True)
        native_env = os.environ.copy()
        if args.vulkan_icd:
            native_env["VK_ICD_FILENAMES"] = args.vulkan_icd
        if platform.system() == "Darwin":
            native_env.update(MTL_DEBUG_LAYER="1", METAL_DEVICE_WRAPPER_TYPE="1")
        command = [str(ROOT / "target/odin-gfx-vulkan-native"), *binaries]
        if args.vulkan_library:
            command.append(args.vulkan_library)
        subprocess.run(command, cwd=ROOT, env=native_env, check=True, timeout=60)


if __name__ == "__main__":
    main()
