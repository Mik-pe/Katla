"""Bounded JSONL MCP client for the canonical Odin scene owner or its proxy."""
import base64
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import time

META = {"io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "katla-authoring", "version": "1"}}
MAX_REPLY_BYTES = 32 << 20


class Client:
    def __init__(self, socket_path=None, command=None):
        self.sequence = 0
        self.buffer = bytearray()
        self.socket = None
        self.child = None
        if socket_path:
            self.socket = socket.socket(socket.AF_UNIX)
            self.socket.settimeout(20)
            self.socket.connect(socket_path)
            self.stream = self.socket.makefile('rwb', buffering=0)
            self.reader = self.writer = self.stream
        elif command:
            env = os.environ.copy()
            env.pop('KATLA_MCP_SOCKET', None)
            env['KATLA_CODEX_SOCKET'] = ''
            env['KATLA_CODEX_THREAD'] = ''
            self.child = subprocess.Popen(command, stdin=subprocess.PIPE,
                                          stdout=subprocess.PIPE, env=env)
            self.reader = self.child.stdout
            self.writer = self.child.stdin
        else:
            raise ValueError('Choose an existing socket or explicit stdio command')
        try:
            self.info = self.rpc('server/discover', {})
            assert self.info['supportedVersions'] == ['2026-07-28'], self.info
            self.tools = self.rpc('tools/list', {})['tools']
            assert any(t['name'] == 'editor_view' for t in self.tools)
        except BaseException:
            self.close()
            raise

    def write(self, value):
        self.writer.write((json.dumps(value) + '\n').encode())
        self.writer.flush()

    def read(self, deadline):
        while b'\n' not in self.buffer:
            ready, _, _ = select.select([self.reader], [], [], max(0, deadline - time.monotonic()))
            if not ready:
                raise TimeoutError('Editor protocol did not respond within 20 seconds')
            chunk = os.read(self.reader.fileno(), 65536)
            if not chunk:
                raise ConnectionError('Editor protocol disconnected (EOF)')
            self.buffer.extend(chunk)
            if len(self.buffer) > MAX_REPLY_BYTES:
                raise ValueError('Editor reply exceeds the 32 MiB transport bound')
        line, _, rest = self.buffer.partition(b'\n')
        self.buffer = bytearray(rest)
        return json.loads(line)

    def rpc(self, method, params):
        self.sequence += 1
        request_id = self.sequence
        self.write({'jsonrpc': '2.0', 'id': request_id, 'method': method,
                    'params': {**params, '_meta': META}})
        deadline = time.monotonic() + 20
        while True:
            message = self.read(deadline)
            if message.get('id') == request_id and 'method' not in message:
                assert 'error' not in message, message
                return message['result']

    def tool(self, name, arguments, allow_error=False):
        result = self.rpc('tools/call', {'name': name, 'arguments': arguments})
        if not allow_error:
            assert not result.get('isError', False), result
        if result.get('isError', False):
            text = next(c['text'] for c in result['content'] if c['type'] == 'text')
            return {'error': text}, result
        return result['structuredContent'], result

    def data(self, name, arguments):
        """Return application data; tool() retains the canonical entity_ids envelope."""
        return self.tool(name, arguments)[0]['data']

    def view(self, action='observe', output=None, **kwargs):
        data, result = self.tool('editor_view', {'action': action, **kwargs})
        assert all(isinstance(data[key], str) and data[key].isascii()
                   and data[key].isdecimal() for key in ('frame_id', 'capture_serial', 'submission')), data
        assert data['gpu_provenance']['submission_id'] == data['submission'], data
        image = next(c for c in result['content'] if c['type'] == 'image')
        assert image['mimeType'] == 'image/png'
        png = base64.b64decode(image['data'], validate=True)
        assert len(png) >= 33 and png[:8] == b'\x89PNG\r\n\x1a\n' and png[12:16] == b'IHDR'
        dimensions = [int.from_bytes(png[16:20]), int.from_bytes(png[20:24])]
        assert dimensions == data['image_size'] == [data['width'], data['height']], data
        assert 'image_png_base64' not in data
        if output:
            output = Path(output)
            output.parent.mkdir(parents=True, exist_ok=True)
            output.with_suffix('.png').write_bytes(png)
            output.with_suffix('.json').write_text(json.dumps(data, ensure_ascii=False, indent=2))
        return data

    def close(self):
        for stream in (self.writer, self.reader):
            try:
                stream.close()
            except OSError:
                pass
        if self.socket:
            self.socket.close()
        if self.child:
            try:
                self.child.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.child.terminate()
                try:
                    self.child.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    self.child.kill()
                    self.child.wait()
