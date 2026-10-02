"""Small JSONL MCP client for an existing socket or an isolated stdio editor."""
import base64
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import time


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
            self.info = self.rpc('initialize', {
                'protocolVersion': '2024-11-05', 'capabilities': {},
                'clientInfo': {'name': 'katla-shared-view-validation', 'version': '1'}})
            self.write({'jsonrpc': '2.0', 'method': 'notifications/initialized'})
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
        line, _, rest = self.buffer.partition(b'\n')
        self.buffer = bytearray(rest)
        return json.loads(line)

    def rpc(self, method, params):
        self.sequence += 1
        request_id = self.sequence
        self.write({'jsonrpc': '2.0', 'id': request_id, 'method': method, 'params': params})
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
        text = next(c['text'] for c in result['content'] if c['type'] == 'text')
        try:
            data = json.loads(text)
        except json.JSONDecodeError:
            data = {'error': text}
        if isinstance(data, dict) and 'success' in data:
            if not allow_error:
                assert data['success'], data
            data = data.get('data', data)
        return data, result

    def view(self, action='observe', output=None, **kwargs):
        data, result = self.tool('editor_view', {'action': action, **kwargs})
        assert 'submission' in data and data['returned_at_frame'] >= data['frame'], data
        image = next(c for c in result['content'] if c['type'] == 'image')
        assert image['mimeType'] == 'image/png'
        png = base64.b64decode(image['data'], validate=True)
        assert png.startswith(b'\x89PNG\r\n\x1a\n')
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
