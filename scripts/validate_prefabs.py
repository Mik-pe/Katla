#!/usr/bin/env python3
"""Exercise prefab authoring, behavior, preview and persistence in a disposable editor.

Run against an isolated editor: this loads a scene and clears its authoring history.
Native viewport PNGs and a structured receipt are written to --output.
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
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix='.prefab-proof-', dir=ROOT / 'resources'))
    relative = scratch.relative_to(ROOT).as_posix()
    client = None
    checks = []

    def tool(tool_name, **arguments):
        return client.tool(tool_name, arguments)[0]

    def rejected(name, **arguments):
        data, result = client.tool(name, arguments, allow_error=True)
        assert result.get('isError') or data.get('success') is False or 'error' in data, data

    def query():
        result = tool('query_entities', name_filter='Agent FX /', limit=32)
        return {e['name']: e['entity_id'] for e in result['data']['entities']}

    try:
        client = Client(socket_path=args.socket, command=args.stdio_command)
        tool('load_scene', path=str(ROOT / 'assets/scenes/prefab-workshop.katla'))
        description = tool('prefab', action='describe')
        mesh = description['mesh_example']
        mesh_path = relative + '/body.katmesh'
        stats = tool('prefab', action='validate', path=mesh_path, document=mesh)
        assert stats['draws_per_entity'] == 1, stats
        tool('prefab', action='write', path=mesh_path, document=mesh)
        found = tool('search_assets', query=scratch.name, extensions=['katmesh'])
        assert mesh_path in found['project_paths'], found
        template = description['prefab_example']
        root, body = template['scene']['entities']
        root['name'] = 'Agent FX / Root'
        body['name'] = 'Agent FX / Body'
        body['source'] = {'MeshAsset': {'path': {'Scene': 'body.katmesh'}}}
        body['rigid_body'] = {'kind': 'Kinematic'}
        body['collider_shape'] = {'Sphere': 0.3}
        path = relative + '/draft.katprefab'
        tool('prefab', action='write', path=path, document=template)
        instance = tool('prefab', action='instantiate', path=path)
        nodes = {n['name']: n['entity_id'] for n in instance['nodes']}
        root_id, body_id = nodes[root['name']], nodes[body['name']]
        checks.append('mesh_write_search_instantiate_named_nodes')

        trigger = tool('trigger', action='create_box', name='Agent FX / Trigger',
                       position=[0, 0.45, 0], half_extents=[1, 1, 1], rules=[])
        trigger_id = trigger['entity_id']
        tool('set_parent', entity_id=trigger_id, parent_id=root_id)
        behavior = tool('behavior', action='describe')
        particles = behavior['particle_example']
        particles.update(active=False, base_lifetime=10, base_scale=0.08,
                         gravity=0, scale_end=0.08, velocity_direction=[0, 1, 0],
                         velocity_magnitude=1, velocity_cone_angle=0.7,
                         color=[0.1, 1, 0.4, 1])
        tool('behavior', action='set_particles', entity_id=trigger_id, document=particles)
        attached = tool('behavior', action='set_script', entity_id=trigger_id,
                        path=behavior['script_path'])
        assert attached['script'] == behavior['script_path'], attached
        rejected('behavior', action='set_script', entity_id=trigger_id,
                 path='scripts/missing-prefab-proof.luau')
        invalid = copy.deepcopy(particles)
        invalid['base_lifetime'] = -1
        rejected('behavior', action='set_particles', entity_id=trigger_id, document=invalid)
        assert tool('behavior', action='inspect', entity_id=trigger_id) == attached
        # Attachment changes share editor history; undo/redo must restore the exact script.
        client.view('undo')
        assert tool('behavior', action='inspect', entity_id=trigger_id)['script'] is None
        client.view('redo')
        assert tool('behavior', action='inspect', entity_id=trigger_id)['script'] == attached['script']
        tool('trigger', action='set_rules', entity_id=trigger_id, rules=[{
            'event': 'enter', 'other_entity': body_id, 'once': True,
            'actions': [{'action': 'emit', 'name': 'prefab_activated'}]}])
        captured_path = relative + '/interactive.katprefab'
        captured = tool('prefab', action='capture', path=captured_path, root_entity=root_id)
        assert captured['entities'] == 3, captured
        document = tool('prefab', action='read', path=captured_path)
        copied = tool('prefab', action='instantiate', path=captured_path, position=[4, 0, 0])
        assert set(copied['entities']).isdisjoint(instance['entities'] + [trigger_id]), copied
        copy_nodes = {n['name']: n['entity_id'] for n in copied['nodes']}
        copied_rules = tool('trigger', action='inspect', entity_id=copy_nodes['Agent FX / Trigger'])
        assert copied_rules['rules'][0]['other_entity'] == copy_nodes['Agent FX / Body'], copied_rules
        tool('prefab', action='remove', root_entity=copied['root_entity'])
        checks.append('validated_attachments_undo_capture_fresh_ids_remapped_trigger')
        client.view('set_camera', position=[2.6, 1.9, 3.3], target=[0, 0.5, 0])
        before = client.view('focus', entity_id=root_id, output=output / '01-authored')
        baseline_gpu = tool('simulation', action='inspect')['particle_gpu']
        baseline_emissions = baseline_gpu['submitted_emissions'] if baseline_gpu else 0
        assert tool('simulation', action='play')['mode'] == 'playing'
        # Tool calls run on subsequent native frames, allowing queued Luau events to dispatch.
        for _ in range(20):
            feedback = tool('behavior', action='inspect', entity_id=trigger_id)
            if feedback['particles']['active']:
                break
        assert feedback['particles']['active'], feedback
        fired = tool('trigger', action='inspect', entity_id=trigger_id)
        for _ in range(20):
            gpu = tool('simulation', action='inspect')['particle_gpu']
            if gpu and gpu['alive'] >= 32 and gpu['submitted_emissions'] >= baseline_emissions + 32:
                break
        assert gpu and gpu['alive'] >= 32 and gpu['submitted_emissions'] >= baseline_emissions + 32, gpu
        assert fired['fired_once_rules'] == [0] and not fired['last_errors'], fired
        rejected('prefab', action='instantiate', path=captured_path)
        assert tool('simulation', action='pause')['mode'] == 'paused'
        preview = client.view(output=output / '02-script-trigger-particles')
        assert tool('simulation', action='resume')['mode'] == 'playing'
        stopped = tool('simulation', action='stop')
        assert stopped['runtime_ids_replaced'] and stopped['mode'] == 'editing', stopped
        fresh = query()
        trigger_id = fresh['Agent FX / Trigger']
        assert trigger_id != trigger['entity_id'], fresh
        restored = tool('behavior', action='inspect', entity_id=trigger_id)
        assert restored['script'] == attached['script'] and not restored['particles']['active'], restored
        checks.append('native_overlap_luau_particles_pause_resume_stop_restore')
        scene = output / 'authored.katla'
        tool('save_scene', path=str(scene))
        tool('load_scene', path=str(scene))
        fresh = query()
        reloaded = tool('behavior', action='inspect', entity_id=fresh['Agent FX / Trigger'])
        assert reloaded['script'] == restored['script'] and reloaded['particles'] == restored['particles'], reloaded
        rules = tool('trigger', action='inspect', entity_id=fresh['Agent FX / Trigger'])
        assert rules['rules'][0]['other_entity'] == fresh['Agent FX / Body'], rules
        client.view('select', entity_id=fresh['Agent FX / Root'], output=output / '03-reloaded')
        checks.append('scene_reload_preserves_script_particles_internal_trigger_references')
        # Keep the receipt self-contained even though temporary project assets are removed.
        (output / 'interactive.json').write_text(json.dumps(document, indent=2))
        (output / 'mesh.json').write_text(json.dumps(mesh, indent=2))
        receipt = {'checks': 'PASS', 'verified': checks, 'prefab_entities': 3,
                   'mesh_stats': stats, 'trigger_feedback': fired, 'particle_gpu': gpu,
                   'native_submissions': [before['submission'], preview['submission']]}
        (output / 'receipt.json').write_text(json.dumps(receipt, indent=2))
        print(json.dumps(receipt, indent=2))
    finally:
        if client:
            client.close()
        shutil.rmtree(scratch)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    transport = parser.add_mutually_exclusive_group(required=True)
    transport.add_argument('--socket')
    transport.add_argument('--stdio-command', nargs=argparse.REMAINDER)
    parser.add_argument('--output', default='/tmp/katla-prefab-proof')
    run(parser.parse_args())
