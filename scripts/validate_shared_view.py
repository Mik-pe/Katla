#!/usr/bin/env python3
"""Exercise real MCP against the already open shared-room editor; never saves."""
import argparse
import json
from pathlib import Path
from katla_mcp_client import Client

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('socket')
parser.add_argument('output', nargs='?', default='/tmp/katla-shared-view-proof')
parser.add_argument('--no-save', action='store_true')
args = parser.parse_args()

client = Client(socket_path=args.socket)
out = Path(args.output)
save = not args.no_save
if save: out.mkdir(parents=True, exist_ok=True)

rpc = client.rpc
tool = client.tool

def view(action, label=None, **kwargs):
    return client.view(action, output=out / label if label and save else None, **kwargs)

info = client.info
print('MCP initialize/tools/list:', info['serverInfo'], len(client.tools), flush=True)
tool('load_scene', {'path':str(Path(__file__).resolve().parents[1]/'assets/scenes/shared-room.katla')})
view('set_camera',position=[0,1.6,1],target=[0,1.2,-6])
room=view('select','room',entity_id=None)
assert room['selected_entity_id'] is None
candidates={c['name']:c for c in room['candidates']}
left=candidates['Dörr vänster']; right=candidates['Dörr höger']
assert left['screen_rect'][2] < .5 < right['screen_rect'][0],(left,right)
assert 'Föremål bakom kameran' not in candidates
assert all(c['visibility']=='frustum_candidate_occlusion_unknown' for c in room['candidates'])
limited=view('observe',limit=1)
assert limited['truncated'] and len(limited['candidates'])==1 and limited['candidate_count']>1
near,_=tool('query_entities',{'position':[0,1.6,1],'radius':15})
assert any(e['name']=='Föremål bakom kameran' for e in near['data']['entities'])
resources,_=tool('list_resources',{'path':'resources','filter':'gltf'})
assert resources['entries'],resources
print('Room without selection:',len(room['candidates']),'candidates; behind-camera object excluded; spatial query includes it',flush=True)
focused=view('focus','focused-left',entity_id=left['entity_id'],select=False)
assert focused['selected_entity_id'] is None
assert focused['center_pick']==left['entity_id'],focused
selected=view('select','selected-left',entity_id=left['entity_id'])
assert selected['selected_entity_id']==left['entity_id']
attrs,_=tool('get_component_attributes',{'entity_id':left['entity_id'],'component':'TransformComponent'})
baseline=next(f['value'] for f in attrs['data']['fields'] if f['name']=='scale_x')
tool('set_field',{'entity_id':left['entity_id'],'component':'TransformComponent','field':'scale_x','value':baseline*1.5})
changed=view('observe','widened-left')
wide=next(c for c in changed['candidates'] if c['entity_id']==left['entity_id'])
assert abs(wide['world_bounds']['extent'][0]-left['world_bounds']['extent'][0]*1.5)<.001
restored=view('undo','restored-left')
original=next(c for c in restored['candidates'] if c['entity_id']==left['entity_id'])
assert abs(original['world_bounds']['extent'][0]-left['world_bounds']['extent'][0])<.001
rejected,_=tool('editor_view',{'action':'focus','entity_id':'18446744073709551615'},allow_error=True)
assert 'error' in rejected
# Genuine placement with the same scene tools, then remove it through agent undo.
view('set_camera',position=[0,1.6,1],target=[0,1.2,-6])
view('select',entity_id=None)
spawned,_=tool('spawn_entity',{'position':[1,0.5,-3],'scale':[1,.5,1],'name':'Provstol','shape':'cube'})
chair_view=view('observe','placed-chair')
assert any(c['name']=='Provstol' for c in chair_view['candidates'])
removed=view('undo')
assert not any(c['name']=='Provstol' for c in removed['candidates'])
print('Focus/selection + GPU center pick, widen + undo, stale ID rejection, resource listing, real placement + undo: PASS',flush=True)
view('set_camera',position=[0,1.6,1],target=[0,1.2,-6])
view('select','ready',entity_id=None)
if save: (out/'receipt.json').write_text(json.dumps({'server':info,'checks':'PASS','door_left':left['entity_id'],'door_right':right['entity_id'],'resource_result':resources},ensure_ascii=False,indent=2))
print('Editor ready; evidence:',out,flush=True)

client.close()
