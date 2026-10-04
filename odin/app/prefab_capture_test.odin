#+test
#+build darwin, linux
package app

import asset "../agent/assets"
import editor "../editor"
import ecs "../ecs"
import resources "../resources"
import ron "../encoding/ron"
import km "../math"
import "core:testing"
import "core:os"
import "core:strings"
import "core:encoding/json"

Capture_Test_Target :: struct { target:ecs.Entity_Id }
@(test)
test_prefab_capture_detaches_root_remaps_custom_refs_and_removal_history :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-prefab-capture-*",context.allocator); testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_root:=strings.concatenate({directory,"/resources"}); defer delete(resource_root); testing.expect_value(t,os.make_directory(resource_root),os.Error(nil))
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None); testing.expect_value(t,asset_resources_init(&owner,directory,resource_root),resources.Error.None)
    editor.editor_register(&owner.world,&owner.registry,"game.target",Capture_Test_Target{},spawn_default=false)
    external:=ecs.spawn(&owner.world,struct {transform:Scene_Transform}{{local=km.TRANSFORM_IDENTITY}})
    root:=ecs.spawn(&owner.world,struct {name:Scene_Name,transform:Scene_Transform,parent:Scene_Parent}{{strings.clone("Custom chair")},{km.transform(position={8,1,4},scale={2,1,1})},{external}})
    descriptor:string=`{"kind":"cube","size":[1,1,1]}`; mesh,mesh_error:=scene_mesh_prepare(&owner,{kind=.Geometry,geometry=transmute([]byte)descriptor}); testing.expect_value(t,mesh_error,Mesh_Error.None)
    child:=ecs.spawn(&owner.world,struct {transform:Scene_Transform,parent:Scene_Parent,mesh:Scene_Mesh,surface:Surface_Material}{{km.transform(position={0,2,0})},{root},mesh,{roughness=0.7,ao=1}})
    ecs.add_component(&owner.world,child,Capture_Test_Target{external})
    request:=asset.Prefab_Request{action=.Capture,path="resources/custom.katprefab",root_entity=root}
    rejected,no_group:=asset_authoring_execute(&owner,request); defer editor.tool_result_destroy(&rejected); defer editor.undo_group_destroy(&no_group)
    testing.expect_value(t,rejected.error,editor.Scene_Error.Invalid_Operation)
    _,has_key:=ecs.get_component(&owner.world,root,Scene_Key); testing.expect(t,!has_key && owner.world.live_count==3)
    roots:=ecs.get_resource_mut(&owner.world,Asset_Roots); missing,missing_error:=resources.read_text(&roots.project,request.path); defer delete(missing); testing.expect_value(t,missing_error,resources.Error.IO)
    ecs.add_component(&owner.world,child,Capture_Test_Target{child})
    captured,capture_group:=asset_authoring_execute(&owner,request); defer editor.tool_result_destroy(&captured); defer editor.undo_group_destroy(&capture_group)
    testing.expect(t,captured.error==.None && strings.contains(string(captured.data),`"published":true`) && owner.world.live_count==3)
    transform,_:=ecs.get_component(&owner.world,root,Scene_Transform); parent,_:=ecs.get_component(&owner.world,root,Scene_Parent)
    testing.expect(t,transform.local.position==km.Vec3{8,1,4} && parent.entity==external)
    bytes,read_error:=resources.read_text(&roots.project,request.path); defer delete(bytes); testing.expect_value(t,read_error,resources.Error.None)
    tree,parse_error:=ron.parse(string(bytes)); defer json.destroy_value(tree); testing.expect_value(t,parse_error.kind,ron.Error_Kind.None)
    prefab,prefab_error:=prefab_document_decode(&owner,tree,request.path); defer prefab_document_destroy(&prefab); testing.expect_value(t,prefab_error,editor.Scene_Error.None)
    for row in prefab.scene.entities { if row.key==prefab.root { value,decoded:=scene_row_owned_decode(&owner,row,"SceneTransform"); defer scene_row_owned_destroy(&owner,"SceneTransform",value); testing.expect(t,decoded && km.transform_is_identity((cast(^Scene_Transform)value).local) && !scene_row_has(row,"SceneParent")) } }
    instantiate:=asset.Prefab_Request{action=.Instantiate,path=request.path,position={3,0,0},rotation={0,0,0,1},scale={1,1,1}}
    inserted,insert_group:=asset_authoring_execute(&owner,instantiate); defer editor.tool_result_destroy(&inserted); defer editor.undo_group_destroy(&insert_group)
    testing.expect(t,inserted.error==.None && len(inserted.entities)==2 && owner.world.live_count==5)
    if len(inserted.entities)!=2 { return }; inserted_child:=inserted.entities[1]
    target,has_target:=ecs.get_component(&owner.world,inserted_child,Capture_Test_Target); testing.expect(t,has_target && target.target==inserted_child)
    removed,remove_group:=asset_authoring_execute(&owner,{action=.Remove,root_entity=root}); defer editor.tool_result_destroy(&removed); defer editor.undo_group_destroy(&remove_group)
    testing.expect(t,removed.error==.None && !ecs.entity_exists(&owner.world,root) && !ecs.entity_exists(&owner.world,child) && owner.world.live_count==3)
    for _ in 0..<3 {
        testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&remove_group),editor.Scene_Error.None)
        restored_root:=remove_group.entities[0]; restored_child:=remove_group.entities[1]; restored_parent,_:=ecs.get_component(&owner.world,restored_child,Scene_Parent); restored_target,_:=ecs.get_component(&owner.world,restored_child,Capture_Test_Target)
        testing.expect(t,restored_root!=root && restored_child!=child && restored_parent.entity==restored_root && restored_target.target==restored_child && owner.world.live_count==5)
        testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&remove_group),editor.Scene_Error.None); testing.expect(t,owner.world.live_count==3)
    }
    args:string=`{"action":"remove","root_entity":"0"}`; decoded,decode_error:=asset.prefab_decode(transmute([]byte)args); defer asset.prefab_destroy(&decoded); testing.expect(t,decode_error==.None && decoded.request.root_entity==0)
    hidden:=ecs.spawn(&owner.world,struct {hidden:Editor_Hidden}{}); protected,protected_group:=asset_authoring_execute(&owner,{action=.Remove,root_entity=hidden}); defer editor.tool_result_destroy(&protected); defer editor.undo_group_destroy(&protected_group); testing.expect_value(t,protected.error,editor.Scene_Error.Protected_Entity)
}
