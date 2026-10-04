#!/usr/bin/env python3
"""Build source-pinned SDL3 and the direct native window/input C ABI."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request

ROOT=Path(__file__).resolve().parents[2]
COMMIT="7f3ae3d57459e59943a4ecfefc8f6277ec6bf540"
ARCHIVE_SHA256="7a8347c770b90b33daac2352858ca03f9c9a2ccc8ce711054870361d1a6b32e5"

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",type=Path,default=ROOT/"target/window-native")
    parser.add_argument("--cmake",default=shutil.which("cmake") or "cmake")
    parser.add_argument("--cc")
    parser.add_argument("--generator",default="Ninja" if os.name=="nt" else None)
    parser.add_argument("--sanitize",action="store_true")
    parser.add_argument("--build-smoke",action="store_true",help="Build the native acceptance executable; does not launch windows")
    args=parser.parse_args()
    output=args.output.resolve();output.mkdir(parents=True,exist_ok=True)
    cache=ROOT/"target/window-native-source"/COMMIT
    marker=cache/".katla-source-pin"
    if not marker.is_file():
        archive_bytes=urllib.request.urlopen(f"https://codeload.github.com/libsdl-org/SDL/tar.gz/{COMMIT}",timeout=120).read()
        if hashlib.sha256(archive_bytes).hexdigest()!=ARCHIVE_SHA256:raise RuntimeError("SDL source archive checksum mismatch")
        with tarfile.open(fileobj=io.BytesIO(archive_bytes)) as archive:
            for item in archive.getmembers():
                parts=Path(item.name).parts[1:]
                if not parts or any(p in ("..","") for p in parts):continue
                destination=cache.joinpath(*parts)
                if item.isdir():destination.mkdir(parents=True,exist_ok=True)
                elif item.isfile():
                    destination.parent.mkdir(parents=True,exist_ok=True)
                    destination.write_bytes(archive.extractfile(item).read())
        marker.write_text(COMMIT+"\n")
        (cache/".katla-archive-sha256").write_text(hashlib.sha256(archive_bytes).hexdigest()+"\n")
    if marker.read_text().strip()!=COMMIT:raise RuntimeError("SDL source pin mismatch")
    if (cache/".katla-archive-sha256").read_text().strip()!=ARCHIVE_SHA256:raise RuntimeError("SDL cached source archive checksum mismatch")
    build=output/"build"
    command=[args.cmake,"-S",ROOT/"tools/window_native","-B",build,f"-DKATLA_SDL_SOURCE={cache}",f"-DCMAKE_INSTALL_PREFIX={output}","-DCMAKE_BUILD_TYPE=RelWithDebInfo"]
    if args.generator:command += ["-G",args.generator]
    if args.cc:command.append(f"-DCMAKE_C_COMPILER={args.cc}")
    if args.build_smoke:command.append("-DKATLA_BUILD_SMOKE=ON")
    if args.sanitize:
        command += ["-DCMAKE_C_FLAGS=-fsanitize=address -fno-omit-frame-pointer -g","-DCMAKE_SHARED_LINKER_FLAGS=-fsanitize=address","-DCMAKE_EXE_LINKER_FLAGS=-fsanitize=address"]
    subprocess.run(list(map(str,command)),check=True)
    if os.name!="nt" and os.uname().sysname=="Linux":
        config=build/"sdl/include-config-/SDL3/SDL_build_config.h"
        candidates=list((build/"sdl").rglob("SDL_build_config.h"))
        if not candidates:raise RuntimeError("SDL native build configuration unavailable")
        config_text="\n".join(p.read_text() for p in candidates)
        for capability in ("SDL_VIDEO_DRIVER_X11","SDL_VIDEO_DRIVER_WAYLAND","HAVE_DBUS_DBUS_H","HAVE_IBUS_IBUS_H","HAVE_FCITX","SDL_USE_IME"):
            if f"#define {capability} 1" not in config_text:raise RuntimeError(f"Required Linux native capability missing: {capability}; install X11/Wayland/D-Bus/IBus development dependencies")
    subprocess.run([args.cmake,"--build",str(build),"--config","RelWithDebInfo","--parallel",str(min(8,os.cpu_count() or 1))],check=True)
    subprocess.run([args.cmake,"--install",str(build),"--config","RelWithDebInfo"],check=True)
    candidates=list(build.rglob("SDL3.dll"))+list(build.rglob("libSDL3.so*"))+list(build.rglob("libSDL3*.dylib"))
    for library in candidates:
        if library.is_file():shutil.copy2(library,output/library.name)
    shutil.copy2(cache/"LICENSE.txt",output/"SDL-LICENSE.txt")
    (output/"source-pins.json").write_text(json.dumps({"SDL":{"commit":COMMIT,"release":"3.2.28","archive_sha256":(cache/".katla-archive-sha256").read_text().strip()},"abi":1},indent=2)+"\n")

if __name__=="__main__":main()
