"""Build pinned TIFF/JPEG sources with the same compiler/runtime as the image decoder."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
SOURCES = {
    "jpeg-9f": ("https://www.ijg.org/files/jpegsrc.v9f.tar.gz", "04705c110cb2469caa79fb71fba3d7bf834914706e9641a4589485c1f832565b"),
    "tiff-4.7.2": ("https://download.osgeo.org/libtiff/tiff-4.7.2.tar.xz", "4996f0c4f93094719b1ca5c6279b20e588773ba8a247533e486416fb662ddb88"),
}


def source(name):
    url, digest = SOURCES[name]
    cache = ROOT / "target/image-codec-source"
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / url.rsplit("/", 1)[1]
    if not archive.exists():
        with urllib.request.urlopen(url, timeout=60) as response:
            archive.write_bytes(response.read())
    if hashlib.sha256(archive.read_bytes()).hexdigest() != digest:
        raise RuntimeError(f"Pinned image source digest mismatch: {archive}")
    with tarfile.open(archive) as package:
        folder = cache / name
        if not folder.exists():
            package.extractall(cache, filter="data")
        for entry in package.getmembers():
            if entry.isfile():
                actual = cache / entry.name
                expected = package.extractfile(entry).read()
                if not actual.is_file() or hashlib.sha256(actual.read_bytes()).digest() != hashlib.sha256(expected).digest():
                    raise RuntimeError(f"Pinned image source has been modified: {actual}")
    return cache / name


def build(output, compiler, sanitize):
    jpeg_source = source("jpeg-9f")
    tiff_source = source("tiff-4.7.2")
    if platform.system() == "Windows":
        from image_tiff_windows import build_windows
        return build_windows(output, compiler, sanitize, jpeg_source, tiff_source)
    common_flags = "-O2 -fPIC" + (" -fsanitize=address -fno-omit-frame-pointer -g" if sanitize else "")
    configuration = json.dumps({"compiler": compiler, "version": subprocess.check_output([*compiler, "--version"], text=True), "flags": common_flags, "sources": SOURCES}, sort_keys=True)
    marker = output / "codec-configuration.json"
    if not marker.exists() or marker.read_text() != configuration:
        for name in ("jpeg", "tiff", "jpeg-objects", "tiff-objects"):
            shutil.rmtree(output / name, ignore_errors=True)
        marker.write_text(configuration)
    environment = os.environ.copy()
    environment["CC"] = shlex.join(compiler)
    environment["CFLAGS"] = common_flags
    environment["LDFLAGS"] = "-fsanitize=address" if sanitize else ""
    jpeg = output / "jpeg"
    jpeg.mkdir(exist_ok=True)
    if not (jpeg / "Makefile").exists():
        subprocess.run([str(jpeg_source / "configure"), "--disable-shared", "--enable-static", "--disable-dependency-tracking"], cwd=jpeg, env=environment, stdout=subprocess.DEVNULL, check=True)
    subprocess.run(["make", "-j", str(min(os.cpu_count() or 2, 8)), "libjpeg.la"], cwd=jpeg, env=environment, stdout=subprocess.DEVNULL, check=True)
    tiff = output / "tiff"
    tiff.mkdir(exist_ok=True)
    if not (tiff / "Makefile").exists():
        arguments = [str(tiff_source / "configure"), "--disable-shared", "--enable-static", "--disable-tools", "--disable-tests", "--disable-contrib", "--disable-cxx", "--disable-dependency-tracking", "--disable-jbig", "--disable-lerc", "--disable-lzma", "--disable-zstd", "--disable-webp", "--disable-libdeflate", f"--with-jpeg-include-dir={jpeg_source}", f"--with-jpeg-lib-dir={jpeg / '.libs'}"]
        environment["CPPFLAGS"] = f"-I{jpeg}"
        subprocess.run(arguments, cwd=tiff, env=environment, stdout=subprocess.DEVNULL, check=True)
    subprocess.run(["make", "-C", "libtiff", "-j", str(min(os.cpu_count() or 2, 8)), "libtiff.la"], cwd=tiff, env=environment, stdout=subprocess.DEVNULL, check=True)
    configuration = (tiff / "libtiff/tiffconf.h").read_text()
    if "#define JPEG_SUPPORT 1" not in configuration or "#define ZIP_SUPPORT 1" not in configuration:
        raise RuntimeError("Native TIFF build requires actual JPEG and zlib Deflate support")
    return tiff_source / "libtiff", tiff / "libtiff", jpeg / ".libs/libjpeg.a", tiff / "libtiff/.libs/libtiff.a"
