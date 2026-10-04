#!/usr/bin/env python3
"""Exercise real Assistant HTTP/SSE, atomic scene files and staged world replacement; no paid model."""
from __future__ import annotations

import argparse
import json
import os
import pathlib
import subprocess
import tempfile
import threading

from validate_odin_llm import LocalServer, Provider, ROOT

from build_katla_odin import cpu_test_environment
from odin_validation_manifest import validation_manifest


class SceneProvider(Provider):
    def respond(self, scenario, api, step, request):
        assert scenario == "scene"
        history = request["input"] if api == "responses" else request["messages"]
        results = [item for item in history if item.get("type") == "function_call_output" or item.get("role") == "tool"]
        payloads = []
        for index, result in enumerate(results):
            assert (result.get("call_id") or result.get("tool_call_id")) == f"scene-{index}", results
            payload = json.loads(result.get("output") or result.get("content"))
            assert payload["error"] == ("Invalid_Operation" if index == 3 else "None"), payload
            payloads.append(payload)
        original = payloads[0]["entities"][0] if payloads else None
        if len(payloads) > 1:
            assert payloads[1]["data"]["published"] and payloads[1]["data"]["path"] == "scene.katla"
        if len(payloads) > 4:
            assert payloads[4]["entities"] == [original], "Failed load changed actual identity"
        if len(payloads) > 5:
            loaded = payloads[5]
            assert loaded["data"]["runtime_ids_replaced"] and len(loaded["entities"]) == 1 and loaded["entities"][0] != original
        if len(payloads) > 6:
            assert payloads[6]["entities"] == payloads[5]["entities"], "Post-load query failed to observe actual fresh identity"
        sequence = [
            ("spawn_entity", {"name": "fox"}),
            ("save_scene", {"path": "scene.katla"}),
            ("material", {"action": "set", "entity_ids": [original], "metallic": 0.8}),
            ("load_scene", {"path": "missing.katla"}),
            ("query_entities", {}),
            ("load_scene", {"path": "scene.katla"}),
            ("query_entities", {}),
        ]
        self.start_sse()
        send = self.emit_responses if api == "responses" else self.chat
        if len(results) < len(sequence):
            name, arguments = sequence[len(results)]
            send(call=(name, arguments, f"scene-{len(results)}"))
        else:
            send(text="Scene restored with fresh IDs.")


def validate(binary: pathlib.Path, environment=None):
    SceneProvider.calls = {}
    SceneProvider.failures = []
    server = LocalServer(("127.0.0.1", 0), SceneProvider)
    server.daemon_threads = True
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        with tempfile.TemporaryDirectory(prefix="katla-assistant-scene-") as directory:
            root = pathlib.Path(directory)
            for api in ("responses", "chat_completions"):
                project = root / api
                resources = project / "resources"
                resources.mkdir(parents=True)
                config = root / "llm.toml"
                config.write_text(f'provider="open_ai_compatible"\napi="{api}"\napi_key="local-transport-test"\nbase_url="http://127.0.0.1:{server.server_port}/scene/v1"\nmodel="explicit-test-model"\nrate_limit_min_interval_ms=0\ntimeout_ms=3000\n')
                config.chmod(0o600)
                result = subprocess.run([str(binary), str(config), str(project), str(resources)], cwd=ROOT, env=environment, capture_output=True, text=True, timeout=12)
                assert result.returncode == 0, (api, result.stderr, result.stdout, SceneProvider.failures)
                assert "local-transport-test" not in result.stdout + result.stderr
                actual = json.loads(result.stdout)
                assert actual == {"text": "Scene restored with fresh IDs.", "ticks": 7, "entities": 1, "edits": 0, "loaded": True, "failed_load_preserved": True, "reset": True}, actual
                scene = project / "scene.katla"
                assert scene.is_file() and 'name:"fox"' in scene.read_text() and "metallic:0" in scene.read_text()
                assert list(project.glob("*.katla")) == [scene], "Unexpected scene publication"
            assert not SceneProvider.failures, SceneProvider.failures
            assert len(SceneProvider.calls["scene"]) == 16
    finally:
        server.shutdown()
        server.server_close()
        worker.join()
    print("PASS: 2 actual Assistant HTTP/SSE scene save/edit/failed-load/restore/query/reset journeys; fresh IDs, atomic files, shared history and allocation cleanup")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, default=ROOT / "target/odin-assistant-scene")
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--build-manifest", type=pathlib.Path, help="Reuse verified canonical dependency paths, compiler and sanitizer mode")
    args = parser.parse_args()
    manifest = validation_manifest(parser, args.build_manifest, args.sanitize, [])
    binary = args.binary.resolve()
    binary.parent.mkdir(parents=True, exist_ok=True)
    environment = cpu_test_environment(os.environ, binary.parent, args.sanitize)
    odin = manifest["odin"] if manifest else "odin"
    if not args.no_build:
        command = [odin, "build", "odin/examples/assistant_scene", f"-out:{binary}", "-vet", "-strict-style"]
        if manifest:
            command += manifest["foreign_defines"]
        if args.sanitize:
            command += ["-sanitize:address", "-debug"]
        subprocess.run(command, cwd=ROOT, env=environment, check=True)
    validate(binary, environment)


if __name__ == "__main__":
    main()
