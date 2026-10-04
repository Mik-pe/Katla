#!/usr/bin/env python3
"""Exercise the optional Odin MCP subprocess over actual stdin/stdout pipes."""
import argparse
import json
from pathlib import Path
import queue
import subprocess
import threading
import tempfile

ROOT = Path(__file__).resolve().parents[1]
VERSION = "2026-07-28"
META = {"io.modelcontextprotocol/protocolVersion": VERSION,
        "io.modelcontextprotocol/clientCapabilities": {}}


def request(identifier, method, **params):
    return {"jsonrpc": "2.0", "id": identifier, "method": method,
            "params": {"_meta": META, **params}}


class Client:
    def __init__(self, binary, roots=()):
        self.process = subprocess.Popen([str(binary), *map(str, roots)], cwd=ROOT, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.lines = queue.Queue()
        self.reader = threading.Thread(target=self.read_output, daemon=True)
        self.reader.start()

    def read_output(self):
        for line in self.process.stdout:
            try:
                self.lines.put(json.loads(line))
            except Exception as error:
                self.lines.put(error)
        self.lines.put(None)

    def raw(self, data):
        self.process.stdin.write(data)
        self.process.stdin.flush()

    def send(self, message):
        self.raw(json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode() + b"\n")

    def read(self):
        message = self.lines.get(timeout=5)
        if isinstance(message, Exception):
            raise message
        assert message is not None, "Server ended before delivering a reply"
        assert message["jsonrpc"] == "2.0"
        if "result" in message:
            assert message["result"]["resultType"] == "complete"
            assert message["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"] == "katla-odin"
        return message

    def call(self, identifier, tool_name, **arguments):
        self.send(request(identifier, "tools/call", name=tool_name, arguments=arguments))
        message = self.read()
        assert message["id"] == identifier and type(message["id"]) is type(identifier)
        assert not message.get("result", {}).get("isError"), message
        return message["result"]["structuredContent"]

    def finish(self):
        self.process.stdin.close()
        code = self.process.wait(timeout=5)
        stderr = self.process.stderr.read().decode()
        self.reader.join(timeout=5)
        assert code == 0 and not stderr, (code, stderr)
        assert self.lines.get(timeout=1) is None, "Unexpected message after completed requests"
        self.process.stdout.close()
        self.process.stderr.close()

    def abort(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=5)
        for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
            if not stream.closed:
                stream.close()


def acceptance(binary):
    client = Client(binary)
    try:
        client.send(request("discover", "server/discover"))
        discovery = client.read()["result"]
        assert discovery["supportedVersions"] == [VERSION]
        assert discovery["capabilities"] == {"tools": {"listChanged": False}}
        client.send(request(0, "tools/list"))
        tools = client.read()["result"]["tools"]
        names = [tool["name"] for tool in tools]
        assert names == sorted(names) and len(set(names)) == 18
        assert all(tool["inputSchema"]["type"] == "object" for tool in tools)
        assert "editor_view" not in names and "spawn_model" not in names
        # Fragment a real frame at arbitrary byte boundaries, including UTF-8.
        spawn = request(9007199254740993, "tools/call", name="spawn_entity",
                        arguments={"name": "Study / stol 🪑", "position": [1, 2, 3]})
        encoded = (json.dumps(spawn, ensure_ascii=False) + "\n").encode()
        for offset in range(0, len(encoded), 7):
            client.raw(encoded[offset:offset + 7])
        spawned = client.read()
        assert spawned["id"] == 9007199254740993
        entity = spawned["result"]["structuredContent"]["entity_ids"][0]
        position = client.call("position", "get_component_attributes", entity_id=entity, component="SceneTransform")
        assert position["data"]["local"]["position"] == [1, 2, 3]
        name = client.call("name", "get_component_attributes", entity_id=entity, component="SceneName")
        assert name["data"]["name"] == "Study / stol 🪑"
        # The actual application service changes and reads the same component.
        client.call("surface", "add_component", entity_id=entity, component="SurfaceMaterial")
        edited = client.call("material", "material", action="set", entity_ids=[entity], preset="oak", roughness=0.27)
        assert abs(edited["data"]["materials"][0]["values"]["roughness"] - 0.27) < 1e-6
        inspected = client.call("inspect", "material", action="inspect", entity_id=entity)
        assert abs(inspected["data"]["values"]["roughness"] - 0.27) < 1e-6
        # Interleave unrelated callers and distinguish integer/string ID identity.
        pipeline = [request(i, "tools/call", name="query_entities", arguments={"limit": 256}) for i in range(24)]
        pipeline.append(request("0", "tools/call", name="list_available_components"))
        client.raw(b"".join(json.dumps(message).encode() + b"\n" for message in pipeline))
        replies = [client.read() for _ in pipeline]
        assert {(type(reply["id"]), reply["id"]) for reply in replies} == {(type(message["id"]), message["id"]) for message in pipeline}
        assert all(not reply["result"]["isError"] for reply in replies)
        client.send(request("invalid", "tools/call", name="spawn_entity", arguments={"shape": "cube"}))
        assert client.read()["result"]["isError"]
        client.send(request("stale", "tools/call", name="destroy_entity", arguments={"entity_id": "18446744073709551615"}))
        assert client.read()["result"]["isError"]
        client.send(request("missing", "tools/call", name="spawn_model"))
        assert client.read()["error"]["code"] == -32602
        client.raw(b'{"jsonrpc":"2.0","id":3,"method":"ping","params":{}}\n')
        assert client.read()["error"]["code"] == -32602
        wrong = request(4, "ping")
        wrong["params"]["_meta"] = {**META, "io.modelcontextprotocol/protocolVersion": "2025-11-25"}
        client.send(wrong)
        assert client.read()["error"]["code"] == -32022
        client.raw(b'{} {}\n')
        assert client.read()["error"]["code"] == -32700
        client.raw(b'x' * ((1 << 20) + 1) + b'\n')
        assert client.read()["error"]["code"] == -32700
        client.call("after-errors", "query_entities", limit=256)
        client.call("delete", "destroy_entity", entity_id=entity)
        assert client.call("empty", "query_entities")["entity_ids"] == []
        client.finish()
    finally:
        client.abort()
    # Every application tool uses the same confined roots and canonical scene owner.
    with tempfile.TemporaryDirectory(prefix="katla-mcp-assets-") as directory:
        project = Path(directory)
        resource = project / "resources"
        resource.mkdir()
        recipe = '(version:1,name:"Chair",parts:[(id:"seat",geometry:(kind:"cube",size:(2,1,1)))])'
        (resource / "chair.katmesh").write_text(recipe)
        client = Client(binary, (project, resource))
        try:
            found = client.call("assets", "search_assets", query="chair", extensions=["katmesh"])["data"]
            assert found["assets"] == ["chair.katmesh"] and found["project_paths"] == ["resources/chair.katmesh"]
            assert client.call("read", "read_resource", path="resources/chair.katmesh")["data"]["content"] == recipe
            client.send(request("escape", "tools/call", name="read_resource", arguments={"path":"../outside"}))
            assert client.read()["result"]["isError"]
            instantiated = client.call("instantiate", "prefab", action="instantiate", path="resources/chair.katmesh", name="Agent chair", position=[3,0,1])
            chair = instantiated["entity_ids"][0]
            mesh = client.call("mesh", "get_component_attributes", entity_id=chair, component="SceneMesh")["data"]
            assert mesh["kind"] == "Recipe" and mesh["path"] == "resources/chair.katmesh" and mesh["root"] == "project"
            client.call("simulation", "simulation", action="inspect")
            client.call("behavior", "behavior", action="inspect", entity_id=chair)
            trigger = client.call("trigger-create", "trigger", action="create_box", name="Door sensor", position=[0,1,0], half_extents=[1,1,1], rules=[])["entity_ids"][0]
            client.call("triggers", "trigger", action="inspect", entity_id=trigger)
            client.call("remove-trigger", "destroy_entity", entity_id=trigger)
            client.call("remove-chair", "destroy_entity", entity_id=chair)
            assert client.call("asset-empty", "query_entities")["entity_ids"] == []
            client.finish()
        finally:
            client.abort()
    # Cancellation is allowed to race acceptance; it must suppress later replies,
    # and its explicit mutation boundary is reflected in the following scene query.
    client = Client(binary)
    try:
        batch = [request("cancelled", "tools/call", name="spawn_entity"),
                 {"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": "cancelled"}},
                 request("cancel-barrier", "tools/call", name="query_entities")]
        client.raw(b"".join(json.dumps(message).encode() + b"\n" for message in batch))
        first = client.read()
        if first["id"] == "cancelled":
            first = client.read()
        assert first["id"] == "cancel-barrier"
        ids = first["result"]["structuredContent"]["entity_ids"]
        assert len(ids) <= 1
        client.send(request("after-cancel", "ping"))
        assert client.read()["id"] == "after-cancel"
        for entity in ids:
            client.call("cleanup", "destroy_entity", entity_id=entity)
        assert client.call("cancel-empty", "query_entities")["entity_ids"] == []
        client.finish()
    finally:
        client.abort()
    # Closing input drains a burst without requiring another read from stdin.
    client = Client(binary)
    try:
        burst = [request(f"eof-{i}", "tools/call", name="spawn_entity") for i in range(40)]
        client.raw(b"".join(json.dumps(message).encode() + b"\n" for message in burst))
        client.process.stdin.close()
        replies = [client.read() for _ in burst]
        assert {reply["id"] for reply in replies} == {message["id"] for message in burst}
        assert client.process.wait(timeout=5) == 0
        assert not client.process.stderr.read()
    finally:
        client.abort()
    # Empty and partial EOF terminate promptly and emit only valid messages.
    for data, expected in [(b"", 0), (b'{"jsonrpc":', 1)]:
        completed = subprocess.run([str(binary)], input=data, capture_output=True, timeout=5, cwd=ROOT)
        assert completed.returncode == 0 and not completed.stderr
        messages = [json.loads(line) for line in completed.stdout.splitlines()]
        assert len(messages) == expected
        if expected:
            assert messages[0]["error"]["code"] == -32700
    # A client that closes stdout must not leave the input thread holding the process open.
    process = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=ROOT)
    try:
        process.stdout.close()
        process.stdin.write(json.dumps(request("broken-output", "ping")).encode() + b"\n")
        process.stdin.flush()
        assert process.wait(timeout=5) != 0
    finally:
        if process.poll() is None:
            process.kill()
        process.wait(timeout=5)
        process.stdin.close()
        process.stderr.close()
    print("Odin MCP: discovery, 18 tool schemas, actual scene/material edits, typed IDs, pipelining, bounded input, recovery and EOF/output failure passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/katla-odin-mcp")
    args = parser.parse_args()
    acceptance(args.binary.resolve())


if __name__ == "__main__":
    main()
