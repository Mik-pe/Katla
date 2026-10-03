#!/usr/bin/env python3
"""Measure paired full glTF imports through the native application fixture."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
TEST = 'application::spawning::material_tests::import_benchmark::test_native_full_material_asset_import_measurements'


def binary(root):
    command = ['cargo', 'test', '-p', 'katla_app', '--lib', '--all-features', '--no-run', '--message-format=json']
    result = subprocess.run(command, cwd=root, capture_output=True, text=True, check=True)
    for line in result.stdout.splitlines():
        try:
            artifact = json.loads(line)
        except json.JSONDecodeError:
            continue
        if artifact.get('reason') == 'compiler-artifact' and artifact.get('executable') and artifact.get('target', {}).get('name') == 'katla_app':
            return artifact['executable']
    raise RuntimeError('cargo produced no katla_app test executable: ' + result.stderr[-3000:])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', required=True, type=Path)
    parser.add_argument('--iterations', type=int, default=5)
    parser.add_argument('--output', type=Path, default=ROOT / 'docs/material-import-study')
    args = parser.parse_args()
    if args.iterations < 3:
        parser.error('at least three paired samples are required')
    baseline = args.baseline.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    roots = {'baseline': baseline, 'current': ROOT}
    executables = {name: binary(root) for name, root in roots.items()}
    asset = ROOT / 'resources/models/DamagedHelmet.glb'
    env = dict(os.environ, KATLA_MATERIAL_BENCHMARK_ASSET=str(asset), RUST_LOG='warn')
    rows = []
    expected = None
    for iteration in range(args.iterations):
        order = ['baseline', 'current'] if iteration % 2 == 0 else ['current', 'baseline']
        for version in order:
            result = subprocess.run([executables[version], TEST, '--exact', '--ignored', '--nocapture'], cwd=roots[version], env=env, capture_output=True, text=True)
            (output / f'{version}-{iteration + 1}.log').write_text(result.stdout + result.stderr)
            if result.returncode:
                raise RuntimeError(f'{version} sample {iteration + 1} failed; inspect its log')
            measurement = next((json.loads(line.removeprefix('KATLA_MATERIAL_IMPORT ')) for line in result.stdout.splitlines() if line.startswith('KATLA_MATERIAL_IMPORT ')), None)
            if measurement is None:
                raise RuntimeError('native fixture returned no measurement')
            pixel = measurement['native_rgba16f_texel']
            if expected is None:
                expected = pixel
            if expected != pixel:
                raise RuntimeError('native texel changed between versions or samples')
            measurement.update(version=version, iteration=iteration + 1)
            rows.append(measurement)
            print(f'{version} {iteration + 1}: first ready {measurement["first_ready_us"]/1000:.2f} ms; warm seven ready {measurement["warm_ready_us"]/1000:.2f} ms', flush=True)
    fields = ['decode_us', 'first_cpu_us', 'first_ready_us', 'warm_cpu_us', 'warm_ready_us', 'unique_texture_handles']
    medians = {version: {field: statistics.median(row[field] for row in rows if row['version'] == version) for field in fields} for version in roots}
    report = {
        'baseline_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=baseline, text=True).strip(),
        'current_base_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'current_tree_diff': subprocess.check_output(['git', 'diff', '--stat'], cwd=ROOT, text=True),
        'baseline_fixture_sha256': hashlib.sha256((baseline / 'katla_app/src/application/spawning/import_benchmark.rs').read_bytes()).hexdigest(),
        'fixture_sha256': hashlib.sha256((ROOT / 'katla_app/src/application/spawning/import_benchmark.rs').read_bytes()).hexdigest(),
        'asset_sha256': hashlib.sha256(asset.read_bytes()).hexdigest(),
        'platform': platform.platform(),
        'rustc': subprocess.check_output(['rustc', '-Vv'], text=True),
        'cpu': subprocess.check_output(['lscpu'], text=True),
        'profile': 'test/dev opt-level=1; eight complete imports; cached OS file pages; shader/driver warmup excluded',
        'samples': rows,
        'medians': medians,
    }
    (output / 'measurements.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(medians, indent=2), flush=True)


if __name__ == '__main__':
    main()
