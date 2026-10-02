#!/usr/bin/env python3
"""Build both benchmark binaries first, then run them without concurrent compilation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from datetime import datetime, timezone

BASELINE = "8d3ceb142fe5c2f7eb421bfea3eda2f6be2c0247"
ROOT = Path(__file__).resolve().parents[2]


def capture(command, cwd=ROOT):
    return subprocess.check_output(command, cwd=cwd, text=True).strip()


def build(root, target, baseline):
    env = os.environ.copy()
    flags = [env.get("RUSTFLAGS", ""), "--check-cfg=cfg(katla_ecs_baseline)"]
    if baseline:
        flags.append("--cfg katla_ecs_baseline")
    env["RUSTFLAGS"] = " ".join(flags)
    env["CARGO_TARGET_DIR"] = str(target)
    result = subprocess.run(
        ["cargo", "bench", "-p", "katla_ecs", "--bench", "ecs_comparison", "--no-run", "--locked", "--message-format=json"],
        cwd=root, env=env, capture_output=True, text=True, check=False,
    )
    print(result.stderr, end="")
    if result.returncode:
        for line in result.stdout.splitlines():
            message = json.loads(line)
            if message.get("reason") == "compiler-message":
                print(message["message"].get("rendered", ""), end="")
        raise RuntimeError(f"Benchmark compilation failed with exit {result.returncode}")
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if message.get("reason") == "compiler-artifact" and message.get("target", {}).get("name") == "ecs_comparison" and message.get("executable"):
            return message["executable"]
    raise RuntimeError("Cargo did not return a benchmark executable")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "docs" / "benchmarks")
    parser.add_argument("--sample-ms", type=int, default=50)
    parser.add_argument("--current-only", action="store_true", help="Reuse an existing baseline captured with the identical Rust harness, toolchain, host, and sample length")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    source = ROOT / "katla_ecs" / "benches"
    hashes = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(source.rglob("*")) if p.is_file() and p.suffix in (".rs", ".py")}
    metadata = {
        "captured_utc": datetime.now(timezone.utc).isoformat(),
        "baseline_commit": BASELINE,
        "current_parent_commit": capture(["git", "rev-parse", "HEAD"]),
        "current_diff_sha256": hashlib.sha256(subprocess.check_output(["git", "diff", "--", "katla_ecs", "katla_derive"], cwd=ROOT)).hexdigest(),
        "harness_sha256": hashes,
        "production_sha256": {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for crate in ("katla_ecs", "katla_derive") for p in sorted((ROOT / crate / "src").rglob("*.rs"))},
        "rustc": capture(["rustc", "-Vv"]),
        "os": capture(["sw_vers"]),
        "cpu": capture(["sysctl", "-n", "machdep.cpu.brand_string"]),
        "logical_cpus": capture(["sysctl", "-n", "hw.logicalcpu"]),
        "physical_cpus": capture(["sysctl", "-n", "hw.physicalcpu"]),
        "memory_bytes": capture(["sysctl", "-n", "hw.memsize"]),
        "sample_ms": args.sample_ms,
        "samples": 7,
        "seed": "0x4b41544c41313338",
        "profile": "cargo bench (optimized default bench profile)",
        "notes": "No affinity or frequency pinning; inspect sample spread and repeat on the target workload. Baseline parallel path is the historical implementation with its known aliasing defects, measured only as historical timing.",
    }
    metadata["baseline_captured_utc"] = metadata["captured_utc"]
    if args.current_only:
        previous = json.loads((args.output / "ecs-environment.json").read_text())
        for key in ("baseline_commit", "rustc", "os", "cpu", "logical_cpus", "physical_cpus", "memory_bytes", "sample_ms", "samples", "seed", "profile"):
            if previous[key] != metadata[key]:
                raise RuntimeError(f"Existing baseline differs in {key}; run the full comparison")
        for name, digest in hashes.items():
            if name.endswith(".rs") and previous["harness_sha256"].get(name) != digest:
                raise RuntimeError(f"Existing baseline Rust harness differs at {name}; run the full comparison")
        if not (args.output / "ecs-baseline-8d3ceb14.csv").is_file():
            raise RuntimeError("Existing baseline CSV is missing; run the full comparison")
        metadata["baseline_captured_utc"] = previous.get("baseline_captured_utc", previous["captured_utc"])
    with tempfile.TemporaryDirectory(prefix="katla-ecs-baseline-") as directory:
        baseline_root = Path(directory)
        archive = subprocess.Popen(["git", "archive", BASELINE], cwd=ROOT, stdout=subprocess.PIPE)
        subprocess.run(["tar", "-x", "-C", directory], stdin=archive.stdout, check=True)
        archive.stdout.close()
        if archive.wait() != 0:
            raise RuntimeError("git archive failed")
        shutil.copy2(source / "ecs_comparison.rs", baseline_root / "katla_ecs" / "benches")
        shutil.copytree(source / "comparison", baseline_root / "katla_ecs" / "benches" / "comparison")
        with (baseline_root / "katla_ecs" / "Cargo.toml").open("a") as manifest:
            manifest.write('\n[[bench]]\nname = "ecs_comparison"\nharness = false\n')
        binaries = {}
        if not args.current_only:
            binaries["ecs-baseline-8d3ceb14"] = build(baseline_root, ROOT / "target" / "ecs-comparison-baseline", True)
        binaries["ecs-current"] = build(ROOT, ROOT / "target" / "ecs-comparison-current", False)
        current_hashes = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for crate in ("katla_ecs", "katla_derive") for p in sorted((ROOT / crate / "src").rglob("*.rs"))}
        if current_hashes != metadata["production_sha256"]:
            raise RuntimeError("Production source changed during compilation; rerun after edits finish")
        env = os.environ.copy()
        env["KATLA_BENCH_SAMPLE_MS"] = str(args.sample_ms)
        env["KATLA_BENCH_MODE"] = "all"
        for label, executable in binaries.items():
            with (args.output / f"{label}.csv").open("w") as output:
                subprocess.run([executable], cwd=ROOT, env=env, stdout=output, check=True)
    current_hashes = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for crate in ("katla_ecs", "katla_derive") for p in sorted((ROOT / crate / "src").rglob("*.rs"))}
    if current_hashes != metadata["production_sha256"]:
        raise RuntimeError("Production source changed during measurement; rerun after edits finish")
    metadata["csv_sha256"] = {name: hashlib.sha256((args.output / name).read_bytes()).hexdigest() for name in ("ecs-baseline-8d3ceb14.csv", "ecs-current.csv")}
    (args.output / "ecs-environment.json").write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    main()
