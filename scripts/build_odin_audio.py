#!/usr/bin/env python3
"""Build the pinned device/codec C ABI for the native Odin audio package."""
import argparse
import hashlib
import os
from pathlib import Path
import platform
import shlex
import subprocess
ROOT = Path(__file__).resolve().parents[1]
def build(output, sanitize=False):
    output = output.resolve(); output.mkdir(parents=True, exist_ok=True)
    from build_box3d import compiler
    cc = shlex.split(compiler(sanitize))
    for name, expected in {'miniaudio.h':'ac7af4de748b7e26b777f37e01cee313a308a7296a3eb080e2906b320cc55c89','stb_vorbis.c':'4c7cb2ff1f7011e9d67950446b7eb9ca044f2e464d76bfbb0b84dd2e23e65636'}.items():
        actual=hashlib.sha256((ROOT/'tools/audio_native'/name).read_bytes()).hexdigest()
        if actual!=expected: raise RuntimeError(f'Pinned source hash mismatch: {name}')
    ar = shlex.split(os.environ.get('AR', 'llvm-ar' if platform.system() == 'Windows' else 'ar'))
    obj = output / 'audio_native.o'; library = output / 'libaudio_native.a'
    command = [*cc, '-std=c11','-O2',*([] if platform.system()=='Windows' else ['-fPIC']),'-Wall','-Wextra','-Werror','-c', str(ROOT / 'tools/audio_native/audio_native.c'),'-o',str(obj)]
    if sanitize: command += ['-fsanitize=address','-fno-omit-frame-pointer','-g']
    subprocess.run(command,check=True,cwd=ROOT)
    objects=[obj]
    if platform.system()=='Darwin':
        route=output/'audio_route.o'
        route_command=[*cc,'-std=c11','-O2',*([] if platform.system()=='Windows' else ['-fPIC']),'-Wall','-Wextra','-Werror','-c',str(ROOT/'tools/audio_native/audio_route_darwin.c'),'-o',str(route)]
        if sanitize: route_command+=['-fsanitize=address','-fno-omit-frame-pointer','-g']
        subprocess.run(route_command,check=True,cwd=ROOT); objects.append(route)
    subprocess.run([*ar,'rcs',str(library),*[str(o) for o in objects]],check=True,cwd=ROOT)
    return library
if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__); parser.add_argument('--output',type=Path); parser.add_argument('--sanitize',action='store_true'); args=parser.parse_args()
    library=build(args.output or ROOT/('target/odin-audio-asan' if args.sanitize else 'target/odin-audio'),args.sanitize)
    print(library); print('-define:AUDIO_LIBRARY='+os.path.relpath(library,ROOT/'odin/deps/audio').replace(os.sep,'/'))
