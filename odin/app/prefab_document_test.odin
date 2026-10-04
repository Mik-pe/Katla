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
import "core:mem"
import "core:encoding/json"

@(private="file")
Prefab_Test_Participant :: struct { reject:bool,prepares,commits,rollbacks:int,allocator:mem.Allocator }
@(private="file")
prefab_test_prepare :: proc(state:rawptr,owner:^Authoring,ids:[]ecs.Entity_Id,mode:Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
    participant:=cast(^Prefab_Test_Participant)state; participant.prepares+=1
    assert(len(ids)>0); for id in ids { assert(ecs.entity_exists(&owner.world,id)) }
    if participant.reject { return nil,.Invalid_Operation }
    token:=new(int,participant.allocator); token^=int(mode); return token,.None
}
@(private="file")
prefab_test_finish :: proc(state,token:rawptr,commit:bool) { participant:=cast(^Prefab_Test_Participant)state; if commit { participant.commits+=1 } else { participant.rollbacks+=1 }; mem.free(token,participant.allocator) }

@(test)
test_actual_chair_prefab_insertion_owned_history_and_native_preparation_failure :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-prefab-insert-*",context.allocator)
    testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    for folder in ([3]string{"resources","resources/meshes","resources/prefabs"}) { path:=strings.concatenate({directory,"/",folder}); testing.expect_value(t,os.make_directory(path),os.Error(nil)); delete(path) }
    for file in ([3]string{"prefabs/chair.katprefab","meshes/chair-frame.katmesh","meshes/chair-cushion.katmesh"}) {
        fixture:=strings.concatenate({"resources/",file}); data,read_error:=os.read_entire_file(fixture,context.allocator); testing.expect(t,read_error==nil); delete(fixture)
        target:=strings.concatenate({directory,"/resources/",file}); testing.expect_value(t,os.write_entire_file(target,data),os.Error(nil)); delete(target); delete(data)
    }
    resource_root:=strings.concatenate({directory,"/resources"}); defer delete(resource_root)
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    testing.expect_value(t,asset_resources_init(&owner,directory,resource_root),resources.Error.None)
    hidden:=ecs.spawn(&owner.world,struct {hidden:Editor_Hidden}{})
    existing:=ecs.spawn(&owner.world,struct {transform:Scene_Transform}{{local=km.TRANSFORM_IDENTITY}})
    participant:=Prefab_Test_Participant{reject=true,allocator=owner.world.allocator}
    ecs.insert_resource(&owner.world,Scene_Participant{&participant,prefab_test_prepare,prefab_test_finish})
    args:string=`{"action":"instantiate","path":"resources/prefabs/chair.katprefab","name":"Seat 4","position":[4,0,2],"rotation":[0,0,0,1],"scale":[2,1,1]}`
    decoded,decode_error:=asset.prefab_decode(transmute([]byte)args); defer asset.prefab_destroy(&decoded); testing.expect_value(t,decode_error,asset.Error.None)
    failed,failed_group:=asset_authoring_execute(&owner,decoded.request); defer editor.tool_result_destroy(&failed); defer editor.undo_group_destroy(&failed_group)
    testing.expect(t,failed.error==.Invalid_Operation && len(failed.entities)==0 && owner.world.live_count==2 && participant.prepares==1 && participant.commits==0)
    identity_before,_:=ecs.get_resource(&owner.world,Scene_Identity); testing.expect_value(t,identity_before.next_entity_id,u64(1))
    participant.reject=false
    result,group:=asset_authoring_execute(&owner,decoded.request); defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&group)
    testing.expect(t,result.error==.None && len(result.entities)==3 && owner.world.live_count==5 && participant.commits==1)
    if len(result.entities)!=3 { return }
    read_result,read_group:=asset_authoring_execute(&owner,{action=.Read,path="resources/prefabs/chair.katprefab"}); defer editor.tool_result_destroy(&read_result); defer editor.undo_group_destroy(&read_group)
    read_tree,read_parse_error:=json.parse(read_result.data,spec=.JSON,parse_integers=true); defer json.destroy_value(read_tree)
    testing.expect(t,read_result.error==.None && read_parse_error==nil && !strings.contains(string(read_result.data),"__variant"))
    if read_parse_error==nil {
        document:=read_tree.(json.Object)["document"]
        written,write_group:=asset_authoring_execute(&owner,{action=.Write,path="resources/prefabs/chair-copy.katprefab",document=document}); defer editor.tool_result_destroy(&written); defer editor.undo_group_destroy(&write_group)
        testing.expect(t,written.error==.None && strings.contains(string(written.data),`"published":true`))
        roots:=ecs.get_resource_mut(&owner.world,Asset_Roots); bytes,read_error:=resources.read_text(&roots.project,"resources/prefabs/chair-copy.katprefab"); defer delete(bytes)
        testing.expect(t,read_error==.None && strings.contains(string(bytes),"source:MeshAsset(path:Resource(") && strings.contains(string(bytes),"kind:Static") && strings.contains(string(bytes),"collider_shape:Trimesh"))
        copy_result,copy_group:=asset_authoring_execute(&owner,{action=.Read,path="resources/prefabs/chair-copy.katprefab"}); defer editor.tool_result_destroy(&copy_result); defer editor.undo_group_destroy(&copy_group); testing.expect_value(t,copy_result.error,editor.Scene_Error.None)
    }
    root:=result.entities[0]; label,_:=ecs.get_component(&owner.world,root,Scene_Name); transform,_:=ecs.get_component(&owner.world,root,Scene_Transform)
    testing.expect(t,label.name=="Seat 4" && transform.local.position==km.Vec3{4,0,2} && transform.local.scale==km.Vec3{2,1,1})
    for entity in result.entities[1:] {
        parent,has_parent:=ecs.get_component(&owner.world,entity,Scene_Parent); mesh,has_mesh:=ecs.get_component(&owner.world,entity,Scene_Mesh)
        testing.expect(t,has_parent && parent.entity==root && has_mesh && len(mesh.geometry.vertices)>0)
        if name,ok:=ecs.get_component(&owner.world,entity,Scene_Name); ok && name.name=="Frame" { body,has_body:=ecs.get_component(&owner.world,entity,Physics_Body); testing.expect(t,has_body && body.shape.kind==.Trimesh && len(mesh.geometry.vertices)==144) }
    }
    broken_path:=strings.concatenate({resource_root,"/meshes/chair-frame.katmesh"}); defer delete(broken_path); testing.expect_value(t,os.write_entire_file(broken_path,"new broken revision"),os.Error(nil))
    for _ in 0..<3 {
        testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.None)
        testing.expect(t,owner.world.live_count==2 && ecs.entity_exists(&owner.world,hidden) && ecs.entity_exists(&owner.world,existing))
        testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.None)
        parent_count:=0
        for entity in group.entities { if parent,has_parent:=ecs.get_component(&owner.world,entity,Scene_Parent); has_parent { parent_count+=1; testing.expect(t,ecs.entity_exists(&owner.world,parent.entity)); mesh,ok:=ecs.get_component(&owner.world,entity,Scene_Mesh); testing.expect(t,ok && len(mesh.geometry.vertices)>0) } }
        testing.expect_value(t,parent_count,2)
    }
    snapshot,capture_error:=scene_snapshot_capture(&owner); defer scene_snapshot_destroy(&snapshot); testing.expect_value(t,capture_error,editor.Scene_Error.None)
    participant.reject=true; before:=owner.world.live_count
    testing.expect_value(t,scene_snapshot_restore(&owner,&snapshot),editor.Scene_Error.Invalid_Operation)
    testing.expect(t,owner.world.live_count==before && ecs.entity_exists(&owner.world,existing))
    participant.reject=false
    commits_before_replace:=participant.commits
    testing.expect_value(t,scene_snapshot_restore(&owner,&snapshot),editor.Scene_Error.None)
    testing.expect(t,owner.world.live_count==before && ecs.entity_exists(&owner.world,hidden) && !ecs.entity_exists(&owner.world,existing) && participant.commits==commits_before_replace+1)
}

@(test)
test_prefab_topology_and_scene_unknown_component_boundaries :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    text:string=`{"version":1,"root":1,"scene":{"version":3,"name":"Fixture","next_entity_id":3,"entities":[{"id":1},{"id":2,"parent":1,"components":{"game.missing":{"version":4,"data":"(target:1)"}}}]}}`
    tree,parse_error:=json.parse(transmute([]byte)text,spec=.JSON,parse_integers=true); defer json.destroy_value(tree); testing.expect(t,parse_error==nil)
    prefab,err:=prefab_document_decode(&owner,tree,"resources/template.katprefab"); defer prefab_document_destroy(&prefab); testing.expect_value(t,err,editor.Scene_Error.Component_Not_Found)
    scene:=tree.(json.Object)["scene"]
    snapshot,decode_error:=scene_document_decode(&owner,scene,"scene.katla"); defer scene_snapshot_destroy(&snapshot); testing.expect_value(t,decode_error,editor.Scene_Error.None)
    testing.expect_value(t,scene_snapshot_restore(&owner,&snapshot),editor.Scene_Error.None)
    ids:=ecs.entity_ids(&owner.world); defer delete(ids); unknown_count:=0
    for id in ids { if unknown,present:=ecs.get_component(&owner.world,id,Scene_Unknown); present { unknown_count+=1; testing.expect(t,strings.contains(string(unknown.components),"game.missing") && strings.contains(string(unknown.components),"target:1")) } }
    testing.expect_value(t,unknown_count,1)
    rows:=scene.(json.Object)["entities"].(json.Array); child:=rows[1].(json.Object); removed:=child["components"]; for key in child { if key=="components" { delete_key(&child,key); delete(key); break } }; json.destroy_value(removed); child["parent"]=json.Integer(2)
    invalid,invalid_error:=prefab_document_decode(&owner,tree,"template.katprefab"); defer prefab_document_destroy(&invalid); testing.expect_value(t,invalid_error,editor.Scene_Error.Invalid_Operation)
}
