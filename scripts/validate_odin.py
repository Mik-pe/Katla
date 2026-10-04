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
    parser.add_argument("--native-metal", action="store_true", help="Require native Metal graphics, memory, image and surface acceptance")
    parser.add_argument("--native-vulkan", action="store_true", help="Require Vulkan 1.3 with Khronos synchronization validation")
    parser.add_argument("--native-physics", action="store_true", help="Require real Rapier/Luau and Box3D dependency execution")
    parser.add_argument("--native-surface", action="store_true", help="Require windowed Vulkan acquire/resize/present acceptance")
    parser.add_argument("--sanitize", action="store_true", help="Run Odin address checks and provider service acceptance")
    parser.add_argument("--vulkan-library", help="Explicit Vulkan loader library path")
    parser.add_argument("--vulkan-icd", help="Explicit Vulkan ICD manifest path")
    parser.add_argument("--glslc", default="glslc", help="GLSL compiler for Vulkan acceptance shaders")
    args = parser.parse_args()
    if args.native_metal and (platform.system() != "Darwin" or platform.machine() != "arm64"):
        parser.error("--native-metal requires macOS arm64 hardware")
    if (args.vulkan_library or args.vulkan_icd) and not args.native_vulkan:
        parser.error("Vulkan loader/ICD options require --native-vulkan")
    if args.native_surface and not args.native_vulkan:
        parser.error("--native-surface requires --native-vulkan")
    (ROOT / "target").mkdir(exist_ok=True)
    commands = [
        [sys.executable, "scripts/build_odin_gltf.py"],
        [sys.executable, "scripts/build_odin_image.py"],
        ["odin", "test", "odin/editor", "-all-packages", "-out:target/odin-ecs-editor-tests", "-vet", "-strict-style"],
        ["odin", "test", "odin/app", "-all-packages", "-out:target/odin-app-tests", "-vet", "-strict-style"],
        ["odin", "test", "odin/app/render", "-all-packages", "-out:target/odin-render-tests", "-vet", "-strict-style"],
        ["odin", "test", "odin/agent/mcp", "-all-packages", "-out:target/odin-mcp-tests", "-vet", "-strict-style"],
        ["odin", "build", "odin/mcp_stdio", "-out:target/katla-odin-mcp", "-vet", "-strict-style"],
        [sys.executable, "scripts/validate_odin_mcp.py"],
        [sys.executable, "scripts/validate_odin_llm.py", *( ["--sanitize"] if args.sanitize else [] )],
        [sys.executable, "scripts/validate_odin_assistant.py", *( ["--sanitize"] if args.sanitize else [] )],
        [sys.executable, "scripts/validate_odin_assistant_scene.py", *( ["--sanitize"] if args.sanitize else [] )],
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
    if args.sanitize:
        from build_box3d import compiler
        native_env = os.environ.copy()
        native_env["CC"] = compiler(True)
        for builder in ("scripts/build_odin_gltf.py", "scripts/build_odin_image.py"):
            subprocess.run([sys.executable, builder, "--sanitize"], cwd=ROOT, env=native_env, check=True)
    for command in commands:
        if args.sanitize and command[0]=="odin":
            command.extend(["-sanitize:address", "-define:CGLTF_LIBRARY=../../../target/odin-cgltf-asan/libcgltf.a", "-define:STB_IMAGE_LIBRARY=../../../target/odin-stb-image-asan/libkatla_image.a"])
        print("Running:", " ".join(command), flush=True)
        subprocess.run(command, cwd=ROOT, check=True)

    if args.native_physics:
        subprocess.run([sys.executable, "scripts/validate_odin_box3d.py"], cwd=ROOT, check=True)
        subprocess.run(["cargo", "test", "--manifest-path", "tools/scene-runtime/Cargo.toml", "--locked"], cwd=ROOT, check=True)
        subprocess.run(["cargo", "build", "--manifest-path", "tools/scene-runtime/Cargo.toml", "--locked"], cwd=ROOT, check=True)
        metadata = subprocess.check_output(["cargo", "metadata", "--manifest-path", "tools/scene-runtime/Cargo.toml", "--format-version", "1", "--no-deps"], cwd=ROOT, text=True)
        import json
        library = Path(json.loads(metadata)["target_directory"]) / "debug" / ("libkatla_odin_scene_runtime.dylib" if platform.system()=="Darwin" else "libkatla_odin_scene_runtime.so")
        subprocess.run(["odin", "run", "odin/examples/scene_runtime", "-vet", "-strict-style", "-out:target/odin-scene-runtime", *( ["-sanitize:address"] if args.sanitize else [] ), "--", str(library), str(ROOT), str(ROOT / "resources")], cwd=ROOT, check=True)

    if args.native_metal or args.native_vulkan:
        command = [sys.executable, "scripts/validate_odin_gpu.py", "--glslc", args.glslc]
        for enabled, flag in ((args.native_metal, "--native-metal"), (args.native_vulkan, "--native-vulkan"),
                              (args.native_surface, "--native-surface"), (args.sanitize, "--sanitize")):
            if enabled:
                command.append(flag)
        for value, flag in ((args.vulkan_library, "--vulkan-library"), (args.vulkan_icd, "--vulkan-icd")):
            if value:
                command.extend([flag, value])
        subprocess.run(command, cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
