#!/usr/bin/env python3
"""Build Katla's Odin application and source-pinned native dependencies."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
ODIN_VERSION = "dev-2026-09"
CPU_TEST_PACKAGES = ("app", "app/render", "app/editor", "gfx", "gfx/shader_tests",
                     "script", "audio", "ui", "resources", "agent/host", "deps/font_native")
ODIN_ARCHIVES = {
    ("Linux", "amd64"): ("linux-amd64.tar.gz", "167c3e1d7056419dad2e04bb3bd98715b7ff286d4c125f3c5a5ee337c6254283"),
    ("Linux", "arm64"): ("linux-arm64.tar.gz", "c150c6f2d13668f3a1c4116ef96ea85c1ba1c9ededfd76081cd3c7d4ba92aa91"),
    ("Darwin", "amd64"): ("macos-amd64.tar.gz", "c1f6d6320218ec7e511093a87bdd599b72a8417a9d2a00de2c9691103562165b"),
    ("Darwin", "arm64"): ("macos-arm64.tar.gz", "3e6cbc1f247d8d14fe02c3151272d0a5b8d6d77acb7219f5b62914e4d95d97f7"),
    ("Windows", "amd64"): ("windows-amd64.zip", "8ed8abd95b82e0af1eb7e86cb75f380fd8bde104947721817b35929f5fc19388"),
}


def architecture():
    machine = platform.machine().lower()
    return {"x86_64": "amd64", "x64": "amd64", "aarch64": "arm64"}.get(machine, machine)


def output_path(sanitize=False):
    return ROOT / "target/katla-odin" / f"{platform.system().lower()}-{architecture()}" / ("asan" if sanitize else "normal")


def run(command, environment=None):
    print("+ " + shlex.join(map(str, command)), flush=True)
    subprocess.run(list(map(str, command)), cwd=ROOT, env=environment, check=True)


def install_odin(directory):
    """Install the verified host release; keep its vendor/core collection beside Odin."""
    directory = directory.resolve()
    descriptor, digest = ODIN_ARCHIVES[(platform.system(), architecture())]
    target, extension = descriptor.split(".", 1)
    name = f"odin-{target}-{ODIN_VERSION}.{extension}"
    url = f"https://github.com/odin-lang/Odin/releases/download/{ODIN_VERSION}/{name}"
    directory.mkdir(parents=True, exist_ok=True)
    archive = directory / name
    if not archive.exists():
        with urllib.request.urlopen(url, timeout=60) as response:
            archive.write_bytes(response.read())
    if hashlib.sha256(archive.read_bytes()).hexdigest() != digest:
        raise RuntimeError(f"Odin release checksum mismatch: {archive}")
    extraction = directory / ODIN_VERSION
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as package:
            for entry in package.infolist():
                candidate = extraction / entry.filename
                if not candidate.resolve().is_relative_to(extraction.resolve()):
                    raise RuntimeError("Invalid Odin release member path")
            package.extractall(extraction)
    else:
        with tarfile.open(archive) as package:
            package.extractall(extraction, filter="data")
    filename = "odin.exe" if platform.system() == "Windows" else "odin"
    candidates = [path for path in extraction.rglob(filename) if path.is_file()]
    if len(candidates) != 1:
        raise RuntimeError(f"Odin release does not contain exactly one {filename}")
    return candidates[0]


def native_environment(odin, sanitize):
    environment = os.environ.copy()
    # Existing builders discover Odin through PATH, including the ASan LLVM version.
    environment["PATH"] = str(odin.parent) + os.pathsep + environment.get("PATH", "")
    if sanitize:
        from build_box3d import compiler
        old_path = os.environ.get("PATH", "")
        try:
            os.environ["PATH"] = environment["PATH"]
            selected = compiler(True)
        finally:
            os.environ["PATH"] = old_path
        command = shlex.split(selected)
        version = subprocess.check_output([*command, "--version"], text=True)
        report = subprocess.check_output([str(odin), "report"], text=True)
        llvm = re.search(r"Backend:\s+LLVM (\d+)", report)
        clang = re.search(r"(?<!Apple )clang version (\d+)\.", version)
        if not llvm or not clang or llvm.group(1) != clang.group(1):
            raise RuntimeError("ASan needs non-Apple Clang matching Odin's LLVM major, including an explicit CC override")
        environment["CC"] = selected
        if "CXX" not in environment:
            sibling = Path(command[0]).with_name("clang++" + (".exe" if os.name == "nt" else ""))
            if not sibling.is_file():
                found = shutil.which(f"clang++-{llvm.group(1)}")
                if found is None:
                    raise RuntimeError("Set CXX to Clang++ matching Odin's LLVM for ASan font/Luau builds")
                sibling = Path(found)
            environment["CXX"] = str(sibling)
        cxx = subprocess.check_output(shlex.split(environment["CXX"]) + ["--version"], text=True)
        if not re.search(rf"(?<!Apple )clang version {llvm.group(1)}\.", cxx):
            raise RuntimeError("ASan CXX must match Odin's LLVM major")
    return environment


def foreign_defines(paths):
    result = []
    for name, package in (("AUDIO_LIBRARY", "audio"), ("CGLTF_LIBRARY", "cgltf"),
                          ("STB_IMAGE_LIBRARY", "stb_image"), ("TOML_LIBRARY", "toml_native")):
        relative = os.path.relpath(paths[name], ROOT / "odin/deps" / package).replace(os.sep, "/")
        result.append(f"-define:{name}={relative}")
    for name in ("BOX3D_LIBRARY", "LUAU_LIBRARY", "FONT_LIBRARY", "SHADER_COMPILER"):
        result.append(f"-define:{name}={paths[name]}")
    return result


def cpu_test_environment(environment, directory, sanitize):
    """Keep CPU ownership checks active independently of native driver policies."""
    result = environment.copy()
    if not sanitize:
        return result
    options = [value for value in result.get("ASAN_OPTIONS", "").split(":")
               if value and not value.startswith("detect_leaks=")]
    result["ASAN_OPTIONS"] = ":".join([*options, "detect_leaks=1"])
    if platform.system() == "Darwin":
        suppression = directory / "cpu-external-cfprefs.lsan"
        suppression.write_text("leak:CFPrefsPlistSource\nleak:CFPrefsSearchListSource\n")
        options = [value for value in result.get("LSAN_OPTIONS", "").split(":")
                   if value and not value.startswith("suppressions=")]
        result["LSAN_OPTIONS"] = ":".join([*options, f"suppressions={suppression}"])
        print("CPU ASan leak detection remains enabled; Darwin suppresses only the observed CFPrefsPlistSource/CFPrefsSearchListSource OS cache stacks.", flush=True)
    return result



def ship_shader_sources(directory):
    """Ship the canonical Odin app shaders, including all relative include sources."""
    source = ROOT / "odin/app/render/shaders"
    files = sorted(source.rglob("*"))
    if not source.is_dir() or not any(path.is_file() for path in files):
        raise RuntimeError("Canonical Odin shader source directory is empty or missing")
    staging = directory / "shaders.staging"
    target = directory / "shaders"
    shutil.rmtree(staging, ignore_errors=True)
    staging.mkdir()
    for path in files:
        if path.is_symlink():
            raise RuntimeError(f"Canonical shader source cannot be a symlink: {path}")
        if path.is_file():
            destination = staging / path.relative_to(source)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, destination)
    shutil.rmtree(target, ignore_errors=True)
    staging.replace(target)
    return target


def build(options):
    host = platform.system()
    directory = (options.output or output_path(options.sanitize)).resolve()
    directory.mkdir(parents=True, exist_ok=True)
    mode_path = directory / "mode.json"
    mode = {"sanitize": options.sanitize, "host": host, "architecture": architecture()}
    if mode_path.exists() and json.loads(mode_path.read_text()) != mode:
        raise RuntimeError("Build directory belongs to a different host or sanitizer mode; select another --output")
    mode_path.write_text(json.dumps(mode, sort_keys=True) + "\n")
    manifest_path = directory / "build.json"
    # A failed rebuild must never publish an older successful manifest.
    manifest_path.unlink(missing_ok=True)
    odin = Path(shutil.which(options.odin) or options.odin).resolve()
    if not odin.is_file():
        raise RuntimeError("Odin is unavailable; pass --odin or use --install-odin")
    environment = native_environment(odin, options.sanitize)
    report = subprocess.check_output([str(odin), "report"], text=True)
    print(report, flush=True)
    dependencies = directory / "deps"
    dependencies.mkdir(exist_ok=True)
    instrument = ["--sanitize"] if options.sanitize else []
    suffix = {"Darwin": ".dylib", "Linux": ".so", "Windows": ".dll"}[host]
    paths = {"AUDIO_LIBRARY": dependencies / "audio/libaudio_native.a",
             "CGLTF_LIBRARY": dependencies / "cgltf/libcgltf.a",
             "STB_IMAGE_LIBRARY": dependencies / "image/libkatla_image.a",
             "TOML_LIBRARY": dependencies / "toml/libtoml.a",
             "BOX3D_LIBRARY": dependencies / f"box/libkatla_box3d{suffix}",
             "LUAU_LIBRARY": dependencies / f"luau/libkatla_luau{suffix}",
             "FONT_LIBRARY": dependencies / "fonts" / ("katla_font_native.dll" if host == "Windows" else f"libkatla_font_native{suffix}"),
             "SHADER_COMPILER": directory / "shader-compiler/debug" / ("katla-shader-compiler.exe" if host == "Windows" else "katla-shader-compiler")}
    for builder, key in (("audio", "AUDIO_LIBRARY"), ("gltf", "CGLTF_LIBRARY"), ("image", "STB_IMAGE_LIBRARY"), ("toml", "TOML_LIBRARY")):
        run([sys.executable, ROOT / f"scripts/build_odin_{builder}.py", "--output", paths[key].parent, *instrument], environment)
    run([sys.executable, ROOT / "scripts/build_box3d.py", "--output", paths["BOX3D_LIBRARY"], *instrument], environment)
    run([sys.executable, ROOT / "scripts/build_odin_luau.py", "--output", paths["LUAU_LIBRARY"], *instrument], environment)
    run([sys.executable, ROOT / "tools/font_native/build.py", "--output", paths["FONT_LIBRARY"].parent, *instrument], environment)
    if host in ("Linux", "Windows"):
        window_dir = dependencies / "window"
        paths["WINDOW_LIBRARY"] = window_dir / ("katla_window_native.dll" if host == "Windows" else "libkatla_window_native.so")
        selected_cc = shlex.split(environment.get("CC", "clang"))
        if len(selected_cc) != 1:
            raise RuntimeError("SDL CMake compiler must be a single executable; use CC without wrapper arguments")
        run([sys.executable, ROOT / "tools/window_native/build.py", "--output", window_dir, "--cc", selected_cc[0], *instrument], environment)
        if host == "Windows":
            environment["PATH"] = str(window_dir) + os.pathsep + environment.get("PATH", "")
    run([options.cargo, "build", "--manifest-path", ROOT / "tools/naga_bridge/Cargo.toml", "--locked", "--target-dir", directory / "shader-compiler"], environment)
    missing = [path for path in paths.values() if not path.is_file()]
    if missing:
        raise RuntimeError(f"Native build did not produce required artifacts: {missing}")
    shader_root = ship_shader_sources(directory)
    flags = ["-vet", "-strict-style", "-debug", *foreign_defines(paths)]
    if options.sanitize:
        flags += ["-sanitize:address"]
    if options.optimize:
        flags += ["-o:speed"]
    binaries = directory / "bin"
    binaries.mkdir(exist_ok=True)
    executable = binaries / ("katla.exe" if host == "Windows" else "katla")
    if not options.dependencies_only:
        if options.objects_only:
            objects_directory = directory / "objects"
            shutil.rmtree(objects_directory, ignore_errors=True)
            objects_directory.mkdir(exist_ok=True)
            run([odin, "build", ROOT / "odin/katla", "-build-mode:obj",
                 *flags, f"-out:{objects_directory / 'katla'}"], environment)
        else:
            run([odin, "build", ROOT / "odin/katla", *flags, f"-out:{executable}"], environment)
        for package in ("mcp_stdio", "mcp_proxy"):
            run([odin, "build", ROOT / "odin" / package, *flags,
                 f"-out:{binaries / ('katla-' + package.replace('_', '-') + ('.exe' if host == 'Windows' else ''))}"], environment)
    if options.tests:
        test_environment = cpu_test_environment(environment, directory, options.sanitize)
        for package in CPU_TEST_PACKAGES:
            run([odin, "test", ROOT / "odin" / package, "-all-packages", *flags,
                 "-define:ODIN_TEST_THREADS=1", "-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true",
                 f"-out:{binaries / (package.replace('/', '-') + '-tests' + ('.exe' if host == 'Windows' else ''))}"], test_environment)
    artifacts = list(paths.values())
    artifacts.extend(sorted(path for path in shader_root.rglob("*") if path.is_file()))
    if options.objects_only:
        artifacts.extend(sorted(path for path in (directory / "objects").iterdir() if path.suffix in (".o", ".obj")))
    if executable.is_file() and not options.objects_only and not options.dependencies_only:
        artifacts.append(executable)
    if not options.dependencies_only:
        artifacts.extend(binaries / ("katla-" + package.replace("_", "-") + (".exe" if host == "Windows" else ""))
                         for package in ("mcp_stdio", "mcp_proxy"))
    artifacts.extend(sorted((paths["FONT_LIBRARY"].parent / "fonts").glob("*.ttf")))
    if host in ("Linux", "Windows"):
        artifacts.extend(sorted(path for path in (dependencies / "window").glob("*SDL3*") if path.is_file()))
    manifest = {"version": 1, "host": host, "architecture": architecture(),
        "sanitize": options.sanitize, "odin": str(odin), "odin_report": report,
        "root": str(ROOT), "executable": str(executable) if executable.is_file() and not options.objects_only and not options.dependencies_only else None,
        "paths": {**{name: str(path) for name, path in paths.items()}, "SHADER_ROOT": str(shader_root)},
        "artifact_sha256": {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in artifacts},
        "foreign_defines": foreign_defines(paths), "tests_passed": options.tests,
        "test_packages": list(CPU_TEST_PACKAGES) if options.tests else [],
        "editor_entrypoint_built": not options.objects_only and not options.dependencies_only}
    temporary = directory / "build.json.tmp"
    temporary.write_text(json.dumps(manifest, indent=2) + "\n")
    temporary.replace(manifest_path)
    print(f"Build manifest: {manifest_path}", flush=True)
    return manifest_path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="Mode-specific output; default target/katla-odin/<host-arch>/<normal|asan>")
    parser.add_argument("--odin", default="odin")
    parser.add_argument("--cargo", default="cargo", help="Only builds the isolated, locked offline Naga executable")
    parser.add_argument("--install-odin", type=Path, help="Download and verify the pinned host compiler, then exit")
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument("--optimize", action="store_true")
    parser.add_argument("--dependencies-only", action="store_true")
    parser.add_argument("--objects-only", action="store_true", help="Generate actual editor entrypoint and reachable application object code without linking a desktop executable")
    parser.add_argument("--tests", action="store_true", help="Run real native-dependency CPU suites serially")
    options = parser.parse_args()
    if options.install_odin:
        print(install_odin(options.install_odin))
    else:
        build(options)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, KeyError, subprocess.CalledProcessError, OSError) as error:
        print(f"Katla build failed: {error}", file=sys.stderr)
        raise SystemExit(1)
