#!/usr/bin/env python3
"""Exercise the optional Odin MCP subprocess over actual stdin/stdout pipes."""
import argparse
import copy
import json
import os
from pathlib import Path
from build_katla_odin import output_path
import queue
import subprocess
import threading
import struct
import zlib
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


def generation_acceptance(client, project):
    target = client.call("generated-target", "spawn_entity", name="Template consumer")["entity_ids"][0]
    for index, (description, rate) in enumerate([("FIRE",150),("rain",500),("snow",80),("spark",200),("steam",40),("sand",60),("mystic",120),("burst",300),("unknown",100)]):
        path = f"resources/generated/particles-{index}.json"
        receipt = client.call(f"generate-particle-{index}", "generate_resource", path=path, resource_type="particle_system", description=description)["data"]
        assert receipt["success"] and receipt["resource_type"] == "particle_system" and (project / path).is_file()
        text = client.call(f"generated-read-{index}", "read_resource", path=path)["data"]["content"]
        descriptor = json.loads(text)
        assert descriptor["emit_rate"] == rate
        client.call(f"generated-attach-{index}", "behavior", action="set_particles", entity_id=target, document=descriptor)
        accepted = client.call(f"generated-inspect-{index}", "behavior", action="inspect", entity_id=target)["data"]["particles"]
        assert accepted == descriptor
        client.send(request(f"generated-duplicate-{index}", "tools/call", name="generate_resource", arguments=dict(path=path, resource_type="scene", description="night")))
        assert client.read()["result"]["isError"]
        assert (project / path).read_text() == text
    client.call("generated-target-remove", "destroy_entity", entity_id=target)
    for index, (description, color, intensity) in enumerate([("night",[.05,.05,.1],.1),("sunset",[.8,.4,.2],.4),("indoor",[.9,.85,.7],.3),("daylight",[.9,.95,1],.5)]):
        path = f"assets/generated/scene-{index}.katla"
        client.call(f"generate-scene-{index}", "generate_resource", path=path, resource_type="scene", description=description)
        client.call(f"generated-load-{index}", "load_scene", path=path)
        rows = client.call(f"generated-query-{index}", "query_entities", component_filter="DirectionalLight")["entity_ids"]
        assert len(rows) == 1
        light = client.call(f"generated-light-{index}", "get_component_attributes", entity_id=rows[0], component="DirectionalLight")["data"]
        assert abs(light["intensity"]-intensity)<1e-6 and all(abs(a-b)<1e-6 for a,b in zip(light["color"],color))
    for index,path in enumerate(["../generation-escape.json","/absolute-generation.json"]):
        client.send(request(f"generated-path-{index}","tools/call",name="generate_resource",arguments=dict(path=path,resource_type="particle_system",description="fire")))
        assert client.read()["result"]["isError"]


def material_values_equal(actual, expected):
    """Serialization crosses sRGB/linear f32 conversion; discrete policy stays exact."""
    assert actual.keys() == expected.keys(), (actual, expected)
    for key, value in expected.items():
        other = actual[key]
        if isinstance(value, list):
            assert len(other) == len(value) and all(abs(a-b) < 1e-6 for a, b in zip(other, value)), (key, other, value)
        elif type(value) is float:
            assert abs(other-value) < 1e-6, (key, other, value)
        else:
            assert type(other) is type(value) and other == value, (key, other, value)


def material_acceptance(client, project, resource, entity):
    """Actual CPU material services; native texture/shader proof is a separate consumer."""
    roles = {"albedo", "normal", "metallic_roughness", "occlusion", "emission"}
    presets = client.call("material-capabilities", "material", action="presets")["data"]
    capabilities = presets["capabilities"]
    assert capabilities["batch_atomic"] and capabilities["maximum_batch_size"] == 256
    assert capabilities["sampling_editable"] and capabilities["textures_editable"]
    assert capabilities["alpha_changes_render_mode"] is False
    assert presets["base_color_space"] == "srgb" and presets["emissive_color_space"] == "linear"
    complete = dict(emissive_factor=[2, 0.25, 0], normal_scale=-1,
                    occlusion_strength=0.4, alpha_mode="mask", alpha_cutoff=0.6,
                    double_sided=True, roughness=0.31)
    client.call("material-complete", "material", action="set", entity_ids=[entity], **complete)
    inspected = client.call("material-complete-inspect", "material", action="inspect", entity_id=entity)["data"]
    for key, value in complete.items():
        actual = inspected["values"][key]
        assert abs(actual-value) < 1e-6 if type(value) is float else actual == value, (key, actual)
    patch = dict(tex_coord=0, offset=[0.25, -0.5], rotation=0.75, scale=[-2, 0],
                 minification="linear_mipmap_linear", magnification="linear",
                 wrap_u="mirrored_repeat", wrap_v="clamp_to_edge", anisotropy=4)
    changed = client.call("material-sampling", "material", action="set_sampling", entity_ids=[entity], role="albedo", patch=patch)["data"]
    assert changed["batch_atomic"] and changed["image_bindings_preserved"] and changed["rotation_unit"] == "radians"
    sampled = client.call("material-sampling-inspect", "material", action="inspect", entity_id=entity)["data"]
    assert set(sampled["sampling"]) == roles and sampled["uv_sets"][0]
    albedo = sampled["sampling"]["albedo"]
    assert albedo["uv"] == {key: patch[key] for key in ("tex_coord", "offset", "rotation", "scale")}
    assert albedo["sampler"] == {key: patch[key] for key in ("minification", "magnification", "wrap_u", "wrap_v", "anisotropy")}
    assert sampled["values"] == inspected["values"]

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind+data))
    pixels = b"\x00" + bytes([255,0,0,255,0,255,0,255]) + b"\x00" + bytes([0,0,255,255,255,255,255,255])
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 2,2,8,6,0,0,0)) + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b"")
    image = resource / "material-checker.png"
    image.write_bytes(png)
    source = {"kind":"file", "asset":{"Resource":image.name}}
    image_receipt = client.call("material-image", "material", action="set_texture", entity_ids=[entity], role="albedo", source=source)["data"]
    assert image_receipt["role"] == "albedo" and image_receipt["batch_atomic"]
    assert image_receipt["sampling_preserved"] and image_receipt["factors_preserved"]
    assert image_receipt["materials"][0]["entity_id"] == entity
    assigned = client.call("material-image-inspect", "material", action="inspect", entity_id=entity)["data"]
    assert assigned["values"] == inspected["values"] and assigned["sampling"] == sampled["sampling"]
    binding = assigned["provenance"]["authored_textures"]["albedo"]
    assert binding == image_receipt["materials"][0]["texture"]
    assert binding["image"]["width"] == 2 and binding["image"]["height"] == 2
    assert binding["image"]["decoded_format"] == "Rgba8" and binding["sampled_color_space"] == "linear"
    bound_source = binding["source"]
    assert bound_source["kind"] == "file"
    bound_asset = bound_source["asset"]
    resolved_image = resource / bound_asset["Resource"] if "Resource" in bound_asset else Path(bound_asset["File"])
    assert resolved_image.resolve() == image.resolve()

    other = client.call("material-target", "spawn_entity", name="Independent material copy")["entity_ids"][0]
    try:
        for action, arguments in [("set", {"roughness":0.8}), ("set_sampling", {"role":"albedo", "patch":{"rotation":0}}), ("set_texture", {"role":"albedo", "source":{"kind":"neutral"}})]:
            before = client.call(f"atomic-{action}-before", "material", action="inspect", entity_id=entity)["data"]
            client.send(request(f"atomic-{action}", "tools/call", name="material", arguments={"action":action,"entity_ids":[entity,"18446744073709551615"],**arguments}))
            assert client.read()["result"]["isError"]
            assert client.call(f"atomic-{action}-after", "material", action="inspect", entity_id=entity)["data"] == before
        before = client.call("failed-image-before", "material", action="inspect", entity_id=entity)["data"]
        client.send(request("failed-image", "tools/call", name="material", arguments={"action":"set_texture","entity_ids":[entity,other],"role":"albedo","source":{"kind":"file","asset":{"Resource":"missing.png"}}}))
        assert client.read()["result"]["isError"]
        assert client.call("failed-image-after", "material", action="inspect", entity_id=entity)["data"] == before
        example = client.call("material-asset-describe", "material_asset", action="describe")["data"]
        assert example["limits"]["targets"] == 256 and set(example["example"]["textures"]) == roles
        path = "materials/captured.katmat"
        captured = client.call("material-capture", "material_asset", action="capture", path=path, entity_id=entity)["data"]
        assert captured["saved"] and (project / path).is_file()
        document = client.call("material-read", "material_asset", action="read", path=path)["data"]
        assert document == captured["document"] and set(document["textures"]) == roles
        assert all(value["kind"] != "inherit" for value in document["textures"].values())
        copied = client.call("material-apply", "material_asset", action="apply", path=path, entity_ids=[other])["data"]
        assert copied["batch_atomic"] and copied["independently_editable"] and copied["entity_ids"] == [other]
        copy_values = client.call("material-copy-inspect", "material", action="inspect", entity_id=other)["data"]
        material_values_equal(copy_values["values"], before["values"])
        assert copy_values["sampling"] == before["sampling"]
        assert copy_values["provenance"]["authored_textures"]["albedo"]["image"] == before["provenance"]["authored_textures"]["albedo"]["image"]
        client.call("material-copy-edit", "material", action="set", entity_ids=[other], roughness=0.9)
        assert client.call("material-original-preserved", "material", action="inspect", entity_id=entity)["data"] == before
        revised = copy.deepcopy(document); revised["values"]["roughness"] = 0.73
        assert client.call("material-validate", "material_asset", action="validate", path=path, document=revised)["data"]["valid"]
        assert client.call("material-write", "material_asset", action="write", path=path, document=revised)["data"]["saved"]
        assert client.call("material-write-preserved", "material", action="inspect", entity_id=entity)["data"] == before
        assert abs(client.call("material-copy-preserved", "material", action="inspect", entity_id=other)["data"]["values"]["roughness"] - 0.9) < 1e-6
        client.call("material-reapply", "material_asset", action="apply", path=path, entity_ids=[other])
        assert abs(client.call("material-revision", "material", action="inspect", entity_id=other)["data"]["values"]["roughness"] - 0.73) < 1e-6
    finally:
        client.call("material-target-cleanup", "destroy_entity", entity_id=other)


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
        expected = {"add_component", "animation", "behavior", "create_resource", "destroy_entity", "duplicate_entity", "editor_view", "generate_resource", "get_component_attributes", "get_scene_hierarchy", "list_available_components", "list_resources", "load_scene", "material", "material_asset", "prefab", "query_entities", "read_resource", "remove_component", "save_scene", "search_assets", "set_field", "set_parent", "simulation", "spawn_entity", "spawn_model", "trigger", "write_resource"}
        assert names == sorted(expected)
        assert all(tool["inputSchema"]["type"] == "object" for tool in tools)
        client.send(request("no-render-owner", "tools/call", name="editor_view", arguments={"action": "observe"}))
        assert client.read()["result"]["isError"]
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
        initial_surface = client.call("initial-surface", "get_component_attributes", entity_id=entity, component="SurfaceMaterial")
        assert initial_surface["data"]["roughness"] == 0.5
        client.call("remove-surface", "remove_component", entity_id=entity, component="SurfaceMaterial")
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
        client.send(request("invalid", "tools/call", name="spawn_entity", arguments={"shape": "unknown_shape"}))
        assert client.read()["result"]["isError"]
        client.send(request("stale", "tools/call", name="destroy_entity", arguments={"entity_id": "18446744073709551615"}))
        assert client.read()["result"]["isError"]
        client.send(request("missing", "tools/call", name="spawn_model"))
        assert client.read()["result"]["isError"]
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
            client.call("capture-chair", "prefab", action="capture", path="chair.katprefab", root_entity=chair)
            assert (project / "chair.katprefab").is_file()
            copied = client.call("copy-chair", "prefab", action="instantiate", path="chair.katprefab", position=[6,0,1])["entity_ids"]
            assert len(copied) == 1 and copied[0] != chair
            client.call("remove-copy", "prefab", action="remove", root_entity=copied[0])
            assert client.call("original-chair", "query_entities")["entity_ids"] == [chair]
            material_acceptance(client, project, resource, chair)
            material_before_reload = client.call("material-before-reload", "material", action="inspect", entity_id=chair)["data"]
            client.call("save-file", "save_scene", path="authored.katla")
            assert (project / "authored.katla").is_file()
            client.call("load-file", "load_scene", path="authored.katla")
            loaded = client.call("loaded-file", "query_entities")["entity_ids"]
            assert len(loaded) == 1 and loaded[0] != chair
            client.call("save-origin", "save_scene", path=None)
            chair = loaded[0]
            material_after_reload = client.call("material-after-reload", "material", action="inspect", entity_id=chair)["data"]
            material_values_equal(material_after_reload["values"], material_before_reload["values"])
            assert material_after_reload["sampling"] == material_before_reload["sampling"]
            assert material_after_reload["provenance"]["authored_textures"]["albedo"]["source"] == material_before_reload["provenance"]["authored_textures"]["albedo"]["source"]
            client.call("remove-chair", "destroy_entity", entity_id=chair)
            assert client.call("asset-empty", "query_entities")["entity_ids"] == []
            generation_acceptance(client, project)
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
    print("Odin MCP: discovery, 28 canonical tool schemas, actual scene/material edits, typed IDs, pipelining, bounded input, recovery and EOF/output failure passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=output_path() / "bin" / ("katla-mcp-stdio.exe" if os.name == "nt" else "katla-mcp-stdio"))
    args = parser.parse_args()
    acceptance(args.binary.resolve())


if __name__ == "__main__":
    main()
