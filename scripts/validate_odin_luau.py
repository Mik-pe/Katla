#!/usr/bin/env python3
"""Require the actual source-pinned VM and canonical Box3D/audio application consumers."""
from pathlib import Path
import os
import platform
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]

def run(command, environment=None):
    print("Running:", " ".join(map(str, command)), flush=True)
    missing_test = False
    with subprocess.Popen(list(map(str, command)), cwd=ROOT, env=environment,
                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True) as process:
        for line in process.stdout:
            print(line, end="", flush=True)
            missing_test |= "No test found for the name:" in line
        result = process.wait()
    if result:
        raise subprocess.CalledProcessError(result, command)
    if missing_test:
        raise RuntimeError("Requested native consumer test is absent; refusing an incomplete passing selection")

def main():
    suffix = "dylib" if platform.system() == "Darwin" else "so"
    if platform.system() not in ("Darwin", "Linux"):
        raise SystemExit("This native dependency build requires Darwin or Linux")
    names = ",".join([
        "app.test_runtime_native_direct_luau_box_trigger_animation_and_stop",
        "app.test_script_native_commands_queries_audio_variables_and_input",
        "app.test_script_native_remove_retains_rules_and_stop_restores_fresh_references",
        "app.test_script_source_names_and_explicit_file_capability_rollback",
        "app.test_script_normalized_attachment_and_file_reload_preserves_old_instance",
        "app.test_external_document_script_file_load_play_stop",
    ]) + ","
    for sanitize in (False, True):
        option = ["--sanitize"] if sanitize else []
        run([sys.executable, "scripts/build_odin_luau.py", *option])
        run([sys.executable, "scripts/build_box3d.py", *option])
        run([sys.executable, "scripts/build_odin_audio.py", *option])
        luau = ROOT / "target" / f"libkatla_luau{'_asan' if sanitize else ''}.{suffix}"
        box = ROOT / "target" / f"libkatla_box3d{'_asan' if sanitize else ''}.{suffix}"
        audio = ROOT / "target" / ("odin-audio-asan" if sanitize else "odin-audio") / "libaudio_native.a"
        flags = ["-vet", "-strict-style", "-define:ODIN_TEST_THREADS=1",
                 "-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true", f"-define:LUAU_LIBRARY={luau}"]
        if sanitize:
            flags.append("-sanitize:address")
        from build_katla_odin import cpu_test_environment
        environment = cpu_test_environment(os.environ, ROOT / "target", sanitize)
        run(["odin", "test", "odin/script", "-all-packages", *flags,
             "-out:target/odin-script-native-tests"], environment)
        run(["odin", "test", "odin/app", "-all-packages", *flags,
             f"-define:BOX3D_LIBRARY={box}",
             "-define:AUDIO_LIBRARY=" + os.path.relpath(audio, ROOT / "odin/deps/audio").replace(os.sep, "/"),
             f"-define:ODIN_TEST_NAMES={names}", "-out:target/odin-app-script-native-tests"], environment)
    for target in ("linux_amd64", "windows_amd64"):
        run(["odin", "check", "odin/app", "-no-entry-point", "-vet", "-strict-style", f"-target:{target}"])

if __name__ == "__main__":
    main()
