#!/usr/bin/env python3
"""Compare actual app CPU paths against a Git snapshot without a GPU."""

import argparse
from collections import Counter
import csv
import datetime
import hashlib
import io
import json
import math
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile


ROOT = Path(__file__).resolve().parents[1]
TRANSFORM = Path("katla_app/src/systems/transform")
HIERARCHY = Path("katla_app/src/ui/editor_ui/declarative")
EDITOR = Path("katla_app/src/application/editor")
HARNESSES = [TRANSFORM / "transform_hierarchy_benchmarks.rs",
             HIERARCHY / "hierarchy_bench.rs", EDITOR / "audio_metadata.rs",
             Path("scripts/cpu_bench_support.rs"), Path("scripts/compare_app_cpu.py")]
INJECTIONS = {
    Path("katla_app/src/lib.rs"): '\n#[cfg(test)]\n#[path = "../../scripts/cpu_bench_support.rs"]\nmod cpu_bench_support;\n',
    TRANSFORM / "transform_hierarchy_system.rs": '\n#[cfg(test)]\n#[path = "transform_hierarchy_benchmarks.rs"]\nmod benchmarks;\n',
    HIERARCHY / "hierarchy.rs": '\n#[cfg(test)]\n#[path = "hierarchy_bench.rs"]\nmod benchmarks;\n',
    EDITOR / "mod.rs": '\n#[cfg(test)]\nmod audio_metadata;\n',
}
TESTS = ["test_benchmark_transform_resolution", "test_benchmark_hierarchy_build",
         "test_benchmark_editor_audio_metadata"]


def command(*args, cwd=ROOT, **kwargs):
    return subprocess.check_output(args, cwd=cwd, text=True, **kwargs)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build(source, target, destination, env):
    output = command("cargo", "test", "-p", "katla_app", "--lib", "--all-features",
                     "--locked", "--profile", "bench", "--no-run", "--message-format=json",
                     cwd=source, env=dict(env, CARGO_TARGET_DIR=str(target)))
    artifacts = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
    artifact = next(item for item in artifacts if item.get("reason") == "compiler-artifact"
                    and item["target"]["name"] == "katla_app" and item.get("executable"))
    shutil.copy2(artifact["executable"], destination)


def capture(binary, env):
    rows = []
    fields = ["scenario", "entities", "threads", "matches", "sample", "iterations",
              "ns_per_iteration", "checksum"]
    header = ",".join(fields)
    for test in TESTS:
        output = command(str(binary), test, "--ignored", "--nocapture", "--test-threads=1", env=env)
        # libtest can prefix the first printed line with the test name.
        lines = [line[line.index(header):] if header in line else line
                 for line in output.splitlines() if line.count(",") == 7]
        reader = csv.DictReader(io.StringIO("\n".join(lines)))
        samples = list(reader)
        if not samples or reader.fieldnames != fields:
            raise RuntimeError(f"missing or invalid CPU benchmark output for {test}")
        rows.extend(samples)
    return rows


def validate(rows):
    expected = {(f"transform_resolve_{shape}", size)
                for shape in ["flat", "broad", "deep", "mixed_depth"]
                for size in [1000, 10000, 100000]}
    expected |= {(f"hierarchy_{shape}", size)
                 for shape in ["flat", "chain_expanded", "chain_collapsed", "chain_search",
                               "deep_then_roots"] for size in [128, 1024, 4096]}
    expected |= {(f"editor_audio_metadata_{selection}", size)
                 for selection in ["none", "selected"] for size in [1, 128, 1024]}
    checksums = {}
    for label, samples in rows.items():
        counts = Counter()
        for row in samples:
            key = (row["scenario"], int(row["entities"]))
            iterations = int(row["iterations"])
            ns, checksum = float(row["ns_per_iteration"]), float(row["checksum"])
            if iterations <= 0 or not math.isfinite(ns) or ns <= 0 or not math.isfinite(checksum):
                raise RuntimeError(f"invalid measurement: {label}: {row}")
            if int(row["threads"]) != 1 or int(row["matches"]) != key[1]:
                raise RuntimeError(f"unexpected input dimensions: {label}: {row}")
            counts[key, int(row["sample"])] += 1
            normalized = checksum / iterations
            if key in checksums and not math.isclose(normalized, checksums[key], rel_tol=1e-12):
                raise RuntimeError(f"observable checksum differs: {label}: {key}")
            checksums[key] = normalized
        required = Counter({(key, sample): 2 for key in expected for sample in range(7)})
        if counts != required:
            raise RuntimeError(f"missing, repeated or unexpected cases in {label}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-ref", required=True)
    parser.add_argument("--sample-ms", type=int, default=50)
    parser.add_argument("--output", type=Path, default=ROOT / "docs/benchmarks/app-cpu")
    args = parser.parse_args()
    if args.sample_ms <= 0:
        parser.error("--sample-ms must be positive")
    baseline = command("git", "rev-parse", args.baseline_ref).strip()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    overrides = {"CARGO_PROFILE_BENCH_LTO": "false", "CARGO_PROFILE_BENCH_CODEGEN_UNITS": "16",
                 "CARGO_PROFILE_BENCH_DEBUG": "0", "CARGO_INCREMENTAL": "0",
                 "KATLA_BENCH_SAMPLE_MS": str(args.sample_ms)}
    env = dict(os.environ, **overrides)
    with tempfile.TemporaryDirectory(prefix="katla-app-cpu-") as temporary:
        temporary = Path(temporary)
        source = temporary / "baseline"
        source.mkdir()
        archive = subprocess.check_output(["git", "archive", baseline], cwd=ROOT)
        with tarfile.open(fileobj=io.BytesIO(archive)) as bundle:
            bundle.extractall(source, filter="data")
        for relative in HARNESSES:
            target = source / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, target)
        for relative, include in INJECTIONS.items():
            path = source / relative
            with path.open("a") as handle:
                handle.write(include)
        binaries = {label: temporary / f"{label}-tests" for label in ["baseline", "current"]}
        for label, tree in [("baseline", source), ("current", ROOT)]:
            print(f"Building {label}", file=sys.stderr, flush=True)
            target = ROOT / "target/app-cpu-comparison" / (baseline if label == "baseline" else label)
            build(tree, target, binaries[label], env)
        rows = {label: [] for label in binaries}
        for order in [("baseline", "current"), ("current", "baseline")]:
            for label in order:
                print(f"Sampling {label}", file=sys.stderr, flush=True)
                run_env = dict(env, KATLA_EDITOR_AUDIO_REFERENCE="1" if label == "baseline" else "0")
                rows[label].extend(capture(binaries[label], run_env))
        validate(rows)
        for label, samples in rows.items():
            with (output / f"{label}.csv").open("w") as handle:
                writer = csv.DictWriter(handle, fieldnames=list(samples[0]))
                writer.writeheader()
                writer.writerows(samples)
        sources = sorted((ROOT / "katla_app/src").rglob("*.rs"))
        metadata = {
            "captured_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "baseline_commit": baseline,
            "current_parent_commit": command("git", "rev-parse", "HEAD").strip(),
            "toolchain": command("rustc", "-vV"), "platform": platform.platform(),
            "cpu": command("lscpu"), "sample_ms": args.sample_ms,
            "passes": 2, "order": ["baseline", "current", "current", "baseline"],
            "build_overrides": overrides,
            "inherited_build_flags": {key: os.environ.get(key) for key in
                                      ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET"]},
            "audio_reference": "Baseline runs the previous unconditional metadata reader adapter; both consume only selected inspector metadata. Current runs selected_audio_metadata.",
            "injected_includes": {str(path): include for path, include in INJECTIONS.items()},
            "injected_baseline_source_sha256": {str(path): digest(source / path) for path in INJECTIONS},
            "harness_sha256": {str(path): digest(ROOT / path) for path in HARNESSES},
            "production_sha256": {str(path.relative_to(ROOT)): digest(path) for path in sources},
            "manifest_sha256": {str(path.relative_to(ROOT)): digest(path) for path in
                                [ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / "katla_app/Cargo.toml"]},
            "binary_sha256": {label: digest(binary) for label, binary in binaries.items()},
            "csv_sha256": {label: digest(output / f"{label}.csv") for label in rows},
        }
        (output / "environment.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
