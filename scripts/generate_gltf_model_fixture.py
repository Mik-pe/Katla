#!/usr/bin/env python3
"""Generate self-contained native mirror/normal and linear unlit blend fixtures."""
import base64
import json
from pathlib import Path
import struct
import zlib

ROOT = Path(__file__).resolve().parents[1]


def main():
    payload = bytearray()
    views, accessors = [], []

    def accessor(values, shape, components, scalar="f", component=5126):
        while len(payload) % 4:
            payload.append(0)
        offset = len(payload)
        flat = [part for value in values for part in value] if components > 1 else values
        payload.extend(struct.pack(f"<{len(flat)}{scalar}", *flat))
        views.append({"buffer": 0, "byteOffset": offset, "byteLength": len(payload) - offset})
        result = {"bufferView": len(views)-1, "componentType": component, "count": len(values), "type": shape}
        if shape == "VEC3":
            result.update(min=[min(value[i] for value in values) for i in range(3)], max=[max(value[i] for value in values) for i in range(3)])
        accessors.append(result)
        return len(accessors)-1

    position = accessor([(-1,-1,0),(1,-1,0),(1,1,0),(-1,1,0)], "VEC3", 3)
    normal = accessor([(0,0,1)]*4, "VEC3", 3)
    tangent = accessor([(1,0,0,1)]*4, "VEC4", 4)
    uv = accessor([(0,0),(1,0),(1,1),(0,1)], "VEC2", 2)
    indices = accessor([0,1,2,0,2,3], "SCALAR", 1, "H", 5123)

    def chunk(kind, value):
        return struct.pack(">I", len(value))+kind+value+struct.pack(">I", zlib.crc32(kind+value)&0xffffffff)

    png = b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR", struct.pack(">IIBBBBB",2,2,8,6,0,0,0))
    png += chunk(b"IDAT", zlib.compress((b"\0"+bytes([128,218,218,255])*2)*2))+chunk(b"IEND",b"")
    common = {
        "asset": {"version":"2.0", "generator":"Katla native model correctness fixture"},
        "buffers":[{"byteLength":len(payload),"uri":"data:application/octet-stream;base64,"+base64.b64encode(payload).decode()}],
        "bufferViews":views,"accessors":accessors,
        "images":[{"uri":"data:image/png;base64,"+base64.b64encode(png).decode()}],
        "textures":[{"source":0}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":position,"NORMAL":normal,"TANGENT":tangent,"TEXCOORD_0":uv},"indices":indices,"material":0}]}],
        "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0,
    }
    for name, material, unlit in (
        ("MirrorNormal.gltf", {"pbrMetallicRoughness":{"baseColorFactor":[.5,.5,.5,1],"metallicFactor":0,"roughnessFactor":1},"normalTexture":{"index":0}},False),
        ("UnlitBlend.gltf", {"pbrMetallicRoughness":{"baseColorFactor":[.5,.5,.5,.5],"metallicFactor":1,"roughnessFactor":0},"normalTexture":{"index":0},"emissiveFactor":[1,0,0],"emissiveTexture":{"index":0},"alphaMode":"BLEND","extensions":{"KHR_materials_unlit":{}}},True),
    ):
        asset = dict(common, materials=[material])
        if unlit:
            asset.update(extensionsUsed=["KHR_materials_unlit"],extensionsRequired=["KHR_materials_unlit"])
        output=ROOT/"resources/models"/name
        output.write_text(json.dumps(asset,indent=2)+"\n")
        print(output)
    back_position = accessor([(-1,-1,-1),(1,-1,-1),(1,1,-1),(-1,1,-1)], "VEC3", 3)
    layered = dict(common)
    layered["buffers"] = [{"byteLength":len(payload),"uri":"data:application/octet-stream;base64,"+base64.b64encode(payload).decode()}]
    layered.update(extensionsUsed=["KHR_materials_unlit"],extensionsRequired=["KHR_materials_unlit"])
    layered["materials"] = [{"pbrMetallicRoughness":{"baseColorFactor":color},"alphaMode":"BLEND","extensions":{"KHR_materials_unlit":{}}} for color in ([1,0,0,.5],[0,0,1,.5])]
    layered["meshes"] = [{"primitives":[{"attributes":{"POSITION":p,"NORMAL":normal,"TANGENT":tangent,"TEXCOORD_0":uv},"indices":indices,"material":i} for i,p in enumerate((position,back_position))]}]
    output=ROOT/"resources/models/BlendDepth.gltf"
    output.write_text(json.dumps(layered,indent=2)+"\n")
    print(output)


if __name__ == "__main__":
    main()
