#!/usr/bin/env python3
"""Generate a self-contained real glTF with authored and transformed-UV tangents."""
import base64
import json
from pathlib import Path
import struct
import zlib

ROOT = Path(__file__).resolve().parents[1]


def main():
    data = bytearray()
    views, accessors = [], []

    def accessor(values, shape, components, component=5126):
        while len(data) % 4:
            data.append(0)
        offset = len(data)
        scalar = "f" if component == 5126 else "H"
        flat = [part for value in values for part in value] if components > 1 else values
        data.extend(struct.pack(f"<{len(flat)}{scalar}", *flat))
        views.append({"buffer": 0, "byteOffset": offset, "byteLength": len(data) - offset})
        result = {"bufferView": len(views) - 1, "componentType": component, "count": len(values), "type": shape}
        if shape == "VEC3":
            result["min"] = [min(value[i] for value in values) for i in range(3)]
            result["max"] = [max(value[i] for value in values) for i in range(3)]
        accessors.append(result)
        return len(accessors) - 1

    uv0 = accessor([(0, 0), (1, 0), (1, 1), (0, 1)] * 2, "VEC2", 2)
    uv1 = accessor([(0, 0), (0, 1), (-1, 1), (-1, 0)] * 2, "VEC2", 2)
    normal = accessor([(0, 0, 1)] * 8, "VEC3", 3)
    tangent = accessor([(0, 1, 0, 1)] * 8, "VEC4", 4)
    indices = accessor([0, 1, 2, 0, 2, 3], "SCALAR", 1, 5123)
    right_indices = accessor([4, 5, 6, 4, 6, 7], "SCALAR", 1, 5123)
    positions = accessor([(-1.5, -1, 0), (-0.1, -1, 0), (-0.1, 1, 0), (-1.5, 1, 0), (0.1, -1, -0.5), (1.5, -1, -0.5), (1.5, 1, -0.5), (0.1, 1, -0.5)], "VEC3", 3)

    def chunk(kind, value):
        return struct.pack(">I", len(value)) + kind + value + struct.pack(">I", zlib.crc32(kind + value) & 0xffffffff)

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 2, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress((b"\0" + bytes([218, 128, 218, 255]) * 2) * 2)) + chunk(b"IEND", b"")
    asset = {
        "asset": {"version": "2.0", "generator": "Katla tangent regression fixture"},
        "extensionsUsed": ["KHR_texture_transform"], "extensionsRequired": ["KHR_texture_transform"],
        "buffers": [{"byteLength": len(data), "uri": "data:application/octet-stream;base64," + base64.b64encode(data).decode()}],
        "bufferViews": views, "accessors": accessors,
        "images": [{"uri": "data:image/png;base64," + base64.b64encode(png).decode()}],
        "textures": [{"source": 0, "sampler": 0}],
        "samplers": [{"minFilter": 9728, "magFilter": 9728, "wrapS": 10497, "wrapT": 10497}],
        "materials": [
            {"name": "Generated UV1 rotated tangent", "pbrMetallicRoughness": {"baseColorFactor": [0.5, 0.5, 0.5, 1], "metallicFactor": 0, "roughnessFactor": 1},
             "normalTexture": {"index": 0, "texCoord": 0, "extensions": {"KHR_texture_transform": {"texCoord": 1, "rotation": 1.5707963267948966, "offset": [0.2, 0.3], "scale": [2, 3]}}}},
            {"name": "Retained authored tangent", "pbrMetallicRoughness": {"baseColorFactor": [0.5, 0.5, 0.5, 1], "metallicFactor": 0, "roughnessFactor": 1}, "normalTexture": {"index": 0, "texCoord": 0}},
        ],
        "meshes": [{"primitives": [
            {"attributes": {"POSITION": positions, "NORMAL": normal, "TEXCOORD_0": uv0, "TEXCOORD_1": uv1}, "indices": indices, "material": 0},
            {"attributes": {"POSITION": positions, "NORMAL": normal, "TEXCOORD_0": uv0, "TEXCOORD_1": uv1, "TANGENT": tangent}, "indices": right_indices, "material": 1},
        ]}], "nodes": [{"mesh": 0}], "scenes": [{"nodes": [0]}], "scene": 0,
    }
    output = ROOT / "resources/models/TangentUV.gltf"
    output.write_text(json.dumps(asset, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
