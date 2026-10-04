"""Resolve validators' dependencies from an exact canonical build receipt."""
from pathlib import Path

from build_katla_odin import foreign_defines
from run_katla_odin import verified_manifest


def validation_manifest(parser, filename, sanitize, overrides):
    if filename is None:
        return None
    filename = filename.resolve()
    if filename.name != "build.json":
        parser.error("--build-manifest must name the canonical build.json receipt")
    try:
        manifest = verified_manifest(filename.parent, sanitize)
        paths = manifest["paths"]
        defines = foreign_defines(paths)
        if manifest.get("foreign_defines") != defines:
            raise RuntimeError("Build manifest foreign defines disagree with its dependency paths")
        for name in ("AUDIO_LIBRARY", "CGLTF_LIBRARY", "STB_IMAGE_LIBRARY", "TOML_LIBRARY",
                     "BOX3D_LIBRARY", "LUAU_LIBRARY", "FONT_LIBRARY", "SHADER_COMPILER"):
            path = Path(paths[name])
            if not path.is_absolute() or str(path) not in manifest["artifact_sha256"]:
                raise RuntimeError(f"Build manifest dependency is not a verified absolute artifact: {name}")
        for option, name, value in overrides:
            if value is not None and value.resolve() != Path(paths[name]).resolve():
                raise RuntimeError(f"{option} disagrees with --build-manifest {name}")
    except (RuntimeError, ValueError, OSError, KeyError, TypeError, AttributeError) as error:
        parser.error(str(error))
    print(f"Using verified {'ASan' if sanitize else 'normal'} build manifest: {filename}", flush=True)
    return manifest
