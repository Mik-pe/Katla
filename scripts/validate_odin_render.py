#!/usr/bin/env python3
"""Validate actual Odin scene pixels, staged assets, models and native windows."""

import argparse
import os
from pathlib import Path
import platform
import shlex
import shutil
import sys

from build_odin_image import compiler_command
from build_katla_odin import cpu_test_environment, foreign_defines
from odin_validation_manifest import validation_manifest
from validate_odin_gpu import asan_environment, run

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-manifest", type=Path, help="Reuse exact verified canonical build.json dependencies and sanitizer mode; skip dependency builds")
    parser.add_argument("--native-metal", action="store_true")
    parser.add_argument("--native-vulkan", action="store_true")
    parser.add_argument("--native-surface", action="store_true", help="Include actual NSWindow resize, retained readback and presentation")
    parser.add_argument("--particles", action="store_true", help="Include the combined Box3D/Luau/GPU particle fixture on both adapters")
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument("--skip-cpu-tests", action="store_true")
    parser.add_argument("--shader-compiler", type=Path, help="Reuse this explicit canonical Naga compiler executable")
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target/odin-render-naga")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/odin-render-validation")
    parser.add_argument("--vulkan-library", type=Path)
    parser.add_argument("--vulkan-icd", type=Path)
    parser.add_argument("--luau-library", type=Path, help="Explicit source-pinned Luau native dependency for --particles")
    parser.add_argument("--box3d-library", type=Path, help="Explicit source-pinned Box3D native dependency for --particles")
    parser.add_argument("--odin", help="Odin compiler; defaults to the verified build compiler when a manifest is supplied")
    parser.add_argument("--cargo", default="cargo")
    args = parser.parse_args()
    manifest = validation_manifest(parser, args.build_manifest, args.sanitize, [
        ("--shader-compiler", "SHADER_COMPILER", args.shader_compiler),
        ("--luau-library", "LUAU_LIBRARY", args.luau_library),
        ("--box3d-library", "BOX3D_LIBRARY", args.box3d_library),
    ])
    if manifest:
        args.shader_compiler = Path(manifest["paths"]["SHADER_COMPILER"])
        args.luau_library = Path(manifest["paths"]["LUAU_LIBRARY"])
        args.box3d_library = Path(manifest["paths"]["BOX3D_LIBRARY"])
    args.odin = args.odin or (manifest["odin"] if manifest else "odin")
    native = args.native_metal or args.native_vulkan
    if native and (platform.system() != "Darwin" or platform.machine() != "arm64"):
        parser.error("These actual application consumers require macOS arm64 native hardware")
    if args.native_surface and not native:
        parser.error("--native-surface requires a selected native adapter and a Cocoa session")
    if args.particles and not (args.native_metal and args.native_vulkan and args.luau_library and args.box3d_library and args.vulkan_library):
        parser.error("--particles requires both adapters, --luau-library, --box3d-library and --vulkan-library")
    if (args.vulkan_library or args.vulkan_icd) and not args.native_vulkan:
        parser.error("Explicit Vulkan paths require --native-vulkan")
    for value in (args.shader_compiler, args.vulkan_library, args.vulkan_icd, args.luau_library, args.box3d_library):
        if value and not value.is_file():
            parser.error(f"Explicit dependency does not exist: {value}")
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    flags = ["-vet", "-strict-style"]
    if args.sanitize:
        flags += ["-debug", "-sanitize:address"]
    if manifest:
        flags += foreign_defines(manifest["paths"])
        library = args.shader_compiler.resolve()
    else:
        dependency_env = os.environ.copy()
        if args.sanitize:
            dependency_env["CC"] = shlex.join(compiler_command(True))
        for builder, directory, define, package, archive in (
            ("build_odin_gltf.py", "cgltf", "CGLTF_LIBRARY", "cgltf", "libcgltf.a"),
            ("build_odin_image.py", "image", "STB_IMAGE_LIBRARY", "stb_image", "libkatla_image.a"),
        ):
            dependency_output = output / directory
            run([sys.executable, ROOT / "scripts" / builder, "--output", dependency_output,
                 *(["--sanitize"] if args.sanitize else [])], env=dependency_env)
            relative = os.path.relpath(dependency_output / archive, ROOT / "odin/deps" / package).replace(os.sep, "/")
            flags.append(f"-define:{define}={relative}")
        font_output = output / "font"
        cxx = shlex.split(os.environ["CXX"]) if os.environ.get("CXX") else compiler_command(args.sanitize)
        if not os.environ.get("CXX"):
            driver = Path(cxx[0])
            cxx[0] = str(driver.with_name(driver.name.replace("clang", "clang++", 1))) if driver.name.startswith("clang") else "c++"
        run([sys.executable, ROOT / "tools/font_native/build.py", "--output", font_output,
             "--cc", shlex.join(compiler_command(args.sanitize)), "--cxx", shlex.join(cxx),
             *(["--sanitize"] if args.sanitize else [])], env=dependency_env)
        font_name = {"Darwin": "libkatla_font_native.dylib", "Linux": "libkatla_font_native.so", "Windows": "katla_font_native.dll"}[platform.system()]
        flags.append(f"-define:FONT_LIBRARY={font_output / font_name}")
        # App/render tests include the actual mixer and parser dependencies through their owner.
        for builder, directory, define, package, archive in (
            ("build_odin_audio.py", "audio", "AUDIO_LIBRARY", "audio", "libaudio_native.a"),
            ("build_odin_toml.py", "toml", "TOML_LIBRARY", "toml_native", "libtoml.a"),
        ):
            dependency_output = output / directory
            run([sys.executable, ROOT / "scripts" / builder, "--output", dependency_output,
                 *(["--sanitize"] if args.sanitize else [])], env=dependency_env)
            relative = os.path.relpath(dependency_output / archive, ROOT / "odin/deps" / package).replace(os.sep, "/")
            flags.append(f"-define:{define}={relative}")

        library = args.shader_compiler.resolve() if args.shader_compiler else args.target_dir.resolve() / "debug/katla-shader-compiler"
        if not args.shader_compiler:
            run([args.cargo, "build", "--manifest-path", ROOT / "tools/naga_bridge/Cargo.toml", "--locked", "--target-dir", args.target_dir.resolve()])
        if not library.is_file():
            raise RuntimeError(f"Native Naga compiler missing: {library}")
        flags.append(f"-define:SHADER_COMPILER={library}")

    def build(package, name, tests=False):
        executable = output / name
        run([args.odin, "build", package, f"-out:{executable}", *flags,
             *(["-build-mode:test", "-all-packages", "-define:ODIN_TEST_THREADS=1", "-define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true"] if tests else [])])
        return executable

    if not args.skip_cpu_tests:
        executable = build("odin/app/render", "render-tests", True)
        cpu_environment = cpu_test_environment(os.environ, output, args.sanitize)
        run([executable], env=cpu_environment, timeout=120)
    if not native:
        return
    # Snapshot real source resources so all scene/file mutations stay in this owned project.
    project = output / "project"
    project.mkdir(exist_ok=True)
    resources = project / "resources"
    if resources.exists():
        shutil.rmtree(resources)
    shutil.copytree(ROOT / "resources", resources)
    environment = os.environ.copy()
    environment.update(MTL_DEBUG_LAYER="1", METAL_DEVICE_WRAPPER_TYPE="1")
    if args.vulkan_icd:
        environment["VK_ICD_FILENAMES"] = str(args.vulkan_icd.resolve())
    if args.sanitize:
        environment = asan_environment(environment, False)
        print("GPU launches retain ASan address checks and Odin ownership trackers; only external CF/ObjC/driver process-exit leak detection is disabled.", flush=True)
    executable = build("odin/examples/material_render", "material-render")
    for enabled, backend in ((args.native_metal, "metal"), (args.native_vulkan, "vulkan")):
        if not enabled:
            continue
        loader = [args.vulkan_library.resolve()] if backend == "vulkan" and args.vulkan_library else []
        for mode, suffix in ((backend, "scene.png"), (backend + "-models", "models"),
                             *(([(backend + "-window", "window.png")] if args.native_surface else []))):
            run([executable, library, mode, output / f"{backend}-{suffix}", resources, *loader], env=environment, timeout=180)
    if args.particles:
        executable = build("odin/examples/particles_render", "particles-render")
        run([executable, library, args.vulkan_library.resolve(), args.luau_library.resolve(), args.box3d_library.resolve(), project], env=environment, timeout=180)
    if args.native_metal and args.native_vulkan:
        executable = build("odin/examples/resize_atomic", "resize-atomic")
        run([executable, library, args.vulkan_library.resolve() if args.vulkan_library else "", resources], env=environment, timeout=180)
        executable = build("odin/examples/render_features", "render-features")
        run([executable, library, args.vulkan_library.resolve() if args.vulkan_library else "libvulkan.dylib"], env=environment, timeout=180)
        executable = build("odin/examples/material_brdf_native", "material-brdf-native")
        run([executable, library, args.vulkan_library.resolve() if args.vulkan_library else "libvulkan.dylib"], env=environment, timeout=180)
        executable = build("odin/examples/model_sources_native", "model-sources-native")
        run([executable, library, args.vulkan_library.resolve() if args.vulkan_library else "libvulkan.dylib"], env=environment, timeout=180)
        executable = build("odin/examples/material_preview_native", "material-preview-native")
        font_library = Path(manifest["paths"]["FONT_LIBRARY"]) if manifest else font_output / font_name
        run([executable, library, font_library, resources, args.vulkan_library.resolve() if args.vulkan_library else "libvulkan.dylib", "--both"], env=environment, timeout=180)
        executable = build("odin/examples/material_coverage_native", "material-coverage-native")
        run([executable, library, args.vulkan_library.resolve() if args.vulkan_library else "libvulkan.dylib"], env=environment, timeout=180)
        executable = build("odin/examples/image_precision_native", "image-precision-native")
        run([executable, args.vulkan_library.resolve() if args.vulkan_library else "libvulkan.dylib"], env=environment, timeout=180)
        executable = build("odin/examples/texture_reload", "texture-reload")
        run([executable, library, args.vulkan_library.resolve() if args.vulkan_library else "libvulkan.dylib", resources], env=environment, timeout=180)
        executable = build("odin/app_render_shader_reload_native", "shader-reload")
        shader_root = Path(manifest["paths"]["SHADER_ROOT"]) if manifest else ROOT / "odin/app/render/shaders"
        run([executable, library, args.vulkan_library.resolve() if args.vulkan_library else "libvulkan.dylib", shader_root], env=environment, timeout=180)
    print("Actual Odin render consumers passed; retained GPU PNG outputs:", output, flush=True)


if __name__ == "__main__":
    main()
