#!/usr/bin/env python3
"""Exercise native Odin HTTP/SSE and owner-thread scene tools against a local HTTP server.

This validates transport and orchestration. It does not claim paid-provider/model acceptance.
"""
from __future__ import annotations

import argparse
import http.server
import json
import os
import pathlib
import subprocess
import socketserver
import ssl
import tempfile
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]

from build_katla_odin import cpu_test_environment
from odin_validation_manifest import validation_manifest


class LocalServer(http.server.ThreadingHTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name = "127.0.0.1"
        self.server_port = self.server_address[1]


class Provider(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    calls: dict[str, list[dict]] = {}
    failures: list[str] = []
    lock = threading.Lock()

    def log_message(self, *_args):
        pass

    def do_POST(self):
        scenario, _, endpoint = self.path.strip("/").split("/", 2)
        try:
            size = int(self.headers["Content-Length"])
            assert 0 < size < 8 * 1024 * 1024
            request = json.loads(self.rfile.read(size))
            assert self.headers["Authorization"] == "Bearer local-transport-test"
            assert request["model"] == "explicit-test-model"
            assert request["stream"] is True and request["tools"]
            assert endpoint in ("responses", "chat/completions")
            api = "responses" if endpoint == "responses" else "chat"
            with self.lock:
                trace = self.calls.setdefault(scenario, [])
                trace.append(request)
                step = len(trace)
            if scenario == "parallel":
                history = request["input"] if api == "responses" else request["messages"]
                step = 1 + sum(item.get("type") == "function_call_output" or item.get("role") == "tool" for item in history)
            self.respond(scenario, api, step, request)
        except (BrokenPipeError, ConnectionResetError):
            pass
        except Exception as exc:
            with self.lock:
                self.failures.append(f"{scenario}: {type(exc).__name__}: {exc}")
            self.close_connection = True

    def start_sse(self):
        self.send_response(200)
        self.send_header("cOnTeNt-TyPe", "text/event-stream; charset=utf-8")
        self.send_header("Connection", "close")
        self.end_headers()
        self.close_connection = True

    def emit(self, value):
        data = ("data: " + json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\r\n\r\n").encode()
        for offset in range(0, len(data), 7):
            self.wfile.write(data[offset:offset + 7])
            self.wfile.flush()

    def chat(self, text=None, call=None, truncated=False):
        if call:
            name, arguments, call_id = call
            args = json.dumps(arguments, separators=(",", ":"))
            midpoint = len(args) // 2
            self.emit({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "id": call_id, "type": "function", "function": {"name": name, "arguments": args[:midpoint]}}]}, "finish_reason": None}]})
            self.emit({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": args[midpoint:]}}]}, "finish_reason": "tool_calls"}]})
        else:
            self.emit({"choices": [{"index": 0, "delta": {"content": text or ""}, "finish_reason": "stop"}]})
        if not truncated:
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()

    def emit_responses(self, text=None, call=None, truncated=False):
        self.emit({"type": "response.created", "response": {"id": "provider-response", "status": "in_progress"}})
        output = []
        if text:
            self.emit({"type": "response.output_text.delta", "delta": text})
            output.append({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": text}]})
        if call:
            name, arguments, call_id = call
            args = json.dumps(arguments, separators=(",", ":"))
            self.emit({"type": "response.function_call_arguments.delta", "delta": args[:1], "item_id": "item"})
            self.emit({"type": "response.function_call_arguments.delta", "delta": args[1:], "item_id": "item"})
            output.append({"type": "function_call", "call_id": call_id, "name": name, "arguments": args})
        if not truncated:
            self.emit({"type": "response.completed", "response": {"status": "completed", "output": output}})

    def respond(self, scenario, api, step, request):
        if scenario == "http429":
            self.send_response(429)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if scenario == "redirect":
            self.send_response(307)
            self.send_header("Location", "/forbidden/v1/responses")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        self.start_sse()
        if scenario in ("timeout", "cancel") or scenario == "cancel_after_tool" and step > 1:
            time.sleep(1.5)
            return
        if scenario == "oversize":
            self.wfile.write(b"data: " + b"x" * (256 * 1024 + 1))
            self.wfile.flush()
            return
        send = self.emit_responses if api == "responses" else self.chat
        if scenario == "backpressure":
            self.emit({"type": "response.output_text.delta", "delta": "first"})
            self.emit({"type": "response.output_text.delta", "delta": "second"})
            self.emit({"type": "response.completed", "response": {"status": "completed", "output": [{"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "firstsecond"}]}]}})
            return
        if scenario == "truncated":
            send(text="partial", truncated=True)
            return
        if scenario == "malformed":
            self.emit({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "id": "bad", "function": {"name": "spawn_entity", "arguments": "[1]"}}]}, "finish_reason": "tool_calls"}]})
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
            return
        if scenario in ("unknown", "known_blocked"):
            tool_name = "unknown_tool" if scenario == "unknown" else "spawn_entity"
            if step == 1:
                send(call=(tool_name, {}, "unknown-id"))
            else:
                history = request["input"] if api == "responses" else request["messages"]
                result = history[-1]
                assert (result.get("call_id") or result.get("tool_call_id")) == "unknown-id"
                content = json.loads(result.get("output") or result.get("content"))
                assert content["error"] == "Tool_Not_Allowed"
                send(text="Tool rejected.")
            return
        if step == 1:
            send(call=("spawn_entity", {"name": "fox"}, "spawn-id-exact"))
            return
        history = request["input"] if api == "responses" else request["messages"]
        result = history[-1]
        content = json.loads(result.get("output") or result.get("content"))
        assert content["error"] == "None"
        assert len(content["entities"]) in ((1, 2) if scenario == "parallel" and step == 3 else (1,)) and isinstance(content["entities"][0], str)
        if step == 2:
            assert (result.get("call_id") or result.get("tool_call_id")) == "spawn-id-exact"
            send(call=("query_entities", {}, "query-id-exact"))
        else:
            assert step == 3
            assert (result.get("call_id") or result.get("tool_call_id")) == "query-id-exact"
            send(text="Fox created. 🦊")


def validate(binary: pathlib.Path, environment=None):
    Provider.calls = {}
    Provider.failures = []
    server = LocalServer(("127.0.0.1", 0), Provider)
    server.daemon_threads = True
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    cases = [
        ("responses", "responses", "None", "", 1, 2),
        ("chat", "chat_completions", "None", "", 1, 2),
        ("unknown", "responses", "None", "", 0, 0),
        ("truncated", "responses", "Truncated", "", 0, 0),
        ("malformed", "chat_completions", "Protocol", "", 0, 0),
        ("http429", "responses", "Rate_Limited", "", 0, 0),
        ("redirect", "responses", "HTTP", "", 0, 0),
        ("oversize", "responses", "Limit", "", 0, 0),
        ("timeout", "responses", "Timeout", "", 0, 0),
        ("cancel", "responses", "Cancelled", "cancel", 0, 0),
        ("paused", "responses", "Cancelled", "paused", 0, 0),
        ("known_blocked", "responses", "None", "readonly", 0, 0),
        ("rate", "responses", "Rate_Limited", "", 1, 1),
        ("cancel_after_tool", "responses", "Cancelled", "cancel_after_tool", 1, 1),
        ("backpressure", "responses", "Limit", "backpressure", 0, 0),
        ("parallel", "responses", "None", "parallel", 2, 4),
    ]
    try:
        with tempfile.TemporaryDirectory(prefix="katla-llm-") as directory:
            for scenario, api, error, mode, entities, actions in cases:
                path = pathlib.Path(directory) / "llm.toml"
                timeout = 150 if scenario == "timeout" else 3000
                maximum = 1 if scenario == "rate" else 20
                path.write_text(f'provider = "open_ai_compatible"\napi = "{api}"\napi_key = "local-transport-test"\nbase_url = "http://127.0.0.1:{server.server_port}/{scenario}/v1"\nmodel = "explicit-test-model"\nrate_limit_min_interval_ms = 0\nrate_limit_max_calls_per_minute = {maximum}\ntimeout_ms = {timeout}\n')
                start = time.monotonic()
                command = [str(binary), str(path)] + ([mode] if mode else [])
                result = subprocess.run(command, cwd=ROOT, env=environment, capture_output=True, text=True, timeout=8)
                assert result.returncode == 0, (scenario, result.stderr, result.stdout)
                assert "local-transport-test" not in result.stdout + result.stderr
                output = json.loads(result.stdout)
                assert output["error"] == error, (scenario, output, result.stderr, Provider.failures)
                assert (output["entities"], output["actions"]) == (entities, actions), (scenario, output)
                if scenario in ("responses", "chat"):
                    assert output["text"] == output["streamed"] == "Fox created. 🦊" and len(Provider.calls[scenario]) == 3
                if scenario == "parallel":
                    assert output["jobs"] == 2 and output["texts"] == ["Fox created. 🦊", "Fox created. 🦊"] and len(Provider.calls[scenario]) == 6
                if mode:
                    assert time.monotonic() - start < 4, (scenario, "shutdown deadline")
            tls_directory = pathlib.Path(directory)
            certificate = tls_directory / "local-test-cert.pem"
            private_key = tls_directory / "local-test-key.pem"
            subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-subj", "/CN=localhost", "-keyout", str(private_key), "-out", str(certificate)], check=True, capture_output=True, timeout=10)
            tls_server = LocalServer(("127.0.0.1", 0), Provider)
            tls_server.daemon_threads = True
            tls_context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            tls_context.load_cert_chain(certificate, private_key)
            tls_server.socket = tls_context.wrap_socket(tls_server.socket, server_side=True)
            tls_worker = threading.Thread(target=tls_server.serve_forever, daemon=True)
            tls_worker.start()
            try:
                path.write_text(f'provider = "open_ai_compatible"\napi = "responses"\napi_key = "local-transport-test"\nbase_url = "https://127.0.0.1:{tls_server.server_port}/untrusted_tls/v1"\nmodel = "explicit-test-model"\nrate_limit_min_interval_ms = 0\ntimeout_ms = 3000\n')
                rejected = subprocess.run([str(binary), str(path)], cwd=ROOT, env=environment, capture_output=True, text=True, timeout=8)
                assert rejected.returncode == 0, rejected.stderr
                rejected_output = json.loads(rejected.stdout)
                assert rejected_output["error"] == "Network" and rejected_output["entities"] == 0 and rejected_output["actions"] == 0
                assert "local-transport-test" not in rejected.stdout + rejected.stderr
                assert "untrusted_tls" not in Provider.calls
            finally:
                tls_server.shutdown()
                tls_server.server_close()
                tls_worker.join()
            assert "forbidden" not in Provider.calls
            assert not Provider.failures, Provider.failures
    finally:
        server.shutdown()
        server.server_close()
        worker.join()
    print(f"PASS: {len(cases) + 1} native HTTP/TLS/SSE journeys; exact tool IDs, scene owner/undo, no secret logs, failures and cancellation")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, default=ROOT / "target/odin-llm-authoring")
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
        command = [odin, "build", "odin/examples/llm_authoring", f"-out:{binary}", "-vet", "-strict-style"]
        if manifest:
            command += manifest["foreign_defines"]
        if args.sanitize:
            command += ["-sanitize:address", "-debug"]
        subprocess.run(command, cwd=ROOT, env=environment, check=True)
    validate(binary, environment)


if __name__ == "__main__":
    main()
