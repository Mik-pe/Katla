#!/usr/bin/env python3
"""Place the prepared teen-room blockout with real scene tools, prove undo, then leave it furnished.

Only use this in a disposable editor: loading the prepared room replaces its scene/history.
This deterministic tool journey is not a live model understanding evaluation.
"""
import argparse
import json
from pathlib import Path
from katla_mcp_client import Client

ROOT = Path(__file__).resolve().parents[1]


def placement_bounds(placement):
    position, size = placement['position'], placement['scale']
    return ([p - s / 2 for p, s in zip(position, size)],
            [p + s / 2 for p, s in zip(position, size)])


def validate_placement(placement):
    low, high = placement_bounds(placement)
    assert -3.9 <= low[0] < high[0] <= 3.9, placement
    assert -6.8 <= low[2] < high[2] <= 2.9, placement
    assert low[1] >= -0.00001 and high[1] <= 3, placement
    if high[1] > 0.2:
        assert high[0] <= -1 or low[0] >= 1, ('Central passage occupied', placement)
        for x in [-2.2, 2.2]:
            assert (high[0] <= x - 0.8 or low[0] >= x + 0.8 or
                    high[2] <= -7 or low[2] >= -5), ('Door approach occupied', placement)


def run(args):
    plan = json.loads((ROOT / 'assets/scenes/teen-room-plan.json').read_text())
    for placement in plan['placements']:
        validate_placement(placement)
    client = Client(socket_path=args.socket, command=args.stdio_command)
    try:
        client.tool('load_scene', {'path': str(ROOT / 'assets/scenes/shared-room.katla')})
        client.view('set_camera', **plan['camera'])
        baseline = client.view('select', entity_id=None, output=Path(args.output) / 'before')
        assert baseline['selected_entity_id'] is None
        query, _ = client.tool('query_entities', {'position': [0, 1.5, -2], 'radius': 12, 'limit': 256})
        entities = query['data']['entities']
        assert {'Dörr vänster', 'Dörr höger', 'Fönster', 'Låg byrå'} <= {e['name'] for e in entities}
        resources, _ = client.tool('list_resources', {'path': 'resources/models'})
        model_entries = [e for e in resources['entries'] if e['path'].lower().endswith(('.gltf', '.glb'))]
        assert model_entries, resources
        before_ids = {e['entity_id'] for e in entities}
        placed = 0
        try:
            for placement in plan['placements']:
                client.tool('spawn_entity', placement)
                placed += 1
            furnished = client.view(output=Path(args.output) / 'furnished')
            all_entities, _ = client.tool('query_entities', {'name_filter': 'Blockout -', 'limit': 256})
            additions = all_entities['data']['entities']
            assert len(additions) == placed
            by_name = {e['name']: e for e in additions}
            for placement in plan['placements']:
                bounds = by_name[placement['name']]['bounds']
                assert bounds is not None
                assert all(abs(a-b) < 0.001 for a,b in zip(bounds['center'],placement['position']))
                assert all(abs(a-b/2) < 0.001 for a,b in zip(bounds['extent'],placement['scale']))
        finally:
            for _ in range(placed):
                client.view('undo')
        restored = client.view(output=Path(args.output) / 'restored')
        after, _ = client.tool('query_entities', {'position': [0, 1.5, -2], 'radius': 12, 'limit': 256})
        assert {e['entity_id'] for e in after['data']['entities']} == before_ids
        assert restored['camera'] == baseline['camera']
        for placement in plan['placements']:
            client.tool('spawn_entity', placement)
        final = client.view(output=Path(args.output) / 'ready')
        receipt = {'checks': 'PASS', 'kind': 'real scene-tool blockout, not live-model QA',
                   'placements': placed, 'resources': resources, 'model_inventory': model_entries,
                   'baseline_submission': baseline['submission'],
                   'furnished_submission': furnished['submission'],
                   'undo_submission': restored['submission'], 'final_submission': final['submission']}
        Path(args.output, 'receipt.json').write_text(json.dumps(receipt, ensure_ascii=False, indent=2))
        print(json.dumps(receipt, ensure_ascii=False, indent=2))
    finally:
        client.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    transport = parser.add_mutually_exclusive_group(required=True)
    transport.add_argument('--socket')
    transport.add_argument('--stdio-command', nargs=argparse.REMAINDER)
    parser.add_argument('--output', default='/tmp/katla-teen-room-proof')
    run(parser.parse_args())
