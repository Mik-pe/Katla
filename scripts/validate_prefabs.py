#!/usr/bin/env python3
"""Validate canonical prefab files/history through a disposable native Odin editor.

The selected editor must use --project as its project root and that project's
resources directory as its resource root. This replaces its scene and history.
"""
import argparse
import copy
import json
from pathlib import Path
import shutil
import tempfile

from katla_mcp_client import Client

ROOT = Path(__file__).resolve().parents[1]


def run(args):
    project = Path(args.project).resolve()
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix='.prefab-proof-', dir=project / 'resources'))
    relative = scratch.relative_to(project).as_posix()
    client = None
    try:
        client = Client(socket_path=args.socket, command=args.stdio_command)
        data = client.data
        client.tool('load_scene', {'path': str(ROOT / 'assets/scenes/shared-room.katla')})
        description = data('prefab', {'action': 'describe'})
        mesh_path = relative + '/seat.katmesh'
        mesh = description['mesh']
        stats = data('prefab', {'action': 'validate', 'path': mesh_path, 'document': mesh})
        assert stats['vertices'] > 0 and stats['triangles'] > 0 and not stats['published'], stats
        written = data('prefab', {'action': 'write', 'path': mesh_path, 'document': mesh})
        assert written['published'], written
        source = data('prefab', {'action': 'read', 'path': 'resources/prefabs/chair.katprefab'})
        template = source['document']
        for row in template['scene']['entities']:
            row['name'] = 'Prefab proof / ' + row['name']
        path = relative + '/chair.katprefab'
        assert data('prefab', {'action': 'write', 'path': path, 'document': template})['published']
        rejected = copy.deepcopy(template)
        rejected['scene']['entities'][1]['id'] = rejected['root']
        failure, failed_reply = client.tool('prefab', {'action': 'write', 'path': path, 'document': rejected}, allow_error=True)
        assert failed_reply['isError'] and 'error' in failure
        assert data('prefab', {'action': 'read', 'path': path})['document'] == template
        inserted, _ = client.tool('prefab', {'action': 'instantiate', 'path': path, 'position': [0, 0, -3]})
        ids = set(inserted['entity_ids'])
        assert len(ids) == 3 and inserted['data']['root_entity'] in ids
        root = inserted['data']['root_entity']
        client.view('select', entity_id=None)
        before = client.view('focus', entity_id=root, output=output / '01-instantiated')

        def query():
            value = data('query_entities', {'name_filter': 'Prefab proof /', 'limit': 32})
            return {row['name']: row for row in value['entities']}

        rows = query()
        assert {row['entity_id'] for row in rows.values()} == ids
        assert all(row['parent_id'] == root for row in rows.values() if row['entity_id'] != root)
        captured_path = relative + '/captured.katprefab'
        captured = data('prefab', {'action': 'capture', 'path': captured_path, 'root_entity': root})
        assert captured['published'] and captured['entity_count'] == 3, captured
        document = data('prefab', {'action': 'read', 'path': captured_path})['document']
        copied, _ = client.tool('prefab', {'action': 'instantiate', 'path': captured_path, 'position': [3, 0, -3]})
        copied_ids = set(copied['entity_ids'])
        assert len(copied_ids) == 3 and ids.isdisjoint(copied_ids)
        copy_root = copied['data']['root_entity']
        client.view('focus', entity_id=copy_root, output=output / '02-copied')
        removed = data('prefab', {'action': 'remove', 'root_entity': copy_root})
        assert removed['removed'] == 3
        client.view('undo', output=output / '03-remove-undo')
        live = data('query_entities', {'name_filter': 'Prefab proof /', 'limit': 32})['entities']
        restored_ids = {row['entity_id'] for row in live} - ids
        assert len(restored_ids) == 3 and restored_ids.isdisjoint(copied_ids)
        client.view('redo', output=output / '04-remove-redo')
        assert {row['entity_id'] for row in query().values()} == ids
        assert data('simulation', {'action': 'play'})['mode'] == 'playing'
        _, play_rejection = client.tool('prefab', {'action': 'instantiate', 'path': captured_path}, allow_error=True)
        assert play_rejection['isError']
        assert data('simulation', {'action': 'pause'})['mode'] == 'paused'
        client.view(output=output / '05-paused')
        stopped = data('simulation', {'action': 'stop'})
        assert stopped['mode'] == 'editing' and stopped['runtime_ids_replaced']
        fresh = query()
        assert len(fresh) == 3 and ids.isdisjoint({row['entity_id'] for row in fresh.values()})
        saved = output / 'authored.katla'
        client.tool('save_scene', {'path': str(saved)})
        client.tool('load_scene', {'path': str(saved)})
        loaded = query()
        assert set(loaded) == set(rows)
        loaded_root = loaded['Prefab proof / Chair']['entity_id']
        assert all(row['parent_id'] == loaded_root for row in loaded.values() if row['entity_id'] != loaded_root)
        final = client.view('focus', entity_id=loaded_root, output=output / '06-reloaded')
        receipt = {'checks': 'PASS', 'mesh_stats': stats, 'document': document,
                   'verified': ['actual_mesh_write', 'rejected_write_preserves_file', 'three_entity_native_prefab',
                                'capture_copy_fresh_ids', 'remove_undo_redo', 'play_admission_gate',
                                'stop_fresh_ids', 'save_reload_hierarchy'],
                   'submissions': [before['submission'], final['submission']]}
        (output / 'receipt.json').write_text(json.dumps(receipt, ensure_ascii=False, indent=2))
        print(json.dumps(receipt, ensure_ascii=False, indent=2))
    finally:
        if client:
            client.close()
        shutil.rmtree(scratch)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    transport = parser.add_mutually_exclusive_group(required=True)
    transport.add_argument('--socket')
    transport.add_argument('--stdio-command', nargs=argparse.REMAINDER, help='Explicit native editor MCP proxy command')
    parser.add_argument('--project', default=str(ROOT))
    parser.add_argument('--output', default='/tmp/katla-prefab-proof')
    run(parser.parse_args())
