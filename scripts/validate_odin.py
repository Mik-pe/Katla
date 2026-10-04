#!/usr/bin/env python3
"""Run the progressive Odin port's native CPU acceptance checks."""

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
    args = parser.parse_args()
    if args.native_metal and (platform.system() != "Darwin" or platform.machine() != "arm64"):
        parser.error("--native-metal requires macOS arm64 hardware")
    (ROOT / "target").mkdir(exist_ok=True)
    commands = [
        ["odin", "test", "odin/editor", "-all-packages", "-out:target/odin-ecs-editor-tests", "-vet", "-strict-style"],
        ["odin", "test", "odin/agent", "-all-packages", "-out:target/odin-agent-tests", "-vet", "-strict-style"],
        ["odin", "test", "odin/gfx", "-out:target/odin-gfx-tests", "-vet", "-strict-style"],
        ["odin", "run", "odin/examples/agent_scene", "-out:target/odin-agent-scene", "-vet", "-strict-style"],
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


if __name__ == "__main__":
    main()
