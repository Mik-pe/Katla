#+test
#+build darwin, linux
package app

import asset "../agent/assets"
import editor "../editor"
import ecs "../ecs"
import resources "../resources"
import "core:testing"
import "core:os"
import "core:strings"
import "core:encoding/json"

Marker_Test_Component :: struct { number:struct { __uint:string }, variant:struct { __variant,__payload:string }, value:u64 }
@(private="file")
marker_test_destroy :: proc(value:rawptr) { component:=cast(^Marker_Test_Component)value; delete(component.number.__uint); delete(component.variant.__variant); delete(component.variant.__payload); component^={} }
@(private="file")
marker_test_clone :: proc(dst,src:rawptr) {
    target:=cast(^Marker_Test_Component)dst; source:=cast(^Marker_Test_Component)src; target^=source^
    target.number.__uint=strings.clone(source.number.__uint); target.variant.__variant=strings.clone(source.variant.__variant); target.variant.__payload=strings.clone(source.variant.__payload)
}
@(private="file")
marker_test_check :: proc(t:^testing.T,owner:^Authoring,entity:ecs.Entity_Id) {
    component,present:=ecs.get_component(&owner.world,entity,Marker_Test_Component)
    testing.expect(t,present && component.number.__uint=="18446744073709551615" && component.variant.__variant=="ordinary" && component.variant.__payload=="owned" && component.value==max(u64))
}
@(test)
test_authored_marker_objects_survive_real_scene_and_raw_unsigned_prefab_transport :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-scene-marker-*",context.allocator); testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_root:=strings.concatenate({directory,"/resources"}); defer delete(resource_root); testing.expect_value(t,os.make_directory(resource_root),os.Error(nil))
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None); testing.expect_value(t,asset_resources_init(&owner,directory,resource_root),resources.Error.None)
    editor.editor_register(&owner.world,&owner.registry,"game.marker",Marker_Test_Component{},ecs.Value_Ops{marker_test_destroy,marker_test_clone},spawn_default=false)
    source:string=`(version:3,name:"Markers",next_entity_id:18446744073709551615,entities:[(id:9223372036854775808,transform:(),source:Empty,components:{"game.marker":(version:1,data:"(number:(__uint:\"18446744073709551615\"),variant:(__variant:\"ordinary\",__payload:\"owned\"),value:18446744073709551615)"),"game.unknown":(version:9,data:"(__uint:\"18446744073709551615\")")})])`
    path:=strings.concatenate({directory,"/markers.katla"}); defer delete(path); testing.expect_value(t,os.write_entire_file(path,source),os.Error(nil))
    loaded,load_history:=scene_file_execute(&owner,{action=.Load,path="markers.katla",has_path=true}); defer editor.tool_result_destroy(&loaded); defer editor.undo_group_destroy(&load_history)
    testing.expect(t,loaded.error==.None && len(loaded.entities)==1); if loaded.error!=.None || len(loaded.entities)!=1 { return }; marker_test_check(t,&owner,loaded.entities[0])
    unknown,has_unknown:=ecs.get_component(&owner.world,loaded.entities[0],Scene_Unknown); testing.expect(t,has_unknown && strings.contains(string(unknown.components),"__uint"))
    saved,save_history:=scene_file_execute(&owner,{action=.Save}); defer editor.tool_result_destroy(&saved); defer editor.undo_group_destroy(&save_history); testing.expect_value(t,saved.error,editor.Scene_Error.None)
    again,again_history:=scene_file_execute(&owner,{action=.Load,path="markers.katla",has_path=true}); defer editor.tool_result_destroy(&again); defer editor.undo_group_destroy(&again_history); testing.expect(t,again.error==.None && len(again.entities)==1); if again.error!=.None || len(again.entities)!=1 { return }; marker_test_check(t,&owner,again.entities[0])
    // The public request contains raw unsigned numbers, not an internal marker object.
    arguments:string=`{"action":"write","path":"markers.katprefab","document":{"version":1,"root":9223372036854775808,"scene":{"version":3,"name":"Markers","next_entity_id":18446744073709551615,"entities":[{"id":9223372036854775808,"transform":{},"source":"Empty","components":{"game.marker":{"version":1,"data":"(number:(__uint:\"18446744073709551615\"),variant:(__variant:\"ordinary\",__payload:\"owned\"),value:18446744073709551615)"}}}]}}}`
    decoded,decode_error:=asset.prefab_decode(transmute([]byte)arguments); defer asset.prefab_destroy(&decoded); testing.expect_value(t,decode_error,asset.Error.None); if decode_error!=.None { return }
    written,write_history:=asset_authoring_execute(&owner,decoded.request); defer editor.tool_result_destroy(&written); defer editor.undo_group_destroy(&write_history); testing.expect_value(t,written.error,editor.Scene_Error.None)
    reply,reply_error:=json.parse(written.data,spec=.JSON,parse_integers=true); defer json.destroy_value(reply); testing.expect(t,reply_error==nil && strings.contains(string(written.data),`"root":"9223372036854775808"`) && !strings.contains(string(written.data),"katla.uint"))
    roots:=ecs.get_resource_mut(&owner.world,Asset_Roots); bytes,read_error:=resources.read_text(&roots.project,"markers.katprefab"); defer delete(bytes)
    testing.expect(t,read_error==.None && strings.contains(string(bytes),"root:9223372036854775808") && strings.contains(string(bytes),"next_entity_id:18446744073709551615") && strings.contains(string(bytes),"__uint"))
    ecs.insert_resource(&owner.world,Scene_Identity{1})
    inserted,insert_history:=asset_authoring_execute(&owner,{action=.Instantiate,path="markers.katprefab",rotation={0,0,0,1},scale={1,1,1}}); defer editor.tool_result_destroy(&inserted); defer editor.undo_group_destroy(&insert_history)
    testing.expect(t,inserted.error==.None && len(inserted.entities)==1); if inserted.error==.None && len(inserted.entities)==1 { marker_test_check(t,&owner,inserted.entities[0]) }
}
