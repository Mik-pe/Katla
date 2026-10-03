#!/usr/bin/env python3
"""Run the progressive Odin port's native CPU acceptance checks."""

from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    (ROOT / "target").mkdir(exist_ok=True)
    commands = [
        ["odin", "test", "odin/editor", "-all-packages", "-out:target/odin-ecs-editor-tests", "-vet", "-strict-style"],
        ["odin", "test", "odin/math", "-out:target/odin-math-tests", "-vet", "-strict-style"],
        ["odin", "run", "odin/examples/movement", "-out:target/odin-movement", "-vet", "-strict-style"],
        [sys.executable, "scripts/compare_math_port.py"],
    ]
    for command in commands:
        print("Running:", " ".join(command), flush=True)
        subprocess.run(command, cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
