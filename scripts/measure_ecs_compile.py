#!/usr/bin/env python3
"""Compare complete ECS consumer builds, excluding downloads and external caches."""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import platform
import shutil
import statistics
import subprocess
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCENARIOS = ("clean", "dependencies_warm", "library_edit", "application_edit", "no_change", "check_clean")


def capture(cmd, cwd=ROOT, env=None):
    return subprocess.run(cmd, cwd=cwd, env=env, check=True, text=True, capture_output=True).stdout.strip()


def source_hashes():
    files = []
    for directory in ("katla_ecs/src", "katla_derive/src", "odin", "benchmarks/ecs_compile/rust"):
        files.extend(p for p in (ROOT / directory).rglob("*") if p.is_file())
    files += [ROOT / name for name in ("katla_ecs/Cargo.toml", "katla_derive/Cargo.toml", "scripts/measure_ecs_compile.py")]
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(files)}


def snapshot(destination):
    destination.mkdir()
    for name in ("katla_ecs", "katla_derive", "odin", "benchmarks/ecs_compile"):
        shutil.copytree(ROOT / name, destination / name, ignore=shutil.ignore_patterns("target", "*.exe", "*.dSYM"))
    # Only dependency manifest lookup uses this workspace. The measured Rust
    # executable has its own workspace, lockfile and production-matched profiles.
    (destination / "Cargo.toml").write_text('''[workspace]
members = ["katla_ecs", "katla_derive"]
resolver = "3"
[workspace.dependencies]
katla_ecs = { path = "katla_ecs" }
katla_derive = { path = "katla_derive" }
rayon = "1.10"
serde_json = "1.0"
syn = { version = "2.0", features = ["full"] }
quote = "1.0"
proc-macro2 = "1.0"
''')


def edit(path, old, new):
    contents = path.read_text()
    if contents.count(old) != 1:
        raise RuntimeError(f"expected one edit anchor in {path}: {old}")
    path.write_text(contents.replace(old, new))


def summarize(rows):
    summary = []
    for feature in ("core", "editor"):
        for profile in ("dev", "release"):
            for scenario in SCENARIOS:
                times = {language: [r["seconds"] for r in rows if (r["feature"], r["profile"], r["scenario"], r["language"]) == (feature, profile, scenario, language)] for language in ("rust", "odin")}
                if not all(times.values()):
                    continue
                item = {"feature": feature, "profile": profile, "scenario": scenario}
                for language, values in times.items():
                    item[language] = {"median_seconds": statistics.median(values), "min_seconds": min(values), "max_seconds": max(values), "samples": len(values)}
                item["rust_over_odin"] = item["rust"]["median_seconds"] / item["odin"]["median_seconds"]
                summary.append(item)
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--samples", type=int, default=5)
    parser.add_argument("--cargo", type=Path, default=Path.home() / ".cargo/bin/cargo")
    parser.add_argument("--output", type=Path, default=ROOT / "docs/benchmarks/ecs-odin-compile")
    args = parser.parse_args()
    if args.samples < 3:
        parser.error("use at least three samples")
    cargo = str(args.cargo.resolve()) if not args.cargo.is_symlink() else str(args.cargo.absolute())
    odin = shutil.which("odin")
    if not odin:
        parser.error("Odin must be installed")
    env = os.environ.copy()
    for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS", "RUSTC_WORKSPACE_WRAPPER"):
        env.pop(key, None)
    env.update(RUSTC_WRAPPER="", RUSTC_WORKSPACE_WRAPPER="", CARGO_BUILD_RUSTC_WRAPPER="", CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER="", RUSTC=str(Path.home() / ".cargo/bin/rustc"))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    csv_path = args.output.with_suffix(".csv")
    json_path = args.output.with_suffix(".json")
    metadata = {
        "captured_at": datetime.now(timezone.utc).isoformat(),
        "base_commit": capture(["git", "rev-parse", "HEAD"]),
        "hardware": capture(["sysctl", "-n", "machdep.cpu.brand_string"]),
        "logical_cpus": os.cpu_count(),
        "memory_bytes": int(capture(["sysctl", "-n", "hw.memsize"])),
        "os": platform.platform(),
        "rustc": capture([env["RUSTC"], "-Vv"], env=env),
        "cargo": capture([cargo, "-V"], env=env),
        "odin": capture([odin, "version"]),
        "odin_root": capture([odin, "root"]),
        "compiler_binaries": {"cargo": cargo, "rustc": env["RUSTC"], "odin": odin},
        "samples": args.samples,
        "sources": source_hashes(),
        "method": {
            "ordering": "paired sequential builds, alternate Rust/Odin first per sample; no simultaneous builds",
            "clean": "remove variant-owned compiler output; dependency source and OS caches stay warm; offline Rust",
            "dependencies_warm": "retain third-party Rust artifacts and katla_derive, remove ECS and consumer artifacts; Odin rebuilds the entire program",
            "library_edit": "alternate default parallel work threshold 32768/32769; sequential output unchanged",
            "application_edit": "alternate STEP 1.0/2.0; complete matched consumer package rebuilt",
            "no_change": "unchanged source and previous outputs; Cargo freshness versus Odin whole-program rebuild",
            "check_clean": "cargo check versus odin check, clean artifacts, dev profile only, no link",
            "dev": "Rust opt-level 1, debug 1, incremental; Odin -debug -o:minimal",
            "release": "Rust opt-level 3, thin LTO, one codegen unit, no incremental; Odin -o:speed, default whole-module backend",
            "excluded": "downloads, compiler installation, tests, graphics, engine build and runtime timing",
            "cache_wrappers": "real Cargo/rustc paths; RUSTC_WRAPPER and workspace wrapper cleared; isolated CARGO_TARGET_DIR",
            "limitations": "same observable workload, different implementations and language safety guarantees; no affinity, frequency or filesystem-cache controls",
        },
        "commands": [],
        "checksums": [],
    }
    rows = []
    with tempfile.TemporaryDirectory(prefix="katla-ecs-compile-") as temp:
        temp = Path(temp)
        for feature in ("core", "editor"):
            for profile in ("dev", "release"):
                directory = temp / f"{feature}-{profile}"
                snapshot(directory)
                manifest = directory / "benchmarks/ecs_compile/rust/Cargo.toml"
                target = directory / "target-rust"
                odin_out = directory / "target-odin"
                local_env = env | {"CARGO_TARGET_DIR": str(target), "CARGO_INCREMENTAL": "1" if profile == "dev" else "0"}
                base_rust = [cargo, "build", "--manifest-path", str(manifest), "--locked", "--offline"]
                if feature == "editor":
                    base_rust += ["--features", "editor"]
                if profile == "release":
                    base_rust += ["--release"]
                package = "odin/editor_workload" if feature == "editor" else "odin/compile_workload"
                odin_flags = ["-debug", "-o:minimal"] if profile == "dev" else ["-o:speed"]
                base_odin = [odin, "build", package, f"-out:{odin_out}", *odin_flags]
                commands = {"rust": base_rust, "odin": base_odin}
                metadata["commands"].append({"feature": feature, "profile": profile, "rust": base_rust, "odin": base_odin})
                rust_app = directory / "benchmarks/ecs_compile/rust/src/main.rs"
                odin_app = directory / "odin/workload/workload.odin"
                rust_lib = directory / "katla_ecs/src/world.rs"
                odin_lib = directory / "odin/ecs/world.odin"
                step, threshold = 1, 32768
                for scenario in SCENARIOS:
                    if scenario == "check_clean" and profile != "dev":
                        continue
                    for sample in range(args.samples):
                        if scenario in ("clean", "check_clean"):
                            shutil.rmtree(target, ignore_errors=True)
                            odin_out.unlink(missing_ok=True)
                        elif scenario == "dependencies_warm":
                            # Cleaning outside the timed interval is intentional.
                            capture([cargo, "clean", "--manifest-path", str(manifest), "--offline", "--profile", profile, "-p", "katla_ecs", "-p", "katla-ecs-compile-workload"], directory, local_env)
                            odin_out.unlink(missing_ok=True)
                        elif scenario == "library_edit":
                            next_threshold = 32769 if threshold == 32768 else 32768
                            edit(rust_lib, f"parallel_work_threshold: {threshold},", f"parallel_work_threshold: {next_threshold},")
                            edit(odin_lib, f"w.parallel_work_threshold={threshold}", f"w.parallel_work_threshold={next_threshold}")
                            threshold = next_threshold
                        elif scenario == "application_edit":
                            next_step = 2 if step == 1 else 1
                            edit(rust_app, f"const STEP: f32 = {step}.0;", f"const STEP: f32 = {next_step}.0;")
                            edit(odin_app, f"STEP :: f32({step}.0)", f"STEP :: f32({next_step}.0)")
                            step = next_step
                        order = ("rust", "odin") if sample % 2 == 0 else ("odin", "rust")
                        outputs = {}
                        for language in order:
                            command = commands[language].copy()
                            if scenario == "check_clean":
                                command[1] = "check"
                                if language == "odin":
                                    command = [odin, "check", package]
                            started = time.perf_counter_ns()
                            process = subprocess.run(command, cwd=directory, env=local_env, text=True, capture_output=True)
                            seconds = (time.perf_counter_ns() - started) / 1e9
                            if process.returncode:
                                raise RuntimeError(f"{feature}/{profile}/{scenario}/{language}:\n{process.stdout}\n{process.stderr}")
                            rows.append({"feature": feature, "profile": profile, "scenario": scenario, "sample": sample, "language": language, "seconds": seconds})
                            print(f"{feature:6} {profile:7} {scenario:18} {sample+1}/{args.samples} {language:4} {seconds:.3f}s", flush=True)
                            if scenario != "check_clean":
                                binary = target / ("debug" if profile == "dev" else "release") / "katla-ecs-compile-workload" if language == "rust" else odin_out
                                outputs[language] = capture([str(binary)], directory, local_env)
                        if outputs:
                            if outputs["rust"] != outputs["odin"]:
                                raise RuntimeError(f"workload mismatch: {outputs}")
                            metadata["checksums"].append({"feature": feature, "profile": profile, "scenario": scenario, "sample": sample, "step": step, "value": outputs["rust"]})
                        # Preserve receipts after each complete pair, even if interrupted later.
                        with csv_path.open("w", newline="") as handle:
                            writer = csv.DictWriter(handle, fieldnames=rows[0].keys(), lineterminator="\n")
                            writer.writeheader(); writer.writerows(rows)
                        metadata["summary"] = summarize(rows)
                        metadata["csv_sha256"] = hashlib.sha256(csv_path.read_bytes()).hexdigest()
                        json_path.write_text(json.dumps(metadata, indent=2) + "\n")
    metadata["completed_at"] = datetime.now(timezone.utc).isoformat()
    json_path.write_text(json.dumps(metadata, indent=2) + "\n")
    print(f"Saved {csv_path} and {json_path}", flush=True)


if __name__ == "__main__":
    main()
