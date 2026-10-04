#!/usr/bin/env python3
"""Build the pinned C17 Box3D dependency and layout-stable Odin accessors."""
from pathlib import Path
import argparse
import os
import platform
import re
import shutil
import subprocess
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1] / "tools/box3d"))
from planar_hulls import prepare as prepare_planar_hulls

ROOT = Path(__file__).resolve().parents[1]
REVISION = "8441b4a06d6d09dcfb0b0f704df4d847d1437b92"


def compiler(sanitize):
    if os.environ.get("CC"):
        return os.environ["CC"]
    if not sanitize:
        return "clang"
    report = subprocess.check_output(["odin", "report"], text=True)
    match = re.search(r"Backend:\s+LLVM (\d+)", report)
    if not match:
        raise SystemExit("Cannot identify Odin ASan runtime; set CC to its matching Clang")
    major = match.group(1)
    candidates = [shutil.which(f"clang-{major}"), shutil.which("clang")]
    if platform.system() == "Darwin" and shutil.which("brew"):
        prefix = subprocess.run(["brew", "--prefix", f"llvm@{major}"], capture_output=True, text=True)
        if prefix.returncode == 0:
            candidates.insert(0, str(Path(prefix.stdout.strip()) / "bin/clang"))
    for candidate in candidates:
        if not candidate or not Path(candidate).exists():
            continue
        version = subprocess.check_output([candidate, "--version"], text=True)
        if re.search(rf"(?<!Apple )clang version {major}\.", version):
            return candidate
    raise SystemExit(f"ASan requires Clang {major} matching Odin; set CC explicitly")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument("--output", type=Path, help="Explicit native artifact path for an isolated ABI validation")
    args = parser.parse_args()
    source = ROOT / "target/box3d-source"
    source.parent.mkdir(exist_ok=True)
    if not source.exists():
        subprocess.run(["git", "clone", "--depth", "1", "--branch", "v0.1.0",
                        "https://github.com/erincatto/box3d.git", str(source)], check=True)
    revision = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip()
    if revision != REVISION:
        raise SystemExit(f"Box3D dependency revision mismatch: {revision}")
    if subprocess.check_output(["git", "-C", str(source), "status", "--porcelain"], text=True).strip():
        raise SystemExit("Box3D dependency checkout must be clean")
    listing = (source / "src/CMakeLists.txt").read_text().split("set(BOX3D_SOURCE_FILES", 1)[1].split(")", 1)[0]
    sources = [str(source / "src" / name) for name in re.findall(r"\b[\w]+\.c\b", listing)]
    host = platform.system()
    suffix = {"Darwin": "dylib", "Linux": "so", "Windows": "dll"}.get(host)
    if suffix is None:
        raise SystemExit(f"Unsupported native Box3D build platform: {host}")
    output = ROOT / "target" / f"libkatla_box3d{'_asan' if args.sanitize else ''}.{suffix}"
    if args.output is not None:
        output = args.output.resolve()
        output.parent.mkdir(parents=True, exist_ok=True)
    adapted_hull = prepare_planar_hulls(source,output.parent / f"{output.stem}-source")
    sources = [str(adapted_hull) if Path(item).name == "hull.c" else item for item in sources]
    command = [compiler(args.sanitize), "-std=c17", "-O1" if args.sanitize else "-O2",
               *([] if host == "Windows" else ["-fPIC"]), "-ffp-contract=off", "-DBOX3D_VALIDATE",
               "-Dbox3d_EXPORTS", "-I", str(source / "include"), "-I", str(source / "src"),
               "-dynamiclib" if suffix == "dylib" else "-shared", *sources,
               str(ROOT / "tools/box3d/bridge.c"), "-o", str(output),
               *([] if host == "Windows" else ["-lm", "-pthread"])]
    if args.sanitize:
        command.extend(["-fsanitize=address", "-fno-omit-frame-pointer"])
    subprocess.run(command, check=True, cwd=ROOT)
    print(output)


if __name__ == "__main__":
    main()
