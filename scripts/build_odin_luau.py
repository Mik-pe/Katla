#!/usr/bin/env python3
"""Build the source-pinned Luau VM/compiler and generic C ABI for Odin."""
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import argparse
import os
import platform
import re
import shlex
import shutil
import subprocess
from build_box3d import compiler

ROOT = Path(__file__).resolve().parents[1]
REVISION = "b968ef742741bb2b703afc3b3c53f06608c87481"

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument("--output", type=Path, help="Native artifact path; objects remain inside its parent")
    options = parser.parse_args()
    source = ROOT / "target/luau-source"
    source.parent.mkdir(exist_ok=True)
    if not source.exists():
        subprocess.run(["git", "clone", "--depth", "1", "--branch", "0.709",
                        "https://github.com/luau-lang/luau.git", source], check=True)
    actual = subprocess.check_output(["git", "-C", source, "rev-parse", "HEAD"], text=True).strip()
    if actual != REVISION or subprocess.check_output(["git", "-C", source, "status", "--porcelain"], text=True).strip():
        raise SystemExit("Luau requires the exact clean pinned source revision")
    host = platform.system()
    suffix = {"Darwin": "dylib", "Linux": "so", "Windows": "dll"}.get(host)
    if suffix is None:
        raise SystemExit(f"Unsupported native Luau platform: {host}")
    output = (options.output or ROOT / "target" / f"libkatla_luau{'_asan' if options.sanitize else ''}.{suffix}").resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    listing = (source / "Sources.cmake").read_text()
    sources = []
    for target in ("Common", "Ast", "Compiler", "VM"):
        files = re.search(rf"target_sources\(Luau\.{target} PRIVATE(.*?)\)", listing, re.S).group(1)
        sources.extend(source / name for name in re.findall(r"[\w/]+\.cpp", files))
    sources.extend([ROOT / "tools/luau_native/bridge.cpp", ROOT / "tools/luau_native/protected.cpp"])
    executable = shlex.split(os.environ.get("CXX", compiler(options.sanitize)))
    if "CXX" not in os.environ:
        cxx_name = Path(executable[0]).name.replace("clang", "clang++", 1)
        adjacent = Path(executable[0]).with_name(cxx_name)
        candidate = str(adjacent) if adjacent.is_file() else shutil.which(cxx_name)
        if candidate:
            executable[0] = candidate
        elif host == "Windows":
            raise SystemExit("Windows Luau requires the C++ Clang driver; set CXX to clang++ with the installed Windows SDK")
    directory = output.parent / ("luau-objects-asan" if options.sanitize else "luau-objects")
    directory.mkdir(exist_ok=True)
    flags = ["-x", "c++", "-std=c++17", "-O1" if options.sanitize else "-O2", "-g",
             *([] if host == "Windows" else ["-fPIC"]),
             '-DLUA_API=extern "C" __declspec(dllexport)' if host == "Windows" else '-DLUA_API=extern "C"',
             "-DLUA_USE_LONGJMP=1"]
    for include in ("Common", "Ast", "Compiler", "VM"):
        flags.extend(["-I", str(source / include / "include")])
    flags.extend(["-I", str(source / "VM/src")])
    if options.sanitize:
        flags.extend(["-fsanitize=address", "-fno-omit-frame-pointer"])
    def build(item):
        output = directory / (item.parent.parent.name + "_" + item.stem + ".o")
        subprocess.run([*executable, *flags, "-c", item, "-o", output], check=True)
        return output
    with ThreadPoolExecutor(max_workers=4) as pool:
        objects = list(pool.map(build, sources))
    cxx_driver = any("++" in Path(part).name for part in executable)
    command = [*executable, "-dynamiclib" if suffix == "dylib" else "-shared", *objects,
               *([] if host == "Windows" or cxx_driver else ["-lstdc++" if suffix == "so" else "-lc++"]),
               *([] if host == "Windows" else ["-pthread"]),
               *(["-fuse-ld=lld"] if host == "Windows" else []), "-o", output]
    if options.sanitize:
        command.append("-fsanitize=address")
    subprocess.run(command, check=True)
    print(output)

if __name__ == "__main__":
    main()
