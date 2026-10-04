#!/usr/bin/env python3
"""Build repository-pinned PNG/JPEG decoding for the native Odin target."""

import argparse
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def compiler_command(sanitize: bool) -> list[str]:
    if os.environ.get("CC"):
        return shlex.split(os.environ["CC"])
    if not sanitize:
        return ["clang" if platform.system() == "Windows" else "cc"]
    report = subprocess.check_output(["odin", "report"], text=True)
    match = re.search(r"Backend:\s+LLVM (\d+)", report)
    if not match:
        raise RuntimeError("Cannot identify Odin ASan runtime; set CC to its matching Clang")
    major = match.group(1)
    candidates = [shutil.which(f"clang-{major}"), shutil.which("clang")]
    if platform.system() == "Darwin" and shutil.which("brew"):
        prefix = subprocess.run(["brew", "--prefix", f"llvm@{major}"], capture_output=True, text=True)
        if prefix.returncode == 0:
            candidates.insert(0, str(Path(prefix.stdout.strip()) / "bin/clang"))
    for candidate in candidates:
        if candidate and Path(candidate).exists():
            version = subprocess.check_output([candidate, "--version"], text=True)
            if re.search(rf"(?<!Apple )clang version {major}\.", version):
                return [candidate]
    raise RuntimeError(f"ASan requires Clang {major} matching Odin; set CC explicitly")


def build(output: Path, sanitize: bool = False) -> Path:
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    compiler = compiler_command(sanitize)
    archiver = shlex.split(os.environ.get("AR", "llvm-ar" if platform.system() == "Windows" else "ar"))
    for command in (compiler, archiver):
        if not command or shutil.which(command[0]) is None:
            raise RuntimeError(f"Required native C compiler/archive tool unavailable: {command}")
    obj = output / "image.o"
    library = output / "libkatla_image.a"
    command = [*compiler, "-std=c11", "-O2", "-fPIC", "-Wall", "-Wextra", "-Wno-unused-function", "-Werror", "-c", str(ROOT / "tools/stb_image/image.c"), "-o", str(obj)]
    if sanitize:
        command += ["-fsanitize=address", "-fno-omit-frame-pointer", "-g"]
    subprocess.run(command, cwd=ROOT, check=True)
    subprocess.run([*archiver, "rcs", str(library), str(obj)], cwd=ROOT, check=True)
    return library


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--sanitize", action="store_true", help="Instrument native decoding; link with Odin -sanitize:address using a matching compiler runtime")
    args = parser.parse_args()
    output = args.output or ROOT / ("target/odin-stb-image-asan" if args.sanitize else "target/odin-stb-image")
    library = build(output, args.sanitize)
    print(f"Built pinned PNG/JPEG decoder: {library}")
    relative = os.path.relpath(library, ROOT / "odin/deps/stb_image").replace(os.sep, "/")
    print(f"Odin override for another output location: -define:STB_IMAGE_LIBRARY={relative}")


if __name__ == "__main__":
    main()
