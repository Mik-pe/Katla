#!/usr/bin/env python3
"""Build the repository-pinned C99 cgltf parser for the native Odin target."""

import argparse
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def build(output: Path, sanitize: bool = False) -> Path:
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    compiler = shlex.split(os.environ.get("CC", "clang" if platform.system() == "Windows" else "cc"))
    archiver = shlex.split(os.environ.get("AR", "llvm-ar" if platform.system() == "Windows" else "ar"))
    for command in (compiler, archiver):
        if not command or shutil.which(command[0]) is None:
            raise RuntimeError(f"Required native C compiler/archive tool unavailable: {command}")
    obj = output / "cgltf.o"
    library = output / "libcgltf.a"
    command = [*compiler, "-std=c99", "-O2", "-fPIC", "-Wall", "-Wextra", "-Werror", "-c", str(ROOT / "tools/cgltf/cgltf.c"), "-o", str(obj)]
    if sanitize:
        command += ["-fsanitize=address", "-fno-omit-frame-pointer", "-g"]
    subprocess.run(command, cwd=ROOT, check=True)
    subprocess.run([*archiver, "rcs", str(library), str(obj)], cwd=ROOT, check=True)
    return library


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--sanitize", action="store_true", help="Instrument the C parser; link consumers with Odin -sanitize:address")
    args = parser.parse_args()
    output = args.output or ROOT / ("target/odin-cgltf-asan" if args.sanitize else "target/odin-cgltf")
    library = build(output, args.sanitize)
    print(f"Built pinned glTF parser: {library}")
    relative = os.path.relpath(library, ROOT / "odin/deps/cgltf").replace(os.sep, "/")
    print(f"Odin override for another output location: -define:CGLTF_LIBRARY={relative}")


if __name__ == "__main__":
    main()
