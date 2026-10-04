#!/usr/bin/env python3
"""Protocol/recipe regressions; native GPU journeys remain separate acceptance."""
import base64
import json
from pathlib import Path
import socket
import subprocess
import struct
import tempfile
import threading
import unittest
import zlib
from unittest import mock

import author_room
from validate_authoring import capture_rows, pixel_changes
from katla_mcp_client import Client, META

ROOT = Path(__file__).resolve().parents[1]
PNG = base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==')
TOOLS = json.loads((ROOT / 'odin/agent/tools.json').read_text())


class Endpoint:
    def __init__(self, replies):
        self.directory = tempfile.TemporaryDirectory(prefix='katla-recipe-protocol-')
        self.path = str(Path(self.directory.name) / 'editor.sock')
        self.listener = socket.socket(socket.AF_UNIX)
        self.listener.bind(self.path)
        self.listener.listen(1)
        self.replies = iter(replies)
        self.calls = []
        self.failure = None
        self.thread = threading.Thread(target=self.serve)
        self.thread.start()

    def serve(self):
        try:
            channel, _ = self.listener.accept()
            with channel, channel.makefile('rwb', buffering=0) as stream:
                for line in stream:
                    request = json.loads(line)
                    assert request['params']['_meta'] == META
                    method = request['method']
                    if method == 'server/discover':
                        result = {'supportedVersions': ['2026-07-28']}
                    elif method == 'tools/list':
                        result = {'tools': TOOLS}
                    else:
                        assert method == 'tools/call'
                        self.calls.append(request['params'])
                        result = next(self.replies)
                    stream.write(json.dumps({'jsonrpc': '2.0', 'method': 'fixture/notification'}).encode() + b'\n')
                    stream.write(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}).encode() + b'\n')
        except BaseException as error:
            self.failure = error

    def close(self):
        self.thread.join(timeout=3)
        self.listener.close()
        self.directory.cleanup()
        assert not self.thread.is_alive(), 'Fixture stream did not close'
        if self.failure:
            raise self.failure


def captured_reply():
    metadata = {'frame_id': '9007199254741001', 'capture_serial': '18446744073709551614',
                'submission': '9007199254741000', 'width': 1, 'height': 1, 'image_size': [1, 1],
                'gpu_provenance': {'submission_id': '9007199254741000'},
                'selected_entities': ['18446744073709551615'], 'frustum_candidates': []}
    return {'isError': False, 'structuredContent': metadata,
            'content': [{'type': 'text', 'text': 'Structured content is authoritative'},
                        {'type': 'image', 'mimeType': 'image/png', 'data': base64.b64encode(PNG).decode()}]}


class ProtocolTests(unittest.TestCase):
    def test_native_capture_pixel_comparison_and_corrupt_payload_rejection(self):
        def png(pixels, filtered=0):
            def chunk(kind, value):
                return struct.pack('>I', len(value)) + kind + value + struct.pack('>I', zlib.crc32(kind + value))
            return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 2, 1, 8, 6, 0, 0, 0))
                    + chunk(b'IDAT', zlib.compress(bytes([filtered]) + pixels)) + chunk(b'IEND', b''))
        with tempfile.TemporaryDirectory() as folder:
            first, second = Path(folder) / 'first.png', Path(folder) / 'second.png'
            first.write_bytes(png(bytes([10, 20, 30, 0, 40, 50, 60, 255])))
            second.write_bytes(png(bytes([10, 20, 30, 255, 40, 50, 60, 0])))
            self.assertEqual(pixel_changes(first, second), 0, 'Alpha is independent of exact RGB material changes')
            second.write_bytes(png(bytes([11, 20, 30, 0, 40, 50, 60, 255])))
            self.assertEqual(pixel_changes(first, second), 1)
            for malformed in (png(bytes(8), filtered=1), png(bytes(100)), first.read_bytes()[:-1],
                              first.read_bytes()[:20] + b'\xff' + first.read_bytes()[21:]):
                second.write_bytes(malformed)
                with self.assertRaises(ValueError):
                    capture_rows(second)

    def test_exact_structured_ids_current_discovery_and_paired_image(self):
        endpoint = Endpoint([{'isError': False, 'structuredContent': {'entity_ids': ['18446744073709551615'], 'data': None}},
                             captured_reply()])
        client = Client(socket_path=endpoint.path)
        try:
            spawned, _ = client.tool('spawn_entity', {'name': 'Fixture'})
            self.assertEqual(spawned['entity_ids'], ['18446744073709551615'])
            with tempfile.TemporaryDirectory() as folder:
                path = Path(folder) / 'capture'
                metadata = client.view(output=path)
                self.assertEqual(metadata['frame_id'], '9007199254741001')
                self.assertEqual(path.with_suffix('.png').read_bytes(), PNG)
                self.assertEqual(json.loads(path.with_suffix('.json').read_text()), metadata)
        finally:
            client.close()
            endpoint.close()
        self.assertEqual([call['name'] for call in endpoint.calls], ['spawn_entity', 'editor_view'])

    def test_failed_tool_then_next_reply_and_invalid_image_provenance(self):
        wrong_submission = captured_reply()
        wrong_submission['structuredContent']['gpu_provenance']['submission_id'] = '1'
        wrong_size = captured_reply()
        wrong_size['structuredContent']['image_size'] = [2, 1]
        endpoint = Endpoint([{'isError': True, 'content': [{'type': 'text', 'text': 'Invalid_Operation'}]},
                             wrong_submission, wrong_size, captured_reply()])
        client = Client(socket_path=endpoint.path)
        try:
            failed, result = client.tool('destroy_entity', {'entity_id': '18446744073709551615'}, allow_error=True)
            self.assertTrue(result['isError'])
            self.assertEqual(failed, {'error': 'Invalid_Operation'})
            with tempfile.TemporaryDirectory() as folder:
                path = Path(folder) / 'rejected'
                with self.assertRaises(AssertionError):
                    client.view(output=path)
                with self.assertRaises(AssertionError):
                    client.view(output=path)
                self.assertEqual(list(Path(folder).iterdir()), [])
            self.assertEqual(client.view()['capture_serial'], '18446744073709551614')
        finally:
            client.close()
            endpoint.close()

    def test_room_material_failure_unwinds_only_accepted_edits(self):
        plan = author_room.room_plan('Studio', [6, 3, 8], [0, 0, 0])
        client = mock.Mock()
        responses = [({'entity_ids': [str(index)], 'data': None}, {}) for index in range(len(plan))]
        responses += [({}, {}), RuntimeError('second material group rejected')]
        client.tool.side_effect = responses
        with self.assertRaisesRegex(RuntimeError, 'second material'):
            author_room.build_room(client, plan)
        self.assertEqual(client.view.call_args_list, [mock.call('undo')] * (len(plan) + 1))
        spawn_schema = next(tool['inputSchema'] for tool in TOOLS if tool['name'] == 'spawn_entity')
        for call in client.tool.call_args_list[:len(plan)]:
            self.assertEqual(call.args[0], 'spawn_entity')
            self.assertLessEqual(call.args[1].keys(), spawn_schema['properties'].keys())
            self.assertNotIn('preset', call.args[1])

    def test_recipe_dry_run_and_all_documented_entrypoint_arguments(self):
        result = subprocess.run(['python3', 'scripts/author_room.py', '--dry-run', '--name', 'Studio',
                                 '--size', '6', '3', '8', '--ceiling'], cwd=ROOT, check=True, capture_output=True, text=True)
        plan = json.loads(result.stdout)['parts']
        self.assertEqual(len(plan), 8)
        self.assertEqual(plan[0]['position'], [0, -0.075, 0])
        for script in ('author_room', 'furnish_shared_room', 'validate_shared_view', 'validate_authoring', 'validate_prefabs'):
            result = subprocess.run(['python3', f'scripts/{script}.py', '--help'], cwd=ROOT, check=True, capture_output=True, text=True)
            self.assertIn('socket', result.stdout)


if __name__ == '__main__':
    unittest.main()
