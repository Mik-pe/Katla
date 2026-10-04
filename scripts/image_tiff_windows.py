"""Build pinned IJG/TIFF/zlib static libraries with Clang, CMake and Ninja."""
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
ZLIB_URL = "https://zlib.net/fossils/zlib-1.3.2.tar.gz"
ZLIB_SHA256 = "bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16"


def zlib_source():
    cache = ROOT / "target/image-codec-source"
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / "zlib-1.3.2.tar.gz"
    if not archive.exists():
        with urllib.request.urlopen(ZLIB_URL, timeout=60) as response:
            archive.write_bytes(response.read())
    if hashlib.sha256(archive.read_bytes()).hexdigest() != ZLIB_SHA256:
        raise RuntimeError(f"Pinned zlib source digest mismatch: {archive}")
    with tarfile.open(archive) as package:
        source = cache / "zlib-1.3.2"
        if not source.exists():
            package.extractall(cache, filter="data")
        for entry in package.getmembers():
            if entry.isfile():
                actual = cache / entry.name
                if not actual.is_file() or actual.read_bytes() != package.extractfile(entry).read():
                    raise RuntimeError(f"Pinned zlib source modified: {actual}")
    return source


def cmake_build(source, output, compiler, sanitize, arguments, target):
    flags = "-O2" + (" -fsanitize=address -fno-omit-frame-pointer -g" if sanitize else "")
    subprocess.run(["cmake", "-S", str(source), "-B", str(output), "-G", "Ninja",
                    f"-DCMAKE_C_COMPILER={compiler[0]}",
                    f"-DCMAKE_C_COMPILER_ARG1={shlex.join(compiler[1:])}",
                    f"-DCMAKE_C_FLAGS={flags}", "-DCMAKE_BUILD_TYPE=Release",
                    "-DCMAKE_POSITION_INDEPENDENT_CODE=ON",
                    "-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL", *arguments], check=True)
    subprocess.run(["cmake", "--build", str(output), "--target", target,
                    "--parallel", str(min(os.cpu_count() or 2, 8))], check=True)


def archive_path(directory, name):
    candidates = [directory / f"{name}.lib", directory / f"lib{name}.a"]
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    raise RuntimeError(f"Static library {name} was not built in {directory}")


def build_windows(output, compiler, sanitize, jpeg_source, tiff_source):
    """Return TIFF headers/config and JPEG/TIFF archives; TIFF includes zlib objects."""
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    for command in ("cmake", "ninja", compiler[0]):
        if not shutil.which(command):
            raise RuntimeError(f"Required Windows codec build tool unavailable: {command}")
    zlib = zlib_source()
    configuration = json.dumps({"compiler": compiler,
        "version": subprocess.check_output([*compiler, "--version"], text=True),
        "sanitize": sanitize, "zlib": ZLIB_SHA256,
        "jpeg": str(jpeg_source), "tiff": str(tiff_source)}, sort_keys=True)
    marker = output / "windows-codec-configuration.json"
    if not marker.exists() or marker.read_text() != configuration:
        for name in ("jpeg-cmake", "jpeg-windows", "zlib-cmake", "zlib-windows", "tiff-windows", "tiff-zlib-objects"):
            shutil.rmtree(output / name, ignore_errors=True)
        marker.write_text(configuration)
    jpeg_project = output / "jpeg-cmake"
    jpeg_project.mkdir(exist_ok=True)
    listing = (jpeg_source / "makefile.vc").read_text()
    match = re.search(r"^LIBSOURCES=(.*?)(?=^#)", listing, re.M | re.S)
    if match is None:
        raise RuntimeError("Pinned IJG makefile lacks library source list")
    sources = re.findall(r"\b\w+\.c\b", match.group(1)) + ["jmemnobs.c"]
    for header in jpeg_source.glob("*.h"):
        shutil.copy2(header, jpeg_project / header.name)
    shutil.copy2(jpeg_source / "jconfig.vc", jpeg_project / "jconfig.h")
    source_list = "\n".join(f'"{(jpeg_source / name).as_posix()}"' for name in sources)
    (jpeg_project / "CMakeLists.txt").write_text(
        'cmake_minimum_required(VERSION 3.16)\nproject(katla_ijg C)\n'
        f'add_library(jpeg STATIC\n{source_list}\n)\n'
        'target_include_directories(jpeg PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")\n')
    jpeg_build = output / "jpeg-windows"
    cmake_build(jpeg_project, jpeg_build, compiler, sanitize, [], "jpeg")
    jpeg_archive = archive_path(jpeg_build, "jpeg")
    # zlib's CMake may generate headers next to its inputs; never mutate verified sources.
    zlib_copy = output / "zlib-cmake"
    if not zlib_copy.exists():
        shutil.copytree(zlib, zlib_copy)
    zlib_build = output / "zlib-windows"
    cmake_build(zlib_copy, zlib_build, compiler, sanitize,
                ["-DZLIB_BUILD_TESTING=OFF", "-DZLIB_BUILD_SHARED=OFF", "-DZLIB_BUILD_STATIC=ON"], "zlibstatic")
    zlib_archive = archive_path(zlib_build, "z")
    tiff_build = output / "tiff-windows"
    arguments = ["-DBUILD_SHARED_LIBS=OFF", "-Dtiff-static=ON", "-Dtiff-tools=OFF",
        "-Dtiff-tests=OFF", "-Dtiff-contrib=OFF", "-Dtiff-docs=OFF", "-Dtiff-install=OFF",
        "-Dcxx=OFF", "-Djpeg=ON", "-Djpeg-prefer-standard=ON", "-Dzlib=ON",
        "-Dlibdeflate=OFF", "-Djbig=OFF", "-Dlerc=OFF", "-Dlzma=OFF", "-Dzstd=OFF", "-Dwebp=OFF",
        f"-DJPEG_LIBRARY={jpeg_archive.as_posix()}", f"-DJPEG_INCLUDE_DIR={jpeg_project.as_posix()}",
        f"-DZLIB_LIBRARY={zlib_archive.as_posix()}", f"-DZLIB_INCLUDE_DIR={zlib_copy.as_posix()}"]
    cmake_build(tiff_source, tiff_build, compiler, sanitize, arguments, "tiff")
    config = (tiff_build / "libtiff/tiffconf.h").read_text()
    if not re.search(r"#define\s+JPEG_SUPPORT\s+1", config) or not re.search(r"#define\s+ZIP_SUPPORT\s+1", config):
        raise RuntimeError("Native TIFF build requires actual JPEG and Deflate support")
    tiff_archive = archive_path(tiff_build / "libtiff", "tiff")
    archiver = shlex.split(os.environ.get("AR", "llvm-ar"))
    combined = output / "libtiff-zlib.a"
    objects = []
    extract_root = output / "tiff-zlib-objects"
    shutil.rmtree(extract_root, ignore_errors=True)
    for index, archive in enumerate((tiff_archive, zlib_archive)):
        directory = extract_root / str(index)
        directory.mkdir(parents=True)
        subprocess.run([*archiver, "x", str(archive)], cwd=directory, check=True)
        objects.extend(sorted(item for item in directory.iterdir() if item.suffix in (".o", ".obj")))
    if not objects:
        raise RuntimeError("No TIFF/zlib native objects extracted")
    combined.unlink(missing_ok=True)
    subprocess.run([*archiver, "rcs", str(combined), *map(str, objects)], check=True)
    return tiff_source / "libtiff", tiff_build / "libtiff", jpeg_archive, combined
