#!/usr/bin/env python3
"""Build identical CPU harnesses against a Git baseline and the working tree."""

import argparse
import csv
import datetime
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile


ROOT = Path(__file__).resolve().parents[1]
CRATES = [("katla_math", "cpu_hotpaths"), ("katla_ecs", "ecs_comparison"),
          ("katla_ecs", "ecs_query_prepare"), ("katla_ui", "cpu_reconciliation")]


def command(*args, cwd=ROOT, **kwargs):
    return subprocess.check_output(args, cwd=cwd, text=True, **kwargs)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build(source, destination, env):
    binaries = []
    for crate, bench in CRATES:
        output = command("cargo", "build", "--profile", "bench", "--locked",
                         "-p", crate, "--bench", bench, "--message-format=json",
                         cwd=source, env=env)
        artifacts = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
        artifact = next(item for item in artifacts
                        if item.get("reason") == "compiler-artifact"
                        and item["target"]["name"] == bench and item.get("executable"))
        target = destination / f"{crate}-{bench}"
        shutil.copy2(artifact["executable"], target)
        binaries.append(target)
    return binaries


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-ref", required=True)
    parser.add_argument("--sample-ms", type=int, default=50)
    parser.add_argument("--output", type=Path, default=ROOT / "docs/benchmarks/cpu-hotpaths")
    args = parser.parse_args()
    if args.sample_ms <= 0:
        parser.error("--sample-ms must be positive")
    baseline_sha = command("git", "rev-parse", args.baseline_ref).strip()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, KATLA_BENCH_SAMPLE_MS=str(args.sample_ms), KATLA_BENCH_MODE="storage")
    with tempfile.TemporaryDirectory(prefix="katla-cpu-") as temporary:
        temporary = Path(temporary)
        source = temporary / "baseline"
        source.mkdir()
        archive = subprocess.check_output(["git", "archive", baseline_sha], cwd=ROOT)
        with tarfile.open(fileobj=io.BytesIO(archive)) as bundle:
            bundle.extractall(source, filter="data")
        harnesses = [ROOT / "scripts/cpu_bench_support.rs", Path(__file__).resolve()]
        for crate, bench in CRATES:
            harness = ROOT / crate / "benches" / f"{bench}.rs"
            harnesses.append(harness)
            if crate == "katla_ecs":
                harnesses.extend(sorted((ROOT / crate / "benches/comparison").glob("*.rs")))
            manifest = source / crate / "Cargo.toml"
            if f'name = "{bench}"' not in manifest.read_text():
                with manifest.open("a") as handle:
                    handle.write(f'\n[[bench]]\nname = "{bench}"\nharness = false\n')
        for harness in harnesses:
            target = source / harness.relative_to(ROOT)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(harness, target)
        executables = {}
        for label, tree in [("baseline", source), ("current", ROOT)]:
            directory = temporary / f"{label}-bin"
            directory.mkdir()
            # Cargo freshness can reuse relative source paths between checkouts.
            # Separate target directories keep baseline objects out of the current build.
            build_env = dict(env, CARGO_TARGET_DIR=str(ROOT / "target/cpu-comparison" / label))
            executables[label] = build(tree, directory, build_env)
        rows = {label: [] for label in executables}
        # All compilation finishes before sampling; reverse order on the second pass.
        for order in [("baseline", "current"), ("current", "baseline")]:
            for label in order:
                for binary in executables[label]:
                    capture = command(str(binary), env=env)
                    rows[label].extend(csv.DictReader(io.StringIO(capture)))
        for label, samples in rows.items():
            with (output / f"{label}.csv").open("w") as handle:
                writer = csv.DictWriter(handle, fieldnames=list(samples[0]))
                writer.writeheader()
                writer.writerows(samples)
        crates = sorted({crate for crate, _ in CRATES})
        production = [path for crate in crates for path in sorted((ROOT / crate / "src").rglob("*.rs"))]
        metadata = {
            "captured_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "baseline_commit": baseline_sha,
            "current_parent_commit": command("git", "rev-parse", "HEAD").strip(),
            "current_diff_sha256": hashlib.sha256(subprocess.check_output(["git", "diff", "--", *crates], cwd=ROOT)).hexdigest(),
            "toolchain": command("rustc", "-vV"), "platform": platform.platform(),
            "cpu": command("lscpu"), "sample_ms": args.sample_ms,
            "passes": 2, "order": ["baseline", "current", "current", "baseline"],
            "harness_sha256": {str(path.relative_to(ROOT)): digest(path) for path in harnesses},
            "binary_sha256": {label: {binary.name: digest(binary) for binary in binaries}
                              for label, binaries in executables.items()},
            "production_sha256": {str(path.relative_to(ROOT)): digest(path) for path in production},
            "manifest_sha256": {str(path.relative_to(ROOT)): digest(path)
                                for path in [ROOT / "Cargo.toml", ROOT / "Cargo.lock",
                                             *(ROOT / crate / "Cargo.toml" for crate in crates)]},
            "csv_sha256": {label: digest(output / f"{label}.csv") for label in rows},
        }
        (output / "environment.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
