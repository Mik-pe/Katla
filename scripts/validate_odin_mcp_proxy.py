#!/usr/bin/env python3
"""Exercise the real bounded stdio/private-socket executable without any host/model."""
import argparse
import errno
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[1]


def run(sanitize=False, slow=False):
    with tempfile.TemporaryDirectory(prefix="katla-mcp-proxy-") as directory:
        binary = Path(directory) / "katla-mcp"
        command = ["odin", "build", "odin/mcp_proxy", "-vet", "-strict-style", f"-out:{binary}"]
        if sanitize:
            command.append("-sanitize:address")
        subprocess.run(command, cwd=ROOT, check=True)
        endpoint = Path(directory) / "editor.sock"

        def fixture(handler, closed_output=False):
            server = socket.socket(socket.AF_UNIX)
            server.bind(str(endpoint))
            endpoint.chmod(0o600)
            server.listen(1)
            failures = []

            def serve():
                try:
                    connection, _ = server.accept()
                    connection.settimeout(20)
                    with connection:
                        handler(connection)
                except (BrokenPipeError, ConnectionResetError):
                    pass
                except OSError as error:
                    if not (closed_output and error.errno == errno.ENOTCONN):
                        failures.append(repr(error))
                except BaseException as error:
                    failures.append(repr(error))

            worker = threading.Thread(target=serve, daemon=True)
            worker.start()
            return server, worker, failures

        def cleanup(server, worker, failures):
            worker.join(3)
            server.close()
            endpoint.unlink()
            assert not failures and not worker.is_alive(), failures

        def echo(connection):
            while chunk := connection.recv(8192):
                connection.sendall(chunk)

        payload = (b'line\x00\r\n{"jsonrpc":"2.0"}\n' * 140000)
        server, worker, failures = fixture(echo)
        completed = subprocess.run([str(binary), str(endpoint)], input=payload, capture_output=True, timeout=20)
        assert completed.returncode == 0 and completed.stdout == payload, (completed.returncode, len(completed.stdout), completed.stderr)
        cleanup(server, worker, failures)

        reply = b'{"jsonrpc":"2.0","id":"closed","result":{}}\n'
        server, worker, failures = fixture(lambda connection: connection.sendall(reply))
        proxy = subprocess.Popen([str(binary), str(endpoint)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        assert proxy.wait(timeout=3) == 0
        assert proxy.stdout.read() == reply
        proxy.stdin.close()
        cleanup(server, worker, failures)

        server, worker, failures = fixture(lambda connection: connection.sendall(b"x" * (2 << 20)), closed_output=True)
        proxy = subprocess.Popen([str(binary), str(endpoint)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        proxy.stdout.close()
        assert proxy.wait(timeout=3) == 1, "Output faults must be an explicit error, never SIGPIPE death"
        proxy.stdin.close()
        cleanup(server, worker, failures)

        server = socket.socket(socket.AF_UNIX)
        server.bind(str(endpoint)); endpoint.chmod(0o644); server.listen(1)
        completed = subprocess.run([str(binary), str(endpoint)], input=b"", capture_output=True, timeout=2)
        assert completed.returncode == 1
        server.close(); endpoint.unlink()
        endpoint.write_text("existing file")
        completed = subprocess.run([str(binary), str(endpoint)], input=b"", capture_output=True, timeout=2)
        assert completed.returncode == 1 and endpoint.read_text() == "existing file"
        endpoint.unlink()

        if slow:
            server, worker, failures = fixture(lambda connection: connection.sendall(b"x" * (8 << 20)), closed_output=True)
            started = time.monotonic()
            proxy = subprocess.Popen([str(binary), str(endpoint)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            assert proxy.wait(timeout=18) == 1
            assert 14 < time.monotonic() - started < 18
            proxy.stdin.close(); proxy.stdout.close()
            cleanup(server, worker, failures)
        print("PASS real stdio/private Unix proxy: byte identity >3 MiB, half-close drain, peer EOF with stdin open, output fault, private0600 and existing-path rejection" + (", stalled-output deadline" if slow else ""))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument("--slow", action="store_true")
    args = parser.parse_args()
    run(args.sanitize, args.slow)
