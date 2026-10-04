#!/usr/bin/env python3
"""Build pinned audio dependencies and verify real codecs/mixer/app ownership; optionally play native audio."""
import argparse
import os
from pathlib import Path
import platform
import subprocess
import sys

from build_odin_audio import build
from build_box3d import compiler

ROOT = Path(__file__).resolve().parents[1]
APP_TESTS = [
    'test_audio_actual_rooted_preview_toggle_failure_and_deleted_emitter',
    'test_audio_document_actual_source_codec_roundtrip_and_confinement',
    'test_audio_spatial_distance_doppler_zone_and_mixer_controls',
    'test_audio_actual_native_physics_occlusion_and_entity_lifecycle',
    'test_audio_unconfigured_add_snapshot_and_explicit_metadata_state',
    'test_audio_runtime_owned_script_loop_cue_reset_and_world_teardown',
]

def run(command, env=None):
    print('Running:', ' '.join(map(str, command)), flush=True)
    subprocess.run(list(map(str, command)), cwd=ROOT, env=env, check=True)

def define_library(name, library, import_dir):
    relative = os.path.relpath(library, ROOT / import_dir).replace(os.sep, '/')
    return f'-define:{name}={relative}'

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--native-asan-leaks', choices=('check', 'external-driver'), default='check', help='ASan native device process only: retain leak checking or explicitly exclude OS audio/XPC driver lifetime leaks; CPU suites always check leaks')
    parser.add_argument('--native', action='store_true', help='Deliver actual nonzero PCM to the default output device')
    parser.add_argument('--switch-default', action='store_true', help='macOS: temporarily switch the actual default output to a speaker-backed aggregate and restore it')
    parser.add_argument('--all-app-tests', action='store_true', help='Run every shared app test, in addition to targeted audio tests')
    parser.add_argument('--skip-cross-checks', action='store_true')
    args = parser.parse_args()
    if args.native_asan_leaks == 'external-driver' and not args.native:
        parser.error('--native-asan-leaks external-driver requires --native')
    if args.switch_default and (not args.native or platform.system() != 'Darwin'):
        parser.error('--switch-default requires --native on macOS')
    for sanitize in (False, True):
        suffix = '-asan' if sanitize else ''
        directory = ROOT / f'target/odin-audio{suffix}'
        library = build(directory, sanitize)
        env = os.environ.copy()
        env['CC'] = compiler(sanitize)
        gltf_directory = ROOT / f'target/odin-cgltf{suffix}'
        run([sys.executable, ROOT / 'scripts/build_odin_gltf.py', '--output', gltf_directory, *(['--sanitize'] if sanitize else [])], env)
        cpu_env = env.copy()
        if sanitize:
            parts = [part for part in cpu_env.get('ASAN_OPTIONS', '').split(':') if part and not part.startswith('detect_leaks=')]
            cpu_env['ASAN_OPTIONS'] = ':'.join([*parts, 'detect_leaks=1'])
            if platform.system() == 'Darwin' and 'suppressions=' not in cpu_env.get('LSAN_OPTIONS', ''):
                suppression = directory / 'external-cfprefs.lsan'
                suppression.write_text('# Observed Apple CFPreferences class initialization on its XPC dispatch thread.\nleak:CFPrefsPlistSource\nleak:CFPrefsSearchListSource\n')
                cpu_env['LSAN_OPTIONS'] = ':'.join(filter(None, [cpu_env.get('LSAN_OPTIONS', ''), f'suppressions={suppression}']))
                print('CPU LSan remains enabled; only the observed Apple CFPreferences initialization stack is suppressed.', flush=True)
        common = ['-define:ODIN_TEST_THREADS=1', '-vet', '-strict-style', *(['-sanitize:address'] if sanitize else []), define_library('AUDIO_LIBRARY', library, 'odin/deps/audio')]
        run(['odin', 'test', 'odin/audio', '-all-packages', *common, '-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true', f'-out:{directory}/audio-tests'], cpu_env)
        app_flags = [*common, define_library('CGLTF_LIBRARY', gltf_directory / 'libcgltf.a', 'odin/deps/cgltf')]
        # The actual occlusion consumer is exercised when Box3D has a native builder on this host.
        if platform.system() in ('Darwin', 'Linux'):
            box_library = directory / ('libbox3d.dylib' if platform.system() == 'Darwin' else 'libbox3d.so')
            run([sys.executable, ROOT / 'scripts/build_box3d.py', '--output', box_library, *(['--sanitize'] if sanitize else [])], env)
            app_flags += [f'-define:BOX3D_LIBRARY={box_library}']
        run(['odin', 'test', 'odin/app', *app_flags, '-define:ODIN_TEST_NAMES=' + ','.join(APP_TESTS), '-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true', f'-out:{directory}/app-audio-tests'], cpu_env)
        if args.all_app_tests:
            run(['odin', 'test', 'odin/app', '-all-packages', *app_flags, '-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true', f'-out:{directory}/app-all-tests'], cpu_env)
        if args.native:
            native_env = cpu_env.copy()
            if sanitize and args.native_asan_leaks == 'external-driver':
                parts = [part for part in native_env.get('ASAN_OPTIONS', '').split(':') if part and not part.startswith('detect_leaks=')]
                native_env['ASAN_OPTIONS'] = ':'.join([*parts, 'detect_leaks=0'])
                print('Native audio ASan checks addresses; leak checking is explicitly disabled only for this OS device process. Observed Apple NSXPC/HAL driver shutdown allocations are outside application ownership; CPU suites retain LSan.', flush=True)
            route = [define_library('AUDIO_ROUTE_LIBRARY', library, 'odin/audio_native')] if platform.system() == 'Darwin' else []
            run(['odin', 'run', 'odin/audio_native', *common, *route, f'-out:{directory}/audio-native', *(['--', '--switch-default'] if args.switch_default else [])], native_env)
    if not args.skip_cross_checks:
        for target in ('linux_amd64', 'windows_amd64'):
            run(['odin', 'check', 'odin/app', '-no-entry-point', '-vet', '-strict-style', f'-target:{target}'])
    print('PASS audio codec, mixer, worker, rooted application ownership and requested native acceptance', flush=True)

if __name__ == '__main__':
    main()
