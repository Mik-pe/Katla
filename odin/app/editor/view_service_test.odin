#+test
package editor_app
import app ".."
import ecs "../../ecs"
import editor "../../editor"
import render "../render"
import gfx "../../gfx"
import km "../../math"
import "core:encoding/json"
import "core:testing"
import "core:strings"

@(test)
test_view_capture_oversized_real_png_returns_owned_failure_without_history :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    state:State; state_init(&state,&owner); defer state_destroy(&state); shell:=Shell{state=&state}; viewport_grid_init(&shell.viewports)
    probe:=View_Probe{serial=7}; service:View_Service; testing.expect(t,view_service_init(&service,&shell,&probe,view_probe_request)==.None); defer view_service_destroy(&service)
    ticket:=view_test_submit(t,&owner,`{"action":"observe"}`); view_service_tick(&service,0)
    snapshot:=view_test_snapshot(); defer render.picking_snapshot_destroy(&snapshot)
    snapshot.metadata.width=4096; snapshot.metadata.height=1200
    snapshot.color_source.desc.width=4096; snapshot.color_source.desc.height=1200
    snapshot.id_source.desc.width=4096; snapshot.id_source.desc.height=1200
    for pixels in ([2]^gfx.Readback_Data{&snapshot.color,&snapshot.id_pixels}) {
        gfx.readback_data_destroy(pixels)
        pixels^={bytes=make([]byte,4096*1200*4),row_pitch=4096*4,allocator=context.allocator}
    }
    snapshot.color.source=snapshot.color_source; snapshot.id_pixels.source=snapshot.id_source
    png,valid:=capture_png(&snapshot); defer delete(png); testing.expect(t,valid && len(png)>18<<20)
    testing.expect(t,!view_service_capture(&service,&snapshot,nil,png) && len(service.pending)==0 && owner.agent.outstanding==1)
    response,ready:=editor.agent_take_result_for(&owner.agent,ticket); defer editor.agent_response_destroy(&response)
    testing.expect(t,ready && response.result.error==.Invalid_Operation && owner.agent.outstanding==0 && len(owner.agent.session.actions)==0)
    error_value,error:=json.parse(response.result.data); testing.expect(t,error==nil); defer json.destroy_value(error_value)
    testing.expect(t,strings.contains(error_value.(json.Object)["capture_error"].(string),"Reduce the viewport size"))
    late:=view_test_submit(t,&owner,`{"action":"observe"}`); view_service_tick(&service,0)
    testing.expect(t,editor.agent_abandon(&owner.agent,late)); testing.expect(t,!view_service_capture(&service,&snapshot,nil,png) && owner.agent.outstanding==0)
}

View_Probe :: struct { busy:bool,serial:u64,calls,mutations:int,error:editor.Scene_Error }
view_probe_request :: proc(raw:rawptr)->(u64,bool) { probe:=cast(^View_Probe)raw; probe.calls+=1; return probe.serial,!probe.busy }
view_probe_mutation :: proc(raw:rawptr)->editor.Scene_Error { probe:=cast(^View_Probe)raw; probe.mutations+=1; return probe.error }
view_test_submit :: proc(t:^testing.T,owner:^app.Authoring,text:string)->u64 { ticket,error:=editor.agent_submit(&owner.agent,{kind=.Application,tool_name="editor_view",value=transmute([]byte)text},"view-caller"); testing.expect(t,error==.None); return ticket }
view_test_snapshot :: proc()->render.Picking_Snapshot {
    snapshot:=render.Picking_Snapshot{metadata={frame=9,serial=7,width=2,height=2},submission={owner=rawptr(uintptr(1)),id=19,token={owner=rawptr(uintptr(2)),generation=3}},allocator=context.allocator}
    snapshot.color_source={owner=snapshot.submission.owner,submission=snapshot.submission,generation=4,desc={width=2,height=2,depth=1,format=.RGBA8_Unorm}}
    snapshot.id_source={owner=snapshot.submission.owner,submission=snapshot.submission,generation=5,desc={width=2,height=2,depth=1,format=.R32_Uint}}
    snapshot.color={source=snapshot.color_source,row_pitch=8,bytes=make([]byte,16),allocator=context.allocator}; snapshot.id_pixels={source=snapshot.id_source,row_pitch=8,bytes=make([]byte,16),allocator=context.allocator}
    snapshot.id_pixels.bytes[0]=99; snapshot.id_pixels.bytes[4]=1; snapshot.entries=make([]render.Picking_Entry,1); snapshot.entries[0]={1,ecs.Entity_Id(max(u64))}
    return snapshot
}
@(test)
test_view_service_busy_capture_does_not_replay_undo_or_steal_other_reply :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    state:State; state_init(&state,&owner); defer state_destroy(&state)
    shell:=Shell{state=&state}; viewport_grid_init(&shell.viewports)
    probe:=View_Probe{busy=true,serial=7}; service:View_Service
    testing.expect(t,view_service_init(&service,&shell,&probe,view_probe_request)==.None); defer view_service_destroy(&service)
    service.prepare_mutation=view_probe_mutation; service.mutation_state=&probe
    other,error:=editor.agent_submit(&owner.agent,{kind=.Spawn},"other-caller"); testing.expect(t,error==.None)
    ticket:=view_test_submit(t,&owner,`{"action":"undo"}`)
    view_service_tick(&service,0); testing.expect(t,owner.world.live_count==0 && probe.mutations==2 && editor.agent_can_redo(&owner.agent.session))
    for _ in 0..<3 { view_service_tick(&service,0) }; testing.expect_value(t,probe.mutations,2)
    other_response,present:=editor.agent_take_result_for(&owner.agent,other); defer editor.agent_response_destroy(&other_response); testing.expect(t,present && other_response.call_id=="other-caller")
    probe.busy=false; view_service_tick(&service,0)
    snapshot:=view_test_snapshot(); defer render.picking_snapshot_destroy(&snapshot)
    png,valid:=capture_png(&snapshot); defer delete(png); testing.expect(t,valid)
    frozen:=`{"frame_id":"9","capture_serial":"7","width":2,"height":2,"selected_entities":[],"frustum_candidates":[],"undo_available":false,"redo_available":true}`
    testing.expect(t,view_service_capture(&service,&snapshot,transmute([]byte)frozen,png))
    response,ready:=editor.agent_take_result_for(&owner.agent,ticket); defer editor.agent_response_destroy(&response)
    testing.expect(t,ready && response.call_id=="view-caller" && response.result.error==.None && owner.agent.outstanding==0 && len(owner.agent.session.actions)==0 && len(owner.agent.session.redo_actions)==1)
    testing.expect(t,history_apply(&state,true)==.None && owner.world.live_count==1)
}
@(test)
test_view_reply_immutable_provenance_samples_null_selection_and_bounded_candidates :: proc(t:^testing.T) {
    snapshot:=view_test_snapshot(); defer render.picking_snapshot_destroy(&snapshot)
    png,valid:=capture_png(&snapshot); defer delete(png); testing.expect(t,valid)
    frozen:=`{"frame_id":"9","capture_serial":"7","width":2,"height":2,"selected_entities":[],"frustum_candidates":[{"name":"Frozen A"},{"name":"Frozen B"}],"undo_available":true,"redo_available":false}`
    bytes,okay:=view_reply_encode(&snapshot,transmute([]byte)frozen,png,1,context.allocator); defer delete(bytes); testing.expect(t,okay)
    tree,error:=json.parse(bytes,parse_integers=true); testing.expect(t,error==nil); defer json.destroy_value(tree)
    object:=tree.(json.Object); candidates:=object["frustum_candidates"].(json.Array); testing.expect(t,len(candidates)==1 && candidates[0].(json.Object)["name"].(string)=="Frozen A" && object["total"].(json.Integer)==2 && object["truncated"].(bool))
    _,is_null:=object["selected_entity"].(json.Null); testing.expect(t,is_null)
    provenance:=object["gpu_provenance"].(json.Object); testing.expect(t,provenance["submission_id"].(string)=="19" && provenance["frame_generation"].(string)=="3")
    samples:=provenance["samples"].(json.Array); overlay:=samples[1].(json.Object); _,overlay_null:=overlay["entity_id"].(json.Null); testing.expect(t,overlay["raw_id"].(json.Integer)==99 && overlay_null)
    mapped:=samples[2].(json.Object); testing.expect(t,mapped["entity_id"].(string)=="18446744073709551615")
    snapshot.id_pixels.source.generation+=1; rejected,reject_ok:=view_reply_encode(&snapshot,transmute([]byte)frozen,png,64,context.allocator); defer delete(rejected); testing.expect(t,!reject_ok)
}
@(test)
test_view_service_abandon_late_capture_and_editing_gate :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    state:State; state_init(&state,&owner); defer state_destroy(&state); shell:=Shell{state=&state}; viewport_grid_init(&shell.viewports)
    probe:=View_Probe{serial=7}; service:View_Service; testing.expect(t,view_service_init(&service,&shell,&probe,view_probe_request)==.None); defer view_service_destroy(&service)
    service.prepare_mutation=view_probe_mutation; service.mutation_state=&probe
    ticket:=view_test_submit(t,&owner,`{"action":"observe"}`); owner.mode=.Playing; view_service_tick(&service,0); testing.expect(t,editor.agent_is_deferred(&owner.agent,ticket) && probe.mutations==0)
    testing.expect(t,editor.agent_abandon(&owner.agent,ticket)); snapshot:=view_test_snapshot(); defer render.picking_snapshot_destroy(&snapshot)
    testing.expect(t,!view_service_capture(&service,&snapshot,nil,nil) && owner.agent.outstanding==0)
    blocked:=view_test_submit(t,&owner,`{"action":"select","entity_id":null}`); view_service_tick(&service,0)
    response,present:=editor.agent_take_result_for(&owner.agent,blocked); defer editor.agent_response_destroy(&response); testing.expect(t,present && response.result.error==.Editing_Required)
    owner.mode=.Editing; zero:=ecs.create_entity(&owner.world); testing.expect_value(t,zero,ecs.Entity_Id(0))
    testing.expect(t,view_service_apply(&service,{action=.Select,entity=zero,has_entity=true})==.None && state.selection.has_primary && state.selection.primary==0)
    probe.error=.Invalid_Field_Value; old_camera:=shell.viewports.slots[0].camera
    failed:=view_test_submit(t,&owner,`{"action":"set_camera","position":[4,5,6],"target":[1,2,3]}`); view_service_tick(&service,0)
    failure,got_failure:=editor.agent_take_result_for(&owner.agent,failed); defer editor.agent_response_destroy(&failure); testing.expect(t,got_failure && failure.result.error==.Invalid_Field_Value && shell.viewports.slots[0].camera==old_camera && len(service.pending)==0)
    probe.error=.None
    ecs.destroy_entity(&owner.world,zero); testing.expect(t,view_service_apply(&service,{action=.Select,entity=zero,has_entity=true})==.Entity_Not_Found)
}
@(test)
test_view_focus_parent_uses_actual_drawable_subtree_and_exact_camera_pose :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner)
    geometry,error:=app.mesh_cube({1,1,1}); defer app.mesh_geometry_destroy(&geometry); testing.expect(t,error==.None)
    root:=ecs.spawn(&owner.world,struct{transform:app.Scene_Transform,name:app.Scene_Name}{{km.transform(position={10,0,0})},{strings.clone("Parent")}})
    ecs.spawn(&owner.world,struct{transform:app.Scene_Transform,parent:app.Scene_Parent,mesh:app.Scene_Mesh}{{km.transform(position={4,0,0})},{root},{geometry=geometry}})
    state:State; state_init(&state,&owner); defer state_destroy(&state); shell:=Shell{state=&state}; viewport_grid_init(&shell.viewports); shell.viewports.slots[0].bounds={width=100,height=100}
    service:=View_Service{shell=&shell}; testing.expect(t,view_service_apply(&service,{action=.Focus,entity=root,has_entity=true,select=true})==.None)
    camera:=&shell.viewports.slots[0].camera; testing.expect(t,camera.target==km.Vec3{14,0,0} && !camera.focus.active && state.selection.primary==root)
    testing.expect(t,view_service_apply(&service,{action=.Set_Camera,position={4,5,6},target={1,2,3}})==.None)
    eye:=camera_position(camera); testing.expect(t,km.length(eye-km.Vec3{4,5,6})<0.00001)
    before:=camera^; testing.expect(t,view_service_apply(&service,{action=.Set_Camera,position={0,10,0},target={0,0,0}})==.Invalid_Operation && camera^==before)
}
