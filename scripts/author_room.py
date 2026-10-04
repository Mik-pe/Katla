#!/usr/bin/env python3
"""Build a named room in the current Katla editor using live scene and material tools.

The interior is width x depth meters, Y is up, and origin is the floor center.
The front (+Z) wall has a centered doorway. No scene is replaced or saved unless
--save is specified. --dry-run emits a reviewable plan without connecting.
"""
import argparse
import json
import math
from pathlib import Path

from katla_mcp_client import Client


def room_plan(name, size, origin, thickness=0.15, doorway=(1.1, 2.2), ceiling=False):
    """Describe a room with walls outside its usable interior and a clear doorway."""
    width, height, depth = size
    door_width, door_height = doorway
    numbers = [*size, *origin, thickness, *doorway]
    if not all(math.isfinite(v) for v in numbers):
        raise ValueError('Room dimensions and origin must be finite')
    if not name.strip() or min(size) <= 0 or thickness <= 0:
        raise ValueError('Provide a name and positive room dimensions/thickness')
    if not 0 < door_width < width or not 0 < door_height < height:
        raise ValueError('Doorway must fit inside the front wall')
    if max(size) > 1000 or max(abs(v) for v in origin) > 1_000_000 or thickness > min(size):
        raise ValueError('Room dimensions exceed usable bounds')
    x, y, z = origin
    front = z + depth / 2 + thickness / 2
    side_width = (width - door_width) / 2

    def box(label, position, scale, preset):
        return {'name': f'{name} / {label}', 'shape': 'cube',
                'position': position, 'scale': scale, 'preset': preset}

    plan = [
        box('Floor', [x, y - thickness / 2, z],
            [width + 2 * thickness, thickness, depth + 2 * thickness], 'oak'),
        box('Back wall', [x, y + height / 2, z - depth / 2 - thickness / 2],
            [width + 2 * thickness, height, thickness], 'plaster'),
        box('Left wall', [x - width / 2 - thickness / 2, y + height / 2, z],
            [thickness, height, depth], 'plaster'),
        box('Right wall', [x + width / 2 + thickness / 2, y + height / 2, z],
            [thickness, height, depth], 'plaster'),
        box('Front left', [x - door_width / 2 - side_width / 2, y + height / 2, front],
            [side_width, height, thickness], 'plaster'),
        box('Front right', [x + door_width / 2 + side_width / 2, y + height / 2, front],
            [side_width, height, thickness], 'plaster'),
        box('Door lintel', [x, y + (height + door_height) / 2, front],
            [door_width, height - door_height, thickness], 'plaster'),
    ]
    if ceiling:
        plan.append(box('Ceiling', [x, y + height + thickness / 2, z],
                        [width + 2 * thickness, thickness, depth + 2 * thickness], 'plaster'))
    return plan


def build_room(client, plan):
    """Apply the plan; undo only this invocation's successful edits on failure."""
    created = []
    operations = 0
    try:
        for part in plan:
            arguments = {key: value for key, value in part.items() if key != 'preset'}
            result, _ = client.tool('spawn_entity', arguments)
            operations += 1
            entity_id = result['entity_ids'][0]
            created.append({**part, 'entity_id': entity_id})
        for preset in sorted({part['preset'] for part in created}):
            ids = [part['entity_id'] for part in created if part['preset'] == preset]
            client.tool('material', {'action': 'set', 'entity_ids': ids, 'preset': preset})
            operations += 1
        return {'parts': created, 'undo_steps': operations,
                'coordinate_contract': 'Meters, Y up; position is each box center. Front is +Z.'}
    except Exception:
        for _ in range(operations):
            client.view('undo')
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--socket', default='/tmp/katla-editor.sock')
    parser.add_argument('--name', default='Room')
    parser.add_argument('--size', nargs=3, type=float, metavar=('WIDTH', 'HEIGHT', 'DEPTH'), default=[6, 3, 8])
    parser.add_argument('--origin', nargs=3, type=float, default=[0, 0, 0])
    parser.add_argument('--doorway', nargs=2, type=float, default=[1.1, 2.2])
    parser.add_argument('--thickness', type=float, default=0.15)
    parser.add_argument('--ceiling', action='store_true')
    parser.add_argument('--dry-run', action='store_true')
    parser.add_argument('--save', help='Explicit .katla destination; omitted leaves scene unsaved')
    parser.add_argument('--output', help='Optional receipt JSON path')
    args = parser.parse_args()
    plan = room_plan(args.name, args.size, args.origin, args.thickness, args.doorway, args.ceiling)
    if args.dry_run:
        print(json.dumps({'parts': plan}, indent=2))
        return
    client = Client(socket_path=args.socket)
    try:
        receipt = build_room(client, plan)
        if args.save:
            client.tool('save_scene', {'path': args.save})
            receipt['saved_scene'] = args.save
        if args.output:
            Path(args.output).write_text(json.dumps(receipt, indent=2))
        print(json.dumps(receipt, indent=2))
    finally:
        client.close()


if __name__ == '__main__':
    main()
