#!/usr/bin/env python3
"""Explicit local existing-thread API fixture for native editor acceptance, never a paid host."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import socket
import signal
import stat
import struct
import zlib

THREAD = "native-fixture-existing-thread"


def png_inspect(encoded):
    png = base64.b64decode(encoded, validate=True)
    assert png[:8] == b"\x89PNG\r\n\x1a\n"
    width, height = struct.unpack(">II", png[16:24])
    assert 64 <= width <= 8192 and 64 <= height <= 8192 and width * height <= 16 << 20
    offset = 8
    ended = False
    while offset < len(png):
        length = struct.unpack(">I", png[offset:offset + 4])[0]
        body = png[offset + 4:offset + 8 + length]
        assert len(body) == length + 4
        crc = struct.unpack(">I", png[offset + 8 + length:offset + 12 + length])[0]
        assert zlib.crc32(body) == crc
        offset += length + 12
        if body[:4] == b"IEND":
            assert length == 0 and offset == len(png)
            ended = True
    assert ended
    return png, width, height


def run(endpoint, receipt, hold_second):
    assert endpoint.is_absolute() and not endpoint.exists()
    parent = endpoint.parent.stat()
    assert parent.st_uid == os.geteuid() and stat.S_IMODE(parent.st_mode) == 0o700
    assert receipt.is_absolute() and receipt.parent == endpoint.parent and not receipt.exists()
    record = {"fixture": "local-existing-thread-api", "thread_id": THREAD, "methods": [], "questions": [], "connections": 0, "failures": []}
    active = None
    listener = socket.socket(socket.AF_UNIX)
    listener.bind(str(endpoint)); endpoint.chmod(0o600); listener.listen(1)
    inode = endpoint.stat().st_ino

    def persist():
        temporary = receipt.with_suffix(".tmp")
        temporary.write_text(json.dumps(record, indent=2) + "\n")
        temporary.chmod(0o600); temporary.replace(receipt)

    persist()
    print(json.dumps({"fixture_socket": str(endpoint), "thread_id": THREAD, "receipt": str(receipt)}), flush=True)
    try:
        while True:
            connection, _ = listener.accept()
            record["connections"] += 1
            try:
                with connection, connection.makefile("rwb", buffering=0) as wire:
                    def send(value):
                        wire.write(json.dumps(value, separators=(",", ":")).encode() + b"\n")

                    def completed(turn, status):
                        send({"method": "turn/completed", "params": {"threadId": THREAD, "turn": {"id": turn, "status": status}}})

                    while True:
                        raw = wire.readline((32 << 20) + 1)
                        if not raw:
                            break
                        assert len(raw) <= 32 << 20 and raw.endswith(b"\n")
                        request = json.loads(raw)
                        method = request["method"]; record["methods"].append(method)
                        assert method not in {"thread/start", "thread/fork", "account/login/start"}
                        params = request.get("params", {})
                        if method == "initialized":
                            persist(); continue
                        assert isinstance(request["id"], int)
                        if method == "initialize":
                            assert params == {"clientInfo": {"name": "katla_odin_editor", "title": "Katla editor", "version": "0.1.0"}}
                            result = {"userAgent": "explicit-native-local-fixture"}
                        elif method == "thread/loaded/list":
                            result = {"data": [THREAD], "nextCursor": None}
                        elif method == "thread/resume":
                            assert params == {"threadId": THREAD}
                            result = {"thread": {"id": THREAD, "name": "Local native acceptance fixture"}}
                        elif method == "thread/read":
                            assert params == {"threadId": THREAD, "includeTurns": True}
                            result = {"thread": {"id": THREAD, "status": {"type": "active" if active else "idle"}, "turns": [{"id": active, "status": "inProgress"}] if active else []}}
                        elif method in {"turn/start", "turn/steer"}:
                            assert params["threadId"] == THREAD
                            assert set(params) == ({"threadId", "input"} if method == "turn/start" else {"threadId", "expectedTurnId", "input"})
                            if active:
                                assert method == "turn/steer" and params["expectedTurnId"] == active
                            else:
                                assert method == "turn/start"
                                active = f"native-turn-{len(record['questions']) + 1}"
                            inputs = params["input"]
                            assert len(inputs) == 2 and inputs[0]["type"] == "text" and inputs[1]["type"] == "image"
                            text = inputs[0]["text"]
                            assert "Geometry candidates are not proof" in text
                            metadata = json.loads(text.rsplit("\n", 1)[1]); assert isinstance(metadata, dict) and metadata
                            image = inputs[1]["url"]; assert image.startswith("data:image/png;base64,")
                            png, width, height = png_inspect(image.split(",", 1)[1])
                            number = len(record["questions"]) + 1
                            image_path = receipt.parent / f"viewport-{number}.png"
                            image_path.write_bytes(png); image_path.chmod(0o600)
                            record["questions"].append({"number": number, "method": method, "turn": active, "text": text.split("\n\nKatla committed", 1)[0], "metadata": metadata, "width": width, "height": height, "png_sha256": hashlib.sha256(png).hexdigest(), "png_path": str(image_path)})
                            send({"id": request["id"], "result": {"turn": {"id": active}}})
                            send({"method": "item/agentMessage/delta", "params": {"threadId": "foreign-thread", "turnId": "foreign", "itemId": "foreign", "delta": "FOREIGN MUST NOT APPEAR"}})
                            send({"method": "item/agentMessage/delta", "params": {"threadId": THREAD, "turnId": active, "itemId": f"native-item-{number}", "delta": f"Local fixture received committed viewport {number}. "}})
                            send({"method": "item/agentMessage/delta", "params": {"threadId": THREAD, "turnId": active, "itemId": f"native-item-{number}", "delta": "The existing conversation identity is preserved."}})
                            if not (hold_second and active == "native-turn-2"):
                                completed(active, "completed"); active = None
                            persist(); continue
                        elif method == "turn/interrupt":
                            assert active and params == {"threadId": THREAD, "turnId": active}
                            send({"id": request["id"], "result": {}}); completed(active, "interrupted"); active = None
                            persist(); continue
                        else:
                            raise AssertionError(method)
                        send({"id": request["id"], "result": result}); persist()
            except (SystemExit, KeyboardInterrupt):
                raise
            except BaseException as error:
                record["failures"].append(repr(error)); persist(); raise
            record["active_turn_after_disconnect"] = active
            persist()
    finally:
        listener.close()
        if endpoint.exists() and endpoint.stat().st_ino == inode:
            endpoint.unlink()


if __name__ == "__main__":
    def stop_fixture(_signal, _frame):
        raise SystemExit(0)

    signal.signal(signal.SIGTERM, stop_fixture)
    parser = argparse.ArgumentParser()
    parser.add_argument("--socket", required=True, type=Path)
    parser.add_argument("--receipt", required=True, type=Path)
    parser.add_argument("--hold-second", action="store_true")
    options = parser.parse_args()
    run(options.socket, options.receipt, options.hold_second)
