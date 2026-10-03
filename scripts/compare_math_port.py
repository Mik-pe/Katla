#!/usr/bin/env python3
"""Build both math consumers and compare numeric contracts in dev and release."""

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import math
import os
import platform
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def run(args, env):
    result = subprocess.run(args, cwd=ROOT, env=env, text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError(f"{' '.join(map(str, args))}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def compare(rust, odin):
    left, right = rust.splitlines(), odin.splitlines()
    if len(left) != len(right) or not left:
        raise AssertionError(f"Record counts differ or are empty: {len(left)}, {len(right)}")
    values = 0
    max_abs = 0.0
    operations = set()
    counts = Counter()
    for index, (r, o) in enumerate(zip(left, right, strict=True)):
        rs, os_ = r.split(), o.split()
        if rs[0] != os_[0] or len(rs) != len(os_):
            raise AssertionError(f"Record {index}: shape mismatch: {r!r}, {o!r}")
        operations.add(rs[0])
        counts[rs[0]] += 1
        for component, (a, b) in enumerate(zip(rs[1:], os_[1:], strict=True)):
            a, b = float(a), float(b)
            if not math.isfinite(a) or not math.isfinite(b):
                raise AssertionError(f"Record {index} {rs[0]}: nonfinite result")
            if not math.isclose(a, b, abs_tol=2e-5, rel_tol=5e-5):
                raise AssertionError(f"Record {index} {rs[0]}[{component}]: Rust={a}, Odin={b}")
            values += 1
            max_abs = max(max_abs, abs(a - b))
    if len(operations) != 29 or set(counts.values()) != {128}:
        raise AssertionError(f"Incomplete reference workload: {dict(counts)}")
    return {
        "records": len(left),
        "scalar_comparisons": values,
        "operations": sorted(operations),
        "max_absolute_difference": max_abs,
        "rust_stdout_sha256": hashlib.sha256(rust.encode()).hexdigest(),
        "odin_stdout_sha256": hashlib.sha256(odin.encode()).hexdigest(),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--cargo", type=Path, default=Path.home() / ".cargo/bin/cargo")
    args = parser.parse_args()
    env = os.environ.copy()
    # Use actual compiler binaries rather than the user's Cargo wrapper.
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
        "absolute_tolerance": 2e-5,
        "relative_tolerance": 5e-5,
        "cases_per_profile": 128,
        "profiles": {},
    }
    with tempfile.TemporaryDirectory(prefix="katla-math-parity-") as directory:
        outputs = Path(directory)
        for profile in ("dev", "release"):
            rust_cmd = [str(args.cargo), "build", "--manifest-path", "benchmarks/math_port/rust/Cargo.toml", "--locked", "--offline", "--target-dir", str(outputs / "rust")]
            if profile == "release":
                rust_cmd.append("--release")
            run(rust_cmd, env)
            binary = outputs / f"odin-{profile}"
            run([odin, "build", "odin/math_reference", f"-out:{binary}", "-vet", "-strict-style", "-o:speed" if profile == "release" else "-o:none"], env)
            rust_out = run([str(outputs / "rust" / ("release" if profile == "release" else "debug") / "katla-math-port-reference")], env)
            odin_out = run([str(binary)], env)
            report["profiles"][profile] = compare(rust_out, odin_out)
            result = report["profiles"][profile]
            print(f"{profile}: {result['records']} records, {result['scalar_comparisons']} scalar comparisons passed")
    sources = list((ROOT / "odin/math").glob("*.odin"))
    sources += list((ROOT / "odin/math_reference").glob("*.odin"))
    sources += list((ROOT / "katla_math").rglob("*.rs"))
    sources += list((ROOT / "benchmarks/math_port/rust").rglob("*.rs"))
    sources += [ROOT / "scripts/compare_math_port.py", ROOT / "benchmarks/math_port/rust/Cargo.toml", ROOT / "benchmarks/math_port/rust/Cargo.lock"]
    report["source_sha256"] = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(sources)}
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
