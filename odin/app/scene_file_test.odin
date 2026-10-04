#+test
#+build darwin, linux
package app

import asset "../agent/assets"
import editor "../editor"
import ecs "../ecs"
import resources "../resources"
import km "../math"
import "core:testing"
import "core:os"
import "core:strings"

@(test)
test_scene_file_load_save_as_rebase_unknown_metadata_and_failure_rollback :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-scene-file-*",context.allocator)
    testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    for folder in ([3]string{"resources","objects","levels"}) { path:=strings.concatenate({directory,"/",folder}); testing.expect_value(t,os.make_directory(path),os.Error(nil)); delete(path) }
    resource_root:=strings.concatenate({directory,"/resources"}); defer delete(resource_root)
    mesh_path:=strings.concatenate({directory,"/objects/cube.katmesh"}); defer delete(mesh_path)
    recipe:string=`(version:1,name:"Cube",parts:[(id:"cube",geometry:(kind:"cube",size:(1,2,3)))])`
    testing.expect_value(t,os.write_entire_file(mesh_path,recipe),os.Error(nil))
    source_path:=strings.concatenate({directory,"/objects/initial.katla"}); defer delete(source_path)
    scene:string=`(version:3,name:"Study",author:"Ada",created_at:"2026-10-04",next_entity_id:3,entities:[(id:1,name:"Room",transform:(),source:Empty),(id:2,name:"Crate",parent:1,transform:(position:(1,0,0)),source:MeshAsset(path:Scene("cube.katmesh")),drawable:(metallic:0,roughness:0.8,ao:1),components:{"game.unknown":(version:9,data:"(target:1,health:42)")})])`
    testing.expect_value(t,os.write_entire_file(source_path,scene),os.Error(nil))
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    testing.expect_value(t,asset_resources_init(&owner,directory,resource_root),resources.Error.None)
    hidden:=ecs.spawn(&owner.world,struct {hidden:Editor_Hidden}{})
    args:string=`{"path":"objects/initial.katla"}`; decoded,decode_error:=asset.scene_file_decode("load_scene",transmute([]byte)args); defer asset.scene_file_destroy(&decoded); testing.expect_value(t,decode_error,asset.Error.None)
    loaded,no_history:=scene_file_execute(&owner,decoded.request); defer editor.tool_result_destroy(&loaded); defer editor.undo_group_destroy(&no_history)
    testing.expect(t,loaded.error==.None && len(loaded.entities)==2 && owner.world.live_count==3 && ecs.entity_exists(&owner.world,hidden))
    root,child:ecs.Entity_Id
    for id in loaded.entities { label,_:=ecs.get_component(&owner.world,id,Scene_Name); if label.name=="Room" { root=id } else { child=id } }
    parent,has_parent:=ecs.get_component(&owner.world,child,Scene_Parent); mesh,has_mesh:=ecs.get_component(&owner.world,child,Scene_Mesh)
    testing.expect(t,has_parent && parent.entity==root && has_mesh && mesh.source.root==.Project && mesh.source.path=="objects/cube.katmesh" && len(mesh.geometry.vertices)==24)
    ecs.get_component_mut(&owner.world,child,Scene_Transform).local.position={4,2,1}
    saved,save_group:=scene_file_execute(&owner,{action=.Save,path="levels/study.katla",has_path=true}); defer editor.tool_result_destroy(&saved); defer editor.undo_group_destroy(&save_group)
    testing.expect(t,saved.error==.None && strings.contains(string(saved.data),`"published":true`))
    roots:=ecs.get_resource_mut(&owner.world,Asset_Roots); bytes,read_error:=resources.read_text(&roots.project,"levels/study.katla"); defer delete(bytes)
    testing.expect(t,read_error==.None && strings.contains(string(bytes),"source:MeshAsset(path:File(") && strings.contains(string(bytes),"game.unknown") && strings.contains(string(bytes),"target:1,health:42") && strings.contains(string(bytes),`author:"Ada"`) && strings.contains(string(bytes),"position:("))
    state,_:=ecs.get_resource(&owner.world,Scene_File_State); testing.expect(t,state.path=="levels/study.katla" && state.name=="Study")
    again,again_group:=scene_file_execute(&owner,{action=.Load,path="levels/study.katla",has_path=true}); defer editor.tool_result_destroy(&again); defer editor.undo_group_destroy(&again_group)
    testing.expect(t,again.error==.None && len(again.entities)==2 && !ecs.entity_exists(&owner.world,root) && !ecs.entity_exists(&owner.world,child) && ecs.entity_exists(&owner.world,hidden))
    found:=false
    for id in again.entities { if value,present:=ecs.get_component(&owner.world,id,Scene_Mesh); present && value.source.kind==.Recipe { transform,_:=ecs.get_component(&owner.world,id,Scene_Transform); unknown,has_unknown:=ecs.get_component(&owner.world,id,Scene_Unknown); found=transform.local.position==km.Vec3{4,2,1} && has_unknown && strings.contains(string(unknown.components),"game.unknown") } }; testing.expect(t,found)
    final,final_group:=scene_file_execute(&owner,{action=.Save,path="objects/portable.katla",has_path=true}); defer editor.tool_result_destroy(&final); defer editor.undo_group_destroy(&final_group); testing.expect_value(t,final.error,editor.Scene_Error.None)
    portable,portable_error:=resources.read_text(&roots.project,"objects/portable.katla"); defer delete(portable); testing.expect(t,portable_error==.None && strings.contains(string(portable),`path:Scene("cube.katmesh",`))
    implicit,implicit_group:=scene_file_execute(&owner,{action=.Save}); defer editor.tool_result_destroy(&implicit); defer editor.undo_group_destroy(&implicit_group); testing.expect_value(t,implicit.error,editor.Scene_Error.None)
    old_state,_:=ecs.get_resource(&owner.world,Scene_File_State)
    testing.expect_value(t,os.write_entire_file(mesh_path,"broken mesh"),os.Error(nil))
    rejected,rejected_group:=scene_file_execute(&owner,{action=.Load,path="objects/initial.katla",has_path=true}); defer editor.tool_result_destroy(&rejected); defer editor.undo_group_destroy(&rejected_group)
    new_state,_:=ecs.get_resource(&owner.world,Scene_File_State)
    testing.expect(t,rejected.error==.Decode_Failed && owner.world.live_count==3 && new_state.path==old_state.path && ecs.entity_exists(&owner.world,again.entities[0]))
}

@(test)
test_failed_scene_save_preserves_keys_origin_and_decoder_boundary :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-scene-save-failure-*",context.allocator); testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_root:=strings.concatenate({directory,"/resources"}); defer delete(resource_root); testing.expect_value(t,os.make_directory(resource_root),os.Error(nil))
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None); testing.expect_value(t,asset_resources_init(&owner,directory,resource_root),resources.Error.None)
    entity:=ecs.spawn(&owner.world,struct {transform:Scene_Transform}{{local=km.TRANSFORM_IDENTITY}})
    before,_:=ecs.get_resource(&owner.world,Scene_Identity)
    failed,group:=scene_file_execute(&owner,{action=.Save,path="missing/scene.katla",has_path=true}); defer editor.tool_result_destroy(&failed); defer editor.undo_group_destroy(&group)
    after,_:=ecs.get_resource(&owner.world,Scene_Identity); _,has_key:=ecs.get_component(&owner.world,entity,Scene_Key)
    testing.expect(t,failed.error==.Invalid_Operation && before==after && !has_key && !ecs.contains_resource(&owner.world,Scene_File_State))
    for invalid in ([3]string{`{}`,`{"path":null}`,`{"path":"scene.katla","surprise":1}`}) { decoded,err:=asset.scene_file_decode("load_scene",transmute([]byte)invalid); defer asset.scene_file_destroy(&decoded); testing.expect_value(t,err,asset.Error.Invalid_Arguments) }
    for text in ([2]string{`{}`,`{"path":null}`}) { decoded,err:=asset.scene_file_decode("save_scene",transmute([]byte)text); defer asset.scene_file_destroy(&decoded); testing.expect(t,err==.None && !decoded.request.has_path) }
}

Unsigned_Scene_Test :: struct { target:ecs.Entity_Id, count:u64 }
@(test)
test_scene_unsigned_keys_and_custom_values_survive_file_and_prefab_consumers :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-scene-unsigned-*",context.allocator); testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_root:=strings.concatenate({directory,"/resources"}); defer delete(resource_root); testing.expect_value(t,os.make_directory(resource_root),os.Error(nil))
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None); testing.expect_value(t,asset_resources_init(&owner,directory,resource_root),resources.Error.None)
    editor.editor_register(&owner.world,&owner.registry,"game.unsigned",Unsigned_Scene_Test{},spawn_default=false)
    scene:string=`(version:3,name:"Unsigned",next_entity_id:18446744073709551615,entities:[(id:9223372036854775808,name:"Root",transform:(),source:Empty),(id:18446744073709551614,name:"Child",parent:9223372036854775808,transform:(),source:Empty,components:{"game.unsigned":(version:1,data:"(target:9223372036854775808,count:18446744073709551615)")})])`
    source_path:=strings.concatenate({directory,"/unsigned.katla"}); defer delete(source_path); testing.expect_value(t,os.write_entire_file(source_path,scene),os.Error(nil))
    loaded,load_group:=scene_file_execute(&owner,{action=.Load,path="unsigned.katla",has_path=true}); defer editor.tool_result_destroy(&loaded); defer editor.undo_group_destroy(&load_group)
    testing.expect(t,loaded.error==.None && len(loaded.entities)==2); if loaded.error!=.None || len(loaded.entities)!=2 { return }
    root,child:ecs.Entity_Id; for id in loaded.entities { key,_:=ecs.get_component(&owner.world,id,Scene_Key); if key.value==9223372036854775808 { root=id } else { child=id } }
    target,has_target:=ecs.get_component(&owner.world,child,Unsigned_Scene_Test); parent,has_parent:=ecs.get_component(&owner.world,child,Scene_Parent); identity,_:=ecs.get_resource(&owner.world,Scene_Identity)
    testing.expect(t,has_target && has_parent && target.target==root && target.count==max(u64) && parent.entity==root && identity.next_entity_id==max(u64))
    saved,save_group:=scene_file_execute(&owner,{action=.Save}); defer editor.tool_result_destroy(&saved); defer editor.undo_group_destroy(&save_group); testing.expect_value(t,saved.error,editor.Scene_Error.None)
    roots:=ecs.get_resource_mut(&owner.world,Asset_Roots); bytes,read_error:=resources.read_text(&roots.project,"unsigned.katla"); defer delete(bytes); testing.expect(t,read_error==.None && strings.contains(string(bytes),"parent:9223372036854775808") && strings.contains(string(bytes),"count:18446744073709551615") && !strings.contains(string(bytes),"__uint"))
    captured,capture_group:=asset_authoring_execute(&owner,{action=.Capture,path="resources/unsigned.katprefab",root_entity=root}); defer editor.tool_result_destroy(&captured); defer editor.undo_group_destroy(&capture_group); testing.expect_value(t,captured.error,editor.Scene_Error.None)
    // Insertion uses a separate monotonic global key range, independent of template keys.
    ecs.insert_resource(&owner.world,Scene_Identity{max(u64)-2})
    inserted,insert_group:=asset_authoring_execute(&owner,{action=.Instantiate,path="resources/unsigned.katprefab",rotation={0,0,0,1},scale={1,1,1}}); defer editor.tool_result_destroy(&inserted); defer editor.undo_group_destroy(&insert_group)
    testing.expect(t,inserted.error==.None && len(inserted.entities)==2); if inserted.error!=.None || len(inserted.entities)!=2 { return }
    inserted_target,inserted_has_target:=ecs.get_component(&owner.world,inserted.entities[1],Unsigned_Scene_Test); testing.expect(t,inserted_has_target && inserted_target.target==inserted.entities[0] && inserted_target.count==max(u64))
    again,again_group:=scene_file_execute(&owner,{action=.Load,path="unsigned.katla",has_path=true}); defer editor.tool_result_destroy(&again); defer editor.undo_group_destroy(&again_group); testing.expect(t,again.error==.None && len(again.entities)==2 && !ecs.entity_exists(&owner.world,root))
}
