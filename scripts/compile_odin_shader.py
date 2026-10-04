#!/usr/bin/env python3
"""Export one selected WGSL entry with the independent offline compiler executable."""

import argparse
import json
from pathlib import Path
import struct
import subprocess
import tempfile


def compile_entry(compiler: Path, source: str, name: str, stage: str) -> dict:
    request = json.dumps({"abi": 1, "source": source, "selections": [{"name": name, "stage": stage}]}).encode()
    if len(request) > 8 * 1024 * 1024:
        raise RuntimeError("Compiler request exceeds its byte limit")
    with tempfile.TemporaryDirectory(prefix="katla-shader-export-") as directory:
        request_path = Path(directory) / "request.json"
        output_path = Path(directory) / "artifact.json"
        request_path.write_bytes(request)
        subprocess.run([str(compiler.resolve()), "--request", str(request_path), "--output", str(output_path)], check=True, timeout=30)
        if not 0 < output_path.stat().st_size <= 128 * 1024 * 1024:
            raise RuntimeError("Compiler returned an invalid artifact size")
        reply = json.loads(output_path.read_bytes())
    if reply["abi"] != 1 or reply["error"] != "None":
        raise RuntimeError(f'{reply["error"]}: {reply["message"]}')
    if len(reply["entries"]) != 1 or reply["entries"][0]["name"] != name or reply["entries"][0]["stage"] != stage:
        raise RuntimeError("Compiler selection does not match requested entry")
    return reply


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--compiler", required=True, type=Path)
    parser.add_argument("--entry", required=True)
    parser.add_argument("--stage", required=True, choices=("Vertex", "Fragment", "Compute"))
    parser.add_argument("--output", required=True, type=Path, help="Output basename for .json, .metal and .spv")
    args = parser.parse_args()
    reply = compile_entry(args.compiler, args.source.read_text(), args.entry, args.stage)
    entry = reply["entries"][0]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.with_suffix(".json").write_text(json.dumps(reply, indent=2) + "\n")
    args.output.with_suffix(".metal").write_text(entry["metal_source"])
    args.output.with_suffix(".spv").write_bytes(struct.pack(f'<{len(entry["spirv"])}I', *entry["spirv"]))
    print(f'Exported {args.stage} {args.entry} (Metal {entry["metal_name"]}): {args.output}')


if __name__ == "__main__":
    main()
