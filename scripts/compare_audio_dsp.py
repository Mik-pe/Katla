#!/usr/bin/env python3
"""Compare native offline Rust/Odin DSP outputs, without opening a sound device."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def run(command, env):
    result = subprocess.run(command, cwd=ROOT, env=env, text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError(f"{' '.join(map(str, command))}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def compare(rust, odin):
    left, right = rust.splitlines(), odin.splitlines()
    if len(left) != 1001 or len(right) != 1001:
        raise AssertionError(f"Incomplete output: {len(left)}, {len(right)} records")
    count = 0
    largest = 0.0
    for line, (r, o) in enumerate(zip(left, right, strict=True)):
        r, o = r.split(), o.split()
        if r[0] != o[0] or len(r) != len(o):
            raise AssertionError(f"Record {line} shape differs")
        for component, (a, b) in enumerate(zip(r[1:], o[1:], strict=True)):
            a, b = float(a), float(b)
            if not math.isfinite(a) or not math.isfinite(b) or not math.isclose(a, b, abs_tol=2e-5, rel_tol=5e-5):
                raise AssertionError(f"{r[0]} sample {component}: Rust={a}, Odin={b}")
            count += 1
            largest = max(largest, abs(a - b))
    if count != 92283:
        raise AssertionError(f"Incomplete sample output: {count}")
    return {
        "records": len(left), "scalar_comparisons": count,
        "max_absolute_difference": largest,
        "rust_stdout_sha256": hashlib.sha256(rust.encode()).hexdigest(),
        "odin_stdout_sha256": hashlib.sha256(odin.encode()).hexdigest(),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--cargo", type=Path, default=Path.home() / ".cargo/bin/cargo")
    args = parser.parse_args()
    env = os.environ.copy()
    for name in ("RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"):
        env.pop(name, None)
    env["RUSTC"] = str(args.cargo.with_name("rustc"))
    odin = shutil.which("odin")
    if odin is None:
        raise RuntimeError("Odin must be installed and on PATH")
    report = {
        "captured_at_utc": datetime.now(timezone.utc).isoformat(),
        "host": {"system": platform.system(), "release": platform.release(), "machine": platform.machine()},
        "rust": run([str(args.cargo.with_name("rustc")), "-vV"], env).strip(),
        "odin": run([odin, "version"], env).strip(),
        "absolute_tolerance": 2e-5, "relative_tolerance": 5e-5,
        "sample_rates": [44100, 48000, 96000], "channels": [1, 2], "profiles": {},
    }
    with tempfile.TemporaryDirectory(prefix="katla-dsp-parity-") as directory:
        outputs = Path(directory)
        for profile in ("dev", "release"):
            rust_command = [str(args.cargo), "build", "--manifest-path", "benchmarks/audio_port/rust/Cargo.toml", "--locked", "--offline", "--target-dir", str(outputs / "rust")]
            if profile == "release":
                rust_command.append("--release")
            run(rust_command, env)
            binary = outputs / f"odin-{profile}"
            run([odin, "build", "odin/audio_reference", f"-out:{binary}", "-vet", "-strict-style", "-o:speed" if profile == "release" else "-o:none"], env)
            rust = run([str(outputs / "rust" / ("release" if profile == "release" else "debug") / "katla-audio-dsp-reference")], env)
            report["profiles"][profile] = compare(rust, run([str(binary)], env))
            print(f"{profile}: 1001 DSP records and 92283 scalar comparisons passed", flush=True)
    sources = list((ROOT / "odin/audio/dsp").glob("*.odin")) + list((ROOT / "odin/audio_reference").glob("*.odin"))
    sources += list((ROOT / "katla_audio/src").rglob("*.rs")) + list((ROOT / "benchmarks/audio_port/rust/src").glob("*.rs"))
    sources += [ROOT / "scripts/compare_audio_dsp.py", ROOT / "benchmarks/audio_port/rust/Cargo.toml", ROOT / "benchmarks/audio_port/rust/Cargo.lock"]
    report["source_sha256"] = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(sources)}
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
