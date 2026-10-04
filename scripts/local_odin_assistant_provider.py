#!/usr/bin/env python3
"""Local native-editor acceptance provider. This server is deterministic transport evidence, not an LLM."""
from __future__ import annotations

import argparse
import json
import pathlib
import re
import time

from validate_odin_llm import LocalServer, Provider


class MaterialProvider(Provider):
    def respond(self, scenario, api, step, request):
        if scenario in ("cancel", "http429", "truncated"):
            if scenario == "truncated":
                self.start_sse()
                self.emit_responses(text="Partial provider response", truncated=True)
            elif scenario == "cancel":
                self.start_sse()
                self.emit({"type": "response.output_text.delta", "delta": "Waiting for provider…"})
                time.sleep(10)
            else:
                super().respond(scenario, api, step, request)
            return
        assert scenario == "material"
        history = request["input"] if api == "responses" else request["messages"]
        results = [item for item in history if item.get("type") == "function_call_output" or item.get("role") == "tool"]
        self.start_sse()
        send = self.emit_responses if api == "responses" else self.chat
        if not results:
            send(call=("query_entities", {}, "native-query-exact"))
        elif len(results) == 1:
            reply = results[0]
            assert (reply.get("call_id") or reply.get("tool_call_id")) == "native-query-exact"
            result = json.loads(reply.get("output") or reply.get("content"))
            assert result["error"] == "None" and result["entities"]
            marker = re.search(r"Selected entity_id=([0-9]+)", history[0]["content"])
            assert marker, "native consumer must supply its selected entity context"
            selected = marker.group(1)
            assert selected in result["entities"] and selected.isdecimal()
            send(call=("material", {"action": "set", "entity_ids": [selected], "base_color": [0.15, 0.85, 0.3, 1], "metallic": 0.8, "roughness": 0.2}, "native-material-exact"))
        else:
            assert len(results) == 2
            reply = results[-1]
            assert (reply.get("call_id") or reply.get("tool_call_id")) == "native-material-exact"
            result = json.loads(reply.get("output") or reply.get("content"))
            assert result["error"] == "None" and len(result["entities"]) == 1
            if api == "responses":
                self.emit({"type": "response.output_text.delta", "delta": "Material updated. "})
                time.sleep(0.4)
                self.emit({"type": "response.output_text.delta", "delta": "Shared Undo is available."})
                self.emit({"type": "response.completed", "response": {"status": "completed", "output": [{"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Material updated. Shared Undo is available."}]}]}})
            else:
                send(text="Material updated. Shared Undo is available.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=pathlib.Path, required=True)
    parser.add_argument("--scenario", choices=("material", "cancel", "http429", "truncated"), default="material")
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--receipt", type=pathlib.Path)
    args = parser.parse_args()
    MaterialProvider.calls = {}
    MaterialProvider.failures = []
    server = LocalServer(("127.0.0.1", args.port), MaterialProvider)
    server.daemon_threads = True
    args.config.write_text(f'provider="open_ai_compatible"\napi="responses"\napi_key="local-transport-test"\nbase_url="http://127.0.0.1:{server.server_port}/{args.scenario}/v1"\nmodel="explicit-test-model"\nrate_limit_min_interval_ms=0\ntimeout_ms=10000\n')
    args.config.chmod(0o600)
    print(f"Local acceptance server ready on 127.0.0.1:{server.server_port}; scenario={args.scenario}; config={args.config}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
        if args.receipt:
            with MaterialProvider.lock:
                args.receipt.write_text(json.dumps({"scope": "local HTTP/SSE native editor acceptance; no paid model", "requests": MaterialProvider.calls, "failures": MaterialProvider.failures}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
