#!/usr/bin/env python3
"""Validate native material output and room tools in a disposable Katla editor.

This replaces the test editor's scene, writes a scene under the output directory,
and compares exact RGB pixels from the canonical native capture PNGs.
"""
import argparse
import json
from pathlib import Path
import struct
import zlib

from author_room import build_room, room_plan
from katla_mcp_client import Client

ROOT = Path(__file__).resolve().parents[1]


def capture_rows(path):
    """Read bounded unfiltered RGBA8 rows emitted by app/editor/capture_png.odin."""
    with Path(path).open('rb') as source:
        png = source.read((32 << 20) + 1)
    if len(png) > 32 << 20 or png[:8] != b'\x89PNG\r\n\x1a\n':
        raise ValueError('Invalid or oversized native capture PNG')
    offset, width, height, ended = 8, 0, 0, False
    compressed = bytearray()
    while offset < len(png):
        if offset + 12 > len(png):
            raise ValueError('Truncated PNG chunk')
        length = int.from_bytes(png[offset:offset + 4])
        kind = png[offset + 4:offset + 8]
        end = offset + 8 + length
        if end + 4 > len(png):
            raise ValueError('Truncated PNG payload')
        data = png[offset + 8:end]
        if zlib.crc32(kind + data) != int.from_bytes(png[end:end + 4]):
            raise ValueError('PNG CRC mismatch')
        if kind == b'IHDR':
            if offset != 8 or length != 13:
                raise ValueError('Invalid capture PNG header')
            width, height, bits, color, compression, filtering, interlace = struct.unpack('>IIBBBBB', data)
            if not (0 < width <= 8192 and 0 < height <= 8192 and width * height <= 16 << 20
                    and (bits, color, compression, filtering, interlace) == (8, 6, 0, 0, 0)):
                raise ValueError('Unsupported capture PNG dimensions or layout')
        elif kind == b'IDAT':
            if not width:
                raise ValueError('PNG data precedes header')
            compressed.extend(data)
        elif kind == b'IEND':
            if length or not compressed or end + 4 != len(png):
                raise ValueError('Invalid PNG end')
            ended = True
        else:
            raise ValueError('Unexpected chunk in native capture PNG')
        offset = end + 4
    if not ended:
        raise ValueError('Missing PNG end')
    expected = height * (width * 4 + 1)
    decoder = zlib.decompressobj()
    rows = decoder.decompress(compressed, expected + 1)
    if len(rows) != expected or not decoder.eof or decoder.unconsumed_tail or decoder.unused_data:
        raise ValueError('Invalid or oversized PNG scanlines')
    stride = width * 4 + 1
    if any(rows[start] != 0 for start in range(0, len(rows), stride)):
        raise ValueError('Expected unfiltered native capture rows')
    return width, height, rows


def pixel_changes(a, b):
    width, height, first = capture_rows(a)
    other_width, other_height, second = capture_rows(b)
    if (width, height) != (other_width, other_height):
        raise ValueError('Native captures have different dimensions')
    stride = width * 4 + 1
    return sum(first[start:start + 3] != second[start:start + 3]
               for y in range(height) for start in range(y * stride + 1, (y + 1) * stride, 4))



def run(args):
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    client = Client(socket_path=args.socket, command=args.stdio_command)
    try:
        client.tool('load_scene', {'path': str(ROOT / 'assets/scenes/material-studio.katla')})
        presets = client.data('material', {'action': 'presets'})
        assert len(presets['presets']) == 6, presets
        assets = client.data('search_assets', {'query': 'Lantern', 'extensions': ['glb']})
        assert assets['assets'] == ['models/Lantern.glb'], assets
        model, _ = client.tool('spawn_model', {'path': assets['assets'][0], 'position': [30, 0, 0]})
        assert model['entity_ids'], model
        scene, _ = client.tool('query_entities', {'name_filter': 'Materials / Porcelain', 'limit': 32})
        sphere = next(e for e in scene['data']['entities'] if e['name'] == 'Materials / Porcelain')
        entity_id = sphere['entity_id']
        before = client.data('material', {'action': 'inspect', 'entity_id': entity_id})
        client.view('set_camera', position=[7.5, 5.5, 9], target=[0, 1, -0.6])
        baseline = client.view('select', entity_id=entity_id, output=output / '01-before')
        client.tool('material', {'action': 'set', 'entity_ids': [entity_id],
                                 'base_color': [0.05, 0.2, 0.95, 1], 'metallic': 0.15, 'roughness': 0.2})
        changed = client.view(output=output / '02-blue')
        difference = pixel_changes(output / '01-before.png', output / '02-blue.png')
        assert difference > 100, difference
        invalid, _ = client.tool('material', {'action': 'set', 'entity_ids': [entity_id, '18446744073709551615'],
                                             'roughness': 0.8}, allow_error=True)
        assert 'error' in invalid, invalid
        value = client.data('material', {'action': 'inspect', 'entity_id': entity_id})
        assert abs(value['values']['roughness'] - 0.2) < 0.00001, value
        restored = client.view('undo', output=output / '03-undo')
        value = client.data('material', {'action': 'inspect', 'entity_id': entity_id})
        assert value == before, (value, before)
        undo_difference = pixel_changes(output / '01-before.png', output / '03-undo.png')
        assert undo_difference == 0, undo_difference

        plan = room_plan('Agent study', [6, 3, 8], [20, 0, -4])
        receipt = build_room(client, plan)
        query, _ = client.tool('query_entities', {'name_filter': 'Agent study /', 'limit': 32})
        additions = query['data']['entities']
        assert len(additions) == len(plan), additions
        by_name = {e['name']: e for e in additions}
        for part in plan:
            bounds = by_name[part['name']]['bounds']
            assert all(abs(a-b) < 0.001 for a, b in zip(bounds['center'], part['position'])), bounds
            assert all(abs(a-b/2) < 0.001 for a, b in zip(bounds['extent'], part['scale'])), bounds
        saved = output / 'authored.katla'
        client.tool('save_scene', {'path': str(saved)})
        client.tool('load_scene', {'path': str(saved)})
        query, _ = client.tool('query_entities', {'name_filter': 'Agent study /', 'limit': 32})
        assert len(query['data']['entities']) == len(plan), query
        floor_id = next(e['entity_id'] for e in query['data']['entities'] if e['name'].endswith('/ Floor'))
        floor = client.data('material', {'action': 'inspect', 'entity_id': floor_id})
        assert abs(floor['values']['base_color'][0] - 0.55) < 0.00001, floor
        client.view('set_camera', position=[20, 1.7, -1], target=[20, 1, -7])
        client.view('select', entity_id=None, output=output / '04-room')
        result = {'checks': 'PASS', 'material_changed_pixels': difference,
                  'undo_changed_pixels': undo_difference, 'room_parts': len(plan),
                  'room_receipt': receipt, 'saved_scene': str(saved),
                  'submissions': [baseline['submission'], changed['submission'], restored['submission']]}
        (output / 'receipt.json').write_text(json.dumps(result, indent=2))
        print(json.dumps(result, indent=2))
    finally:
        client.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    transport = parser.add_mutually_exclusive_group(required=True)
    transport.add_argument('--socket')
    transport.add_argument('--stdio-command', nargs=argparse.REMAINDER)
    parser.add_argument('--output', default='/tmp/katla-authoring-proof')
    run(parser.parse_args())
