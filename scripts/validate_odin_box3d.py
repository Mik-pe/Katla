#!/usr/bin/env python3
"""Require real pinned Box3D execution through native owners and the Odin scene."""
from pathlib import Path
import platform
import subprocess
import sys
from build_box3d import compiler

ROOT = Path(__file__).resolve().parents[1]


def run(command):
    print("Running:", " ".join(map(str, command)), flush=True)
    subprocess.run(list(map(str, command)), cwd=ROOT, check=True)


def main():
    suffix = "dylib" if platform.system() == "Darwin" else "so"
    if platform.system() not in ("Darwin", "Linux"):
        raise SystemExit("This native dependency driver requires Darwin or Linux")
    native_tests = ",".join([
        "app.test_box3d_scene_parent_local_commit_contact_and_sensor_transitions",
        "app.test_box3d_scene_preflight_rejects_whole_batch_and_backend_switch_gate",
        "app.test_box3d_missing_dependency_preserves_selection_and_scene",
        "app.test_physics_native_box3d_all_joint_variants_and_preview_restore",
    ]) + ","
    for sanitize in (False, True):
        run([sys.executable, "scripts/build_box3d.py", *( ["--sanitize"] if sanitize else [] )])
        library = ROOT / "target" / f"libkatla_box3d{'_asan' if sanitize else ''}.{suffix}"
        native_regression = ROOT / "target" / "box3d-joint-rollback-tests"
        run([compiler(sanitize), "-std=c17", "-Wall", "-Wextra", "-Werror",
             "-I", ROOT / "target/box3d-source/include", "tools/box3d/joints_test.c",
             library, *( ["-fsanitize=address", "-fno-omit-frame-pointer"] if sanitize else [] ),
             "-o", native_regression])
        run([native_regression])
        flags = ["-vet", "-strict-style", "-define:ODIN_TEST_THREADS=1", f"-define:BOX3D_LIBRARY={library}"]
        if sanitize:
            flags.append("-sanitize:address")
        run(["odin", "test", "odin/physics/box3d", *flags, "-out:target/odin-box3d-validation-tests"])
        run(["odin", "test", "odin/app", "-all-packages", *flags,
             f"-define:ODIN_TEST_NAMES={native_tests}", "-out:target/odin-app-box3d-validation-tests"])
    for target in ("linux_amd64", "windows_amd64"):
        run(["odin", "check", "odin/physics/box3d", "-no-entry-point", "-vet", "-strict-style", f"-target:{target}"])


if __name__ == "__main__":
    main()
