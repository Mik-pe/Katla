#!/usr/bin/env python3
"""Validate the actual running native editor through its private stdio MCP proxy."""
import argparse
import base64
import json
import os
from pathlib import Path
import selectors
import subprocess
import time

META = {"io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {}}


class Client:
    def __init__(self, binary, endpoint):
        self.process = subprocess.Popen([binary, endpoint], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.buffer = bytearray()
        self.number = 0

    def call(self, name, arguments, error=False):
        self.number += 1
        identifier = str(9007199254740992 + self.number)
        request = {"jsonrpc": "2.0", "id": identifier, "method": "tools/call", "params": {"name": name, "arguments": arguments, "_meta": META}}
        self.process.stdin.write(json.dumps(request).encode() + b"\n")
        self.process.stdin.flush()
        deadline = time.monotonic() + 20
        while b"\n" not in self.buffer:
            with selectors.DefaultSelector() as selector:
                selector.register(self.process.stdout, selectors.EVENT_READ)
                assert selector.select(max(0, deadline - time.monotonic())), "Native reply deadline"
            data = os.read(self.process.stdout.fileno(), 65536)
            assert data, "Proxy closed before reply"
            self.buffer.extend(data)
            assert len(self.buffer) <= 32 << 20, "Native reply bound"
        line, _, self.buffer = self.buffer.partition(b"\n")
        response = json.loads(line)
        assert response["id"] == identifier and "error" not in response
        result = response["result"]
        assert bool(result.get("isError", False)) == error, result.get("content", [])[:1] if error else "Native tool failed"
        return result

    def close(self):
        self.process.stdin.close()
        assert self.process.wait(timeout=5) == 0, self.process.stderr.read()
        assert self.process.stdout.read() == b""


def run(binary, endpoint, output):
    output = Path(output)
    assert output.is_dir()
    client = Client(binary, endpoint)
    receipts = []
    def view(action, **arguments):
        result = client.call("editor_view", {"action": action, **arguments})
        value = result["structuredContent"]
        image = next(block for block in result["content"] if block["type"] == "image")
        png = base64.b64decode(image["data"], validate=True)
        assert image["mimeType"] == "image/png" and png[:8] == b"\x89PNG\r\n\x1a\n"
        assert value["image_size"] == [int.from_bytes(png[16:20]), int.from_bytes(png[20:24])]
        assert "image_png_base64" not in value and "image_png_base64" not in result["content"][0]["text"]
        assert value["gpu_provenance"]["submission_id"] == value["submission"]
        assert value["visibility_basis"] and value["camera"]["view_matrix"] and value["camera"]["projection_matrix"]
        assert int(value["capture_serial"]) > int(receipts[-1]["capture_serial"]) if receipts else True
        number = len(receipts)
        (output / f"view-{number}-{action}.png").write_bytes(png)
        (output / f"view-{number}-{action}.json").write_text(json.dumps(value))
        receipts.append(value)
        return value
    try:
        spawned = client.call("spawn_entity", {"position": [4, 0, 0], "shape": "cube", "name": "Native View Fixture"})["structuredContent"]
        entity = spawned["entity_ids"][0]
        cleared = view("select", entity_id=None)
        assert cleared["selected_entity_id"] is None
        observed = view("set_camera", position=[4, 2, 10], target=[4, 0, 0])
        assert observed["center_pick"] == entity and observed["candidate_count"] >= 1
        selected = view("select", entity_id=entity)
        assert selected["selected_entity_id"] == entity
        focused = view("focus", entity_id=entity, select=True)
        assert focused["selected_entity_id"] == entity
        unselected = view("select", entity_id=None)
        assert unselected["center_pick"] == entity
        undone = view("undo")
        assert entity not in [row["entity_id"] for row in undone["candidates"]] and undone["redo_available"]
        client.call("editor_view", {"action": "select", "entity_id": entity}, error=True)
        redone = view("redo")
        assert redone["candidate_count"] >= 1 and redone["undo_available"]
        client.call("editor_view", {"action": "set_camera", "position": [0, 1, 0], "target": [0, 0, 0]}, error=True)
        client.call("save_scene", {"path": "native-view.katla"})
        client.call("save_scene", {"path": None})
        client.call("load_scene", {"path": "native-view.katla"})
        loaded = view("observe", limit=1)
        assert loaded["candidate_count"] >= 1 and len(loaded["candidates"]) == 1
        (output / "receipt.json").write_text(json.dumps({"endpoint": endpoint, "entity": entity, "frames": [item["frame_id"] for item in receipts], "capture_serials": [item["capture_serial"] for item in receipts], "images": len(receipts)}))
        print(f"PASS actual running editor private MCP: {len(receipts)} paired GPU PNG replies, camera/selection/focus/Undo/Redo/stale-ID/save/load")
    finally:
        client.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--proxy", required=True)
    parser.add_argument("--socket", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    run(args.proxy, args.socket, args.output)
