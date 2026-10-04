#!/usr/bin/env python3
"""Two real stdio proxy processes operate one existing CPU Authoring owner over private MCP."""
import argparse
import json
import os
from pathlib import Path
import selectors
import subprocess
import tempfile
import time

from build_katla_odin import cpu_test_environment
from odin_validation_manifest import validation_manifest

ROOT = Path(__file__).resolve().parents[1]
META = {"io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {}}
PRIMARY_TOOLS = frozenset({
    "add_component", "animation", "behavior", "create_resource", "destroy_entity",
    "duplicate_entity", "editor_view", "get_component_attributes", "get_scene_hierarchy",
    "list_available_components", "list_resources", "load_scene", "material", "prefab",
    "query_entities", "read_resource", "save_scene", "search_assets", "set_field",
    "set_parent", "simulation", "spawn_entity", "spawn_model", "trigger", "write_resource",
})


class Client:
    def __init__(self, binary, endpoint, environment=None):
        self.process = subprocess.Popen([str(binary), str(endpoint)], env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.buffer = bytearray()
        self.number = 0

    def request(self, method, **params):
        self.number += 1
        identifier = str(9007199254740992 + self.number)
        params["_meta"] = META
        self.process.stdin.write(json.dumps({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params}).encode() + b"\n")
        self.process.stdin.flush()
        deadline = time.monotonic() + 5
        while b"\n" not in self.buffer:
            with selectors.DefaultSelector() as selector:
                selector.register(self.process.stdout, selectors.EVENT_READ)
                assert selector.select(max(0, deadline - time.monotonic())), "MCP response deadline"
            data = os.read(self.process.stdout.fileno(), 65536)
            assert data, "MCP proxy closed unexpectedly"
            self.buffer.extend(data)
            assert len(self.buffer) <= 32 << 20
        line, _, remainder = self.buffer.partition(b"\n")
        self.buffer = bytearray(remainder)
        result = json.loads(line)
        assert result["id"] == identifier and "error" not in result, result
        assert result["result"].get("isError") is not True, result
        return result["result"]

    def tool(self, tool_name, **arguments):
        return self.request("tools/call", name=tool_name, arguments=arguments)["structuredContent"]

    def close(self):
        self.process.stdin.close()
        assert self.process.wait(timeout=3) == 0, self.process.stderr.read()
        assert self.process.stdout.read() == b""


def run(sanitize=False, manifest=None):
    with tempfile.TemporaryDirectory(prefix="katla-mcp-world-") as directory:
        private = Path(directory)
        environment = cpu_test_environment(os.environ, private, sanitize)
        odin = manifest["odin"] if manifest else "odin"
        endpoint = private / "editor.sock"
        project = private / "project"
        (project / "resources").mkdir(parents=True)
        world = private / "world"
        proxy = private / "katla-mcp"
        for package, binary in [("odin/examples/mcp_socket_authoring", world), ("odin/mcp_proxy", proxy)]:
            command = [odin, "build", package, "-vet", "-strict-style", f"-out:{binary}"]
            if manifest:
                command += manifest["foreign_defines"]
            if sanitize:
                command.append("-sanitize:address")
            subprocess.run(command, cwd=ROOT, env=environment, check=True)
        owner = subprocess.Popen([str(world), str(endpoint), str(project)], env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            started = time.monotonic()
            while not endpoint.exists():
                assert owner.poll() is None, owner.stderr.read()
                assert time.monotonic() - started < 5
                time.sleep(0.005)
            assert endpoint.stat().st_mode & 0o777 == 0o600
            one = Client(proxy, endpoint, environment)
            assert one.request("server/discover")["supportedVersions"] == ["2026-07-28"]
            names = {tool["name"] for tool in one.request("tools/list")["tools"]}
            assert PRIMARY_TOOLS <= names and names == PRIMARY_TOOLS | {"remove_component"}
            assert names == {tool["name"] for tool in json.loads((ROOT / "odin/agent/tools.json").read_text())}
            first = one.tool("spawn_entity", name="ÅNGSTRÖM shared sphere", position=[1, 2, 3], rotation=[0, 90, 0], scale=[1, 1, 1], shape="sphere")["entity_ids"][0]
            second = one.tool("duplicate_entity", entity_id=first, position_offset=[2, 0, 0])["entity_ids"][0]
            assert first != second and first.isdecimal() and second.isdecimal()
            assert set(one.tool("query_entities")["entity_ids"]) == {first, second}
            queried = one.tool("query_entities", name_filter="ångström", limit=1)["data"]
            assert queried["total"] == 2 and queried["truncated"] is True and len(queried["entities"]) == 1
            row = queried["entities"][0]
            assert row["entity_id"] == first and row["name"] == "ÅNGSTRÖM shared sphere" and row["parent_id"] is None
            assert row["position"] == [1, 2, 3] and row["bounds"]["center"] == [1, 2, 3]
            assert all(value > 0 for value in row["bounds"]["extent"])
            assert row["components"] == sorted(row["components"]) and "SceneMesh" in row["components"]
            assert len(one.tool("query_entities", limit=256)["entity_ids"]) == 2
            one.tool("create_resource", path="resources/shared.txt", content="first content")
            one.tool("write_resource", path="resources/shared.txt", content="replacement content")
            assert (project / "resources/shared.txt").read_text() == "replacement content"
            two = Client(proxy, endpoint, environment)
            assert set(two.tool("query_entities")["entity_ids"]) == {first, second}
            assert len(two.tool("query_entities", position=[1, 2, 3], radius=0.1)["entity_ids"]) == 1
            hierarchy = two.tool("get_scene_hierarchy")["data"]["entities"]
            assert {row["id"] for row in hierarchy} == {first, second}
            one.close()
            assert set(two.tool("query_entities")["entity_ids"]) == {first, second}, "Disconnect must retain shared world"
            two.close()
            assert owner.wait(timeout=5) == 0, owner.stderr.read().decode()
            assert not endpoint.exists(), "Only owned listener endpoint must be cleaned up"
            assert owner.stdout.read() == b"", "MCP-owner lifecycle output must never contaminate protocol stdout"
            print("PASS real two-process stdio/MCP socket: frozen primary 25-tool contract plus Odin remove_component, exact string IDs, actual primitive/duplicate/spatial query/hierarchy/resource writes, shared world survives independent disconnect and captured-owner cleanup")
        finally:
            if owner.poll() is None:
                owner.terminate(); owner.wait(timeout=3)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument("--build-manifest", type=Path, help="Reuse verified canonical dependency paths, compiler and sanitizer mode")
    args = parser.parse_args()
    manifest = validation_manifest(parser, args.build_manifest, args.sanitize, [])
    run(args.sanitize, manifest)
