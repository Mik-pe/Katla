#!/usr/bin/env python3
"""Verify the actual application service against local HTTP/SSE; no paid model acceptance is claimed."""
from __future__ import annotations

import argparse
import json
import os
import pathlib
import subprocess
import ssl
import tempfile
import threading
import time

from validate_odin_llm import LocalServer, Provider, ROOT

from build_katla_odin import cpu_test_environment
from odin_validation_manifest import validation_manifest


class AssistantProvider(Provider):
    def respond(self, scenario, api, step, request):
        if scenario == "partial_cancel":
            self.start_sse()
            self.emit({"type": "response.output_text.delta", "delta": "Working on the scene…"})
            time.sleep(1.5)
            return
        super().respond(scenario, api, step, request)


def validate(binary: pathlib.Path, environment=None):
    AssistantProvider.calls = {}
    AssistantProvider.failures = []
    server = LocalServer(("127.0.0.1", 0), AssistantProvider)
    server.daemon_threads = True
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    cases = [
        ("responses", "responses", "None", "", 1, 2),
        ("chat", "chat_completions", "None", "", 1, 2),
        ("unknown", "responses", "None", "", 0, 0),
        ("truncated", "responses", "Truncated", "", 0, 0),
        ("http429", "responses", "Rate_Limited", "", 0, 0),
        ("redirect", "responses", "HTTP", "", 0, 0),
        ("oversize", "responses", "Limit", "", 0, 0),
        ("timeout", "responses", "Timeout", "", 0, 0),
        ("cancel", "responses", "Cancelled", "cancel", 0, 0),
        ("paused", "responses", "Cancelled", "paused", 0, 0),
        ("cancel_after_tool", "responses", "Cancelled", "cancel_after_tool", 1, 1),
        ("partial_cancel", "responses", "Cancelled", "cancel", 0, 0),
    ]
    try:
        with tempfile.TemporaryDirectory(prefix="katla-assistant-") as directory:
            for scenario, api, error, mode, entities, actions in cases:
                path = pathlib.Path(directory) / "llm.toml"
                timeout = 150 if scenario == "timeout" else 3000
                path.write_text(f'provider="open_ai_compatible"\napi="{api}"\napi_key="local-transport-test"\nbase_url="http://127.0.0.1:{server.server_port}/{scenario}/v1"\nmodel="explicit-test-model"\nrate_limit_min_interval_ms=0\ntimeout_ms={timeout}\n')
                result = subprocess.run([str(binary), str(path)] + ([mode] if mode else []), cwd=ROOT, env=environment, capture_output=True, text=True, timeout=8)
                assert result.returncode == 0, (scenario, result.stderr, result.stdout)
                assert "local-transport-test" not in result.stderr + result.stdout
                output = json.loads(result.stdout)
                assert output["error"] == error and output["state"] == ("Completed" if error == "None" else "Failed"), (scenario, output)
                assert (output["entities"], output["actions"]) == (entities, actions), (scenario, output)
                if scenario in ("responses", "chat"):
                    assert output["text"] == "Fox created. 🦊" and len(AssistantProvider.calls[scenario]) == 3 and output["messages"] >= 7
                if scenario == "partial_cancel":
                    assert output["progress"] and output["text"] == "Working on the scene…", output
            certificate = pathlib.Path(directory) / "local-cert.pem"
            private_key = pathlib.Path(directory) / "local-key.pem"
            subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-subj", "/CN=localhost", "-keyout", str(private_key), "-out", str(certificate)], check=True, capture_output=True, timeout=10)
            tls_server = LocalServer(("127.0.0.1", 0), AssistantProvider)
            tls_server.daemon_threads = True
            tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            tls.load_cert_chain(certificate, private_key)
            tls_server.socket = tls.wrap_socket(tls_server.socket, server_side=True)
            tls_worker = threading.Thread(target=tls_server.serve_forever, daemon=True)
            tls_worker.start()
            try:
                path.write_text(f'provider="open_ai_compatible"\napi="responses"\napi_key="local-transport-test"\nbase_url="https://127.0.0.1:{tls_server.server_port}/untrusted_tls/v1"\nmodel="explicit-test-model"\nrate_limit_min_interval_ms=0\ntimeout_ms=3000\n')
                result = subprocess.run([str(binary), str(path)], cwd=ROOT, env=environment, capture_output=True, text=True, timeout=8)
                assert result.returncode == 0, result.stderr
                output = json.loads(result.stdout)
                assert output["error"] == "Network" and output["state"] == "Failed" and output["entities"] == output["actions"] == 0, output
                assert "untrusted_tls" not in AssistantProvider.calls and "local-transport-test" not in result.stderr + result.stdout
            finally:
                tls_server.shutdown()
                tls_server.server_close()
                tls_worker.join()
            assert "forbidden" not in AssistantProvider.calls
            assert not AssistantProvider.failures, AssistantProvider.failures
    finally:
        server.shutdown()
        server.server_close()
        worker.join()
    print(f"PASS: {len(cases) + 1} application assistant HTTP/TLS/SSE journeys; service stream/status, main-thread tool/undo, failures/cancel/reset and allocation cleanup")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, default=ROOT / "target/odin-assistant-authoring")
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
        command = [odin, "build", "odin/examples/assistant_authoring", f"-out:{binary}", "-vet", "-strict-style"]
        if manifest:
            command += manifest["foreign_defines"]
        if args.sanitize:
            command += ["-sanitize:address", "-debug"]
        subprocess.run(command, cwd=ROOT, env=environment, check=True)
    validate(binary, environment)


if __name__ == "__main__":
    main()
