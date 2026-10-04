#!/usr/bin/env python3
"""Export one real selected WGSL entry through the versioned Naga compiler ABI."""

import argparse
import ctypes
import json
from pathlib import Path
import struct


class Buffer(ctypes.Structure):
    _fields_ = [("data", ctypes.c_void_p), ("length", ctypes.c_size_t)]


def compile_entry(library: Path, source: str, name: str, stage: str) -> dict:
    compiler = ctypes.CDLL(str(library.resolve()))
    compiler.katla_naga_abi.restype = ctypes.c_uint32
    if compiler.katla_naga_abi() != 1:
        raise RuntimeError("Compiler dependency ABI mismatch")
    compiler.katla_naga_compile.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    compiler.katla_naga_compile.restype = Buffer
    compiler.katla_naga_free.argtypes = [Buffer]
    request = json.dumps({"abi": 1, "source": source, "selections": [{"name": name, "stage": stage}]}).encode()
    if len(request) > 8 * 1024 * 1024:
        raise RuntimeError("Compiler request exceeds ABI byte limit")
    result = compiler.katla_naga_compile(request, len(request))
    try:
        if not result.data or not 0 < result.length <= 128 * 1024 * 1024:
            raise RuntimeError("Compiler returned an invalid ABI buffer")
        reply = json.loads(ctypes.string_at(result.data, result.length))
    finally:
        compiler.katla_naga_free(result)
    if reply["abi"] != 1 or reply["error"] != "None":
        raise RuntimeError(f'{reply["error"]}: {reply["message"]}')
    if len(reply["entries"]) != 1 or reply["entries"][0]["name"] != name or reply["entries"][0]["stage"] != stage:
        raise RuntimeError("Compiler selection does not match requested entry")
    return reply


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--library", required=True, type=Path)
    parser.add_argument("--entry", required=True)
    parser.add_argument("--stage", required=True, choices=("Vertex", "Fragment", "Compute"))
    parser.add_argument("--output", required=True, type=Path, help="Output basename for .json, .metal and .spv")
    args = parser.parse_args()
    reply = compile_entry(args.library, args.source.read_text(), args.entry, args.stage)
    entry = reply["entries"][0]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.with_suffix(".json").write_text(json.dumps(reply, indent=2) + "\n")
    args.output.with_suffix(".metal").write_text(entry["metal_source"])
    args.output.with_suffix(".spv").write_bytes(struct.pack(f'<{len(entry["spirv"])}I', *entry["spirv"]))
    print(f'Exported {args.stage} {args.entry} (Metal {entry["metal_name"]}): {args.output}')


if __name__ == "__main__":
    main()
