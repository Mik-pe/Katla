#!/usr/bin/env python3
"""Deterministic local native-panel scene/prefab acceptance; this server is not a paid model."""
from __future__ import annotations

import argparse
import json
import pathlib
import re

from validate_odin_llm import LocalServer, Provider


class EditorProvider(Provider):
    def respond(self, scenario, api, step, request):
        assert scenario == "editor_scene" and api == "responses"
        history = request["input"]
        tools = {item["name"] for item in request["tools"]}
        assert tools == {"material", "query_entities", "get_component_attributes", "search_assets", "list_resources", "read_resource", "save_scene", "load_scene", "prefab"}
        results = [item for item in history if item.get("type") == "function_call_output"]
        payloads = []
        for index, item in enumerate(results):
            assert item["call_id"] == f"native-scene-{index}"
            payload = json.loads(item["output"])
            assert payload["error"] == ({3: "Invalid_Operation", 5: "Entity_Not_Found"}.get(index, "None")), (index, payload)
            payloads.append(payload)
        initial = re.search(r"Selected entity_id=([0-9]+)", history[0]["content"]).group(1)
        if payloads:
            assert len(payloads[0]["entities"]) == 2 and initial in payloads[0]["entities"]
        if len(payloads) > 1:
            assert payloads[1]["data"]["published"] and payloads[1]["data"]["entity_count"] == 2
        fresh = payloads[6]["entities"][0] if len(payloads) > 6 else None
        if len(payloads) > 6:
            assert len(payloads[6]["entities"]) == 2 and not set(payloads[6]["entities"]) & set(payloads[0]["entities"])
        if len(payloads) > 7:
            key = payloads[7]["data"]["value"]
            assert key in (1, 2), payloads[7]
            if key == 2:
                fresh = payloads[6]["entities"][1]
        if len(payloads) > 8:
            assert payloads[8]["data"]["published"] and payloads[8]["data"]["entity_count"] == 1
        inserted = payloads[9]["data"]["root_entity"] if len(payloads) > 9 else None
        if len(payloads) > 9:
            assert inserted in payloads[9]["entities"] and inserted not in payloads[6]["entities"]
        if len(payloads) > 10:
            assert payloads[10]["data"]["removed"] == 1 and payloads[10]["data"]["root_entity"] == inserted
        if len(payloads) > 11:
            assert payloads[11]["entities"] == payloads[6]["entities"]
        sequence = [
            ("query_entities", {}),
            ("save_scene", {"path": "native.katla"}),
            ("material", {"action": "set", "entity_ids": [initial], "base_color": [0.15, 0.85, 0.3, 1], "metallic": 0.8, "roughness": 0.2}),
            ("load_scene", {"path": "missing.katla"}),
            ("load_scene", {"path": "native.katla"}),
            ("material", {"action": "set", "entity_ids": [initial], "metallic": 1}),
            ("query_entities", {}),
            ("get_component_attributes", {"entity_id": fresh, "component": "SceneKey"}),
            ("prefab", {"action": "capture", "path": "resources/native.katprefab", "root_entity": fresh}),
            ("prefab", {"action": "instantiate", "path": "resources/native.katprefab", "name": "Captured sphere", "position": [0, 0.8, 0], "scale": [0.65, 0.65, 0.65]}),
            ("prefab", {"action": "remove", "root_entity": inserted}),
            ("query_entities", {}),
        ]
        turns = [item for item in history if item.get("role") == "user"]
        if len(turns) > 1:
            contexts = [item["content"] for item in history if item.get("role") == "system"]
            assert len(contexts) >= 2 and contexts[0] != contexts[-1], "Next Send must retain history and add fresh selected context"
            selected = re.search(r"Selected entity_id=([0-9]+)", contexts[-1]).group(1)
            assert selected == fresh and selected != initial
            sequence += [
                ("material", {"action": "set", "entity_ids": [selected], "base_color": [0.9, 0.65, 0.12, 1], "metallic": 0.8, "roughness": 0.25}),
                ("get_component_attributes", {"entity_id": selected, "component": "SurfaceMaterial"}),
            ]
        self.start_sse()
        if len(results) < len(sequence):
            name, arguments = sequence[len(results)]
            self.emit_responses(call=(name, arguments, f"native-scene-{len(results)}"))
        else:
            if len(turns) > 1:
                assert abs(payloads[13]["data"]["metallic"] - 0.8) < 0.001
                self.emit_responses(text="Current selection edited. Earlier conversation preserved.")
            else:
                self.emit_responses(text="Scene restored. Stale ID rejected. Prefab captured, instantiated and removed; Undo is available.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=pathlib.Path, required=True)
    parser.add_argument("--receipt", type=pathlib.Path, required=True)
    args = parser.parse_args()
    EditorProvider.calls = {}
    EditorProvider.failures = []
    server = LocalServer(("127.0.0.1", 0), EditorProvider)
    server.daemon_threads = True
    args.config.write_text(f'provider="open_ai_compatible"\napi="responses"\napi_key="local-transport-test"\nbase_url="http://127.0.0.1:{server.server_port}/editor_scene/v1"\nmodel="explicit-test-model"\nrate_limit_min_interval_ms=0\ntimeout_ms=10000\n')
    args.config.chmod(0o600)
    print(f"Native panel acceptance ready on 127.0.0.1:{server.server_port}; config={args.config}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
        with EditorProvider.lock:
            args.receipt.write_text(json.dumps({"scope": "Local HTTP/SSE native UI + actual scene/prefab/GPU; no paid model", "requests": EditorProvider.calls, "failures": EditorProvider.failures}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
