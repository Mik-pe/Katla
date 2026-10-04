#!/usr/bin/env python3
"""Real private Unix API fixture, never a real conversation."""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading

ROOT = Path(__file__).resolve().parents[1]


def run(sanitize=False, scenario="journey"):
    with tempfile.TemporaryDirectory(prefix="katla-host-fixture-") as directory:
        endpoint = Path(directory) / "control.sock"
        server = socket.socket(socket.AF_UNIX)
        server.bind(str(endpoint))
        endpoint.chmod(0o600)
        server.listen(1)
        failures = []
        methods = []

        def serve():
            try:
                connection, _ = server.accept()
                connection.settimeout(10)
                with connection, connection.makefile("rwb", buffering=0) as wire:
                    def send(value):
                        wire.write(json.dumps(value, separators=(",", ":")).encode() + b"\n")

                    while True:
                        raw = wire.readline()
                        if not raw:
                            break
                        try:
                            request = json.loads(raw)
                        except ValueError as error:
                            raise AssertionError(repr(raw)) from error
                        method = request["method"]
                        methods.append(method)
                        if scenario == "eof":
                            return
                        if scenario == "malformed":
                            wire.write(b'{"id":18446744073709551617,"result":{}}\n')
                            continue
                        if scenario == "cancel_wait":
                            assert wire.readline() == b""
                            return
                        assert method not in {"thread/start", "thread/fork", "account/login/start"}
                        params = request.get("params", {})
                        if method == "initialized":
                            continue
                        assert isinstance(request["id"], int), "No response to unsolicited host approval allowed"
                        if method == "initialize":
                            assert params == {"clientInfo": {"name": "katla_odin_editor", "title": "Katla editor", "version": "0.1.0"}}
                            result = {"userAgent": "local-api-fixture"}
                        elif method == "thread/loaded/list":
                            result = {"data": ["other-thread"], "nextCursor": None} if scenario == "missing_thread" else ({"data": ["other-thread"], "nextCursor": "page2"} if params["cursor"] is None else {"data": ["fixture-existing-thread"], "nextCursor": None})
                        elif method == "thread/resume":
                            assert params == {"threadId": "fixture-existing-thread"}
                            result = {"thread": {"id": "wrong-thread" if scenario == "wrong_resume" else "fixture-existing-thread", "name": "Existing fixture conversation"}}
                        elif method == "thread/read":
                            assert params == {"threadId": "fixture-existing-thread", "includeTurns": True}
                            active = "turn/start" in methods
                            result = {"thread": {"id": "fixture-existing-thread", "status": {"type": "active" if active else "idle"}, "turns": [{"id": "existing-active-turn", "status": "inProgress"}] if active else []}}
                        elif method in {"turn/start", "turn/steer"}:
                            assert params["threadId"] == "fixture-existing-thread"
                            assert set(params) == ({"threadId", "input"} if method == "turn/start" else {"threadId", "expectedTurnId", "input"})
                            inputs = params["input"]
                            assert len(inputs) == 2 and inputs[0]["type"] == "text" and inputs[1]["url"].startswith("data:image/png;base64,")
                            assert "Geometry candidates are not proof" in inputs[0]["text"]
                            if method == "turn/start":
                                send({"method": "item/agentMessage/delta", "params": {"threadId": "foreign-thread", "turnId": "wrong", "itemId": "wrong", "delta": "Must be ignored"}})
                                send({"method": "item/agentMessage/delta", "params": {"threadId": "fixture-existing-thread", "turnId": "new-turn", "itemId": "agent-item", "delta": "Viewport fixture reply"}})
                                send({"id": "approval-owned-by-host", "method": "item/commandExecution/requestApproval", "params": {}})
                                send({"method": "turn/completed", "params": {"threadId": "fixture-existing-thread", "turn": {"id": "new-turn", "status": "completed"}}})
                                result = {"turn": {"id": "new-turn"}}
                            else:
                                assert params["expectedTurnId"] == "existing-active-turn"
                                result = {"turnId": "existing-active-turn"}
                        elif method == "turn/interrupt":
                            assert params == {"threadId": "fixture-existing-thread", "turnId": "existing-active-turn"}
                            send({"id": request["id"], "result": {}})
                            send({"method": "turn/completed", "params": {"threadId": "fixture-existing-thread", "turn": {"id": "existing-active-turn", "status": "interrupted"}}})
                            return
                        else:
                            raise AssertionError(method)
                        send({"id": request["id"], "result": result})
            except BaseException as error:
                failures.append(repr(error))

        worker = threading.Thread(target=serve, daemon=True)
        worker.start()
        binary = Path(directory) / "host-connection"
        command = ["odin", "build", "odin/examples/host_connection", "-vet", "-strict-style", f"-out:{binary}"]
        if sanitize:
            command.append("-sanitize:address")
        subprocess.run(command, cwd=ROOT, check=True)
        mode = "journey" if scenario == "journey" else "cancel_wait" if scenario == "cancel_wait" else "disconnect"
        completed = subprocess.run([str(binary), str(endpoint), mode], cwd=ROOT, capture_output=True, text=True, timeout=25)
        worker.join(2)
        server.close()
        assert not failures, failures
        assert completed.returncode == 0, str(methods) + completed.stdout + completed.stderr
        if scenario == "journey":
            assert methods == ["initialize", "initialized", "thread/loaded/list", "thread/loaded/list", "thread/resume", "thread/read", "turn/start", "thread/read", "turn/steer", "turn/interrupt"], methods
        print(completed.stdout.strip())
        print("PASS direct existing-host Unix API fixture; no paid provider or human conversation used")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--sanitize", action="store_true")
    for case in ["journey", "eof", "malformed", "missing_thread", "wrong_resume", "cancel_wait"]:
        run(parser.parse_args().sanitize, case)
