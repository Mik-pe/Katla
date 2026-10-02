#!/usr/bin/env python3
"""Validate native material output and room tools in a disposable Katla editor.

This replaces the test editor's scene, writes a scene under the output directory,
and uses ImageMagick to compare committed viewport pixels.
"""
import argparse
import json
import subprocess
from pathlib import Path

from author_room import build_room, room_plan
from katla_mcp_client import Client

ROOT = Path(__file__).resolve().parents[1]


def pixel_changes(a, b):
    result = subprocess.run(['magick', str(a), str(b), '-alpha', 'off',
                             '-compose', 'Difference', '-composite', '-colorspace', 'gray',
                             '-threshold', '0', '-format', '%[fx:mean*w*h]', 'info:'],
                            capture_output=True, text=True, check=True)
    return round(float(result.stdout.strip()))



def run(args):
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    client = Client(socket_path=args.socket, command=args.stdio_command)
    try:
        client.tool('load_scene', {'path': str(ROOT / 'assets/scenes/material-studio.katla')})
        presets, _ = client.tool('material', {'action': 'presets'})
        assert len(presets['presets']) == 6, presets
        assets, _ = client.tool('search_assets', {'query': 'Lantern', 'extensions': ['glb']})
        assert assets['assets'] == ['models/Lantern.glb'], assets
        model, _ = client.tool('spawn_model', {'path': assets['assets'][0], 'position': [30, 0, 0]})
        assert model['entities'], model
        scene, _ = client.tool('query_entities', {'name_filter': 'Materials / Porcelain', 'limit': 32})
        sphere = next(e for e in scene['data']['entities'] if e['name'] == 'Materials / Porcelain')
        entity_id = sphere['entity_id']
        before, _ = client.tool('material', {'action': 'inspect', 'entity_id': entity_id})
        client.view('set_camera', position=[7.5, 5.5, 9], target=[0, 1, -0.6])
        baseline = client.view('select', entity_id=entity_id, output=output / '01-before')
        client.tool('material', {'action': 'set', 'entity_ids': [entity_id],
                                 'base_color': [0.05, 0.2, 0.95, 1], 'metallic': 0.15, 'roughness': 0.2})
        changed = client.view(output=output / '02-blue')
        difference = pixel_changes(output / '01-before.png', output / '02-blue.png')
        assert difference > 100, difference
        invalid, _ = client.tool('material', {'action': 'set', 'entity_ids': [entity_id, '0'],
                                             'roughness': 0.8}, allow_error=True)
        assert invalid.get('success') is False, invalid
        value, _ = client.tool('material', {'action': 'inspect', 'entity_id': entity_id})
        assert abs(value['values']['roughness'] - 0.2) < 0.00001, value
        restored = client.view('undo', output=output / '03-undo')
        value, _ = client.tool('material', {'action': 'inspect', 'entity_id': entity_id})
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
        floor, _ = client.tool('material', {'action': 'inspect', 'entity_id': floor_id})
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
