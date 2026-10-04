#!/usr/bin/env python3
"""Build the repository-pinned TOML parser for the native Odin target."""

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
    from build_box3d import compiler as native_compiler
    compiler = shlex.split(native_compiler(sanitize))
    archiver = shlex.split(os.environ.get("AR", "llvm-ar" if platform.system() == "Windows" else "ar"))
    for command in (compiler, archiver):
        if not command or shutil.which(command[0]) is None:
            raise RuntimeError(f"Required native C compiler/archive tool unavailable: {command}")
    obj = output / "toml.o"
    library = output / "libtoml.a"
    position_flags = [] if platform.system() == "Windows" else ["-fPIC"]
    command = [*compiler, "-std=c17", "-O2", *position_flags, "-Wall", "-Wextra", "-Werror", "-c", str(ROOT / "tools/toml_native/katla_toml.c"), "-o", str(obj)]
    if sanitize:
        command += ["-fsanitize=address", "-fno-omit-frame-pointer", "-g"]
    subprocess.run(command, cwd=ROOT, check=True)
    parser_obj = output / "tomlc17.o"
    parser_command = [*compiler, "-std=c17", "-O2", *position_flags, "-Wall", "-Wextra", "-Werror", "-c", str(ROOT / "tools/toml_native/vendor/tomlc17.c"), "-o", str(parser_obj)]
    if sanitize:
        parser_command += ["-fsanitize=address", "-fno-omit-frame-pointer", "-g"]
    subprocess.run(parser_command, cwd=ROOT, check=True)
    subprocess.run([*archiver, "rcs", str(library), str(obj), str(parser_obj)], cwd=ROOT, check=True)
    return library


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--sanitize", action="store_true", help="Instrument the C parser; link consumers with Odin -sanitize:address")
    args = parser.parse_args()
    output = args.output or ROOT / ("target/odin-toml-asan" if args.sanitize else "target/odin-toml")
    library = build(output, args.sanitize)
    print(f"Built pinned TOML parser: {library}")
    relative = os.path.relpath(library, ROOT / "odin/deps/toml_native").replace(os.sep, "/")
    print(f"Odin override for another output location: -define:TOML_LIBRARY={relative}")


if __name__ == "__main__":
    main()
