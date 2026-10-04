#+test
#+build darwin, linux
package app

import asset "../agent/assets"
import editor "../editor"
import ecs "../ecs"
import km "../math"
import resources "../resources"
import "core:testing"
import "core:os"
import "core:strings"

@(test)
test_mesh_instantiation_project_origin_history_revision_and_failed_staging :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-mesh-instantiate-*",context.allocator)
    testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_path:=strings.concatenate({directory,"/resources"}); defer delete(resource_path); testing.expect_value(t,os.make_directory(resource_path),os.Error(nil))
    path:=strings.concatenate({directory,"/chair.katmesh"}); defer delete(path)
    recipe:string=`(version:1,name:"Chair",parts:[(id:"seat",geometry:(kind:"cube",size:(2,1,1)))])`
    testing.expect_value(t,os.write_entire_file(path,recipe),os.Error(nil))
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    scene_components_register(&owner); scene_mesh_register(&owner)
    testing.expect_value(t,asset_resources_init(&owner,directory,resource_path),resources.Error.None)
    existing:=ecs.spawn(&owner.world,struct { name:Scene_Name,transform:Scene_Transform }{{strings.clone("Existing")},{local=km.TRANSFORM_IDENTITY}})
    args:string=`{"action":"instantiate","path":"chair.katmesh","name":"Office chair","position":[3,0,1],"rotation":[0,0,0,1],"scale":[1,2,1]}`
    decoded,decode_error:=asset.prefab_decode(transmute([]byte)args); defer asset.prefab_destroy(&decoded); testing.expect_value(t,decode_error,asset.Error.None)
    created,command:=asset_authoring_execute(&owner,decoded.request); defer editor.tool_result_destroy(&created); defer editor.undo_group_destroy(&command)
    testing.expect(t,created.error==.None && len(created.entities)==1 && strings.contains(string(created.data),"Office chair"))
    if len(created.entities)!=1 { return }
    entity:=created.entities[0]
    mesh,present:=ecs.get_component(&owner.world,entity,Scene_Mesh)
    testing.expect(t,present && mesh.source.root==.Project && mesh.source.path=="chair.katmesh" && len(mesh.geometry.vertices)==24)
    transform,has_transform:=ecs.get_component(&owner.world,entity,Scene_Transform)
    testing.expect(t,has_transform && transform.local.position==km.Vec3{3,0,1} && transform.local.scale==km.Vec3{1,2,1})
    snapshot,capture_error:=scene_snapshot_capture(&owner); defer scene_snapshot_destroy(&snapshot); testing.expect_value(t,capture_error,editor.Scene_Error.None)
    testing.expect_value(t,os.write_entire_file(path,"broken later revision"),os.Error(nil))
    for _ in 0..<3 {
        testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&command),editor.Scene_Error.None)
        testing.expect(t,ecs.entity_exists(&owner.world,existing) && !ecs.entity_exists(&owner.world,command.entities[0]))
        testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&command),editor.Scene_Error.None)
        current:=command.entities[0]; restored,ok:=ecs.get_component(&owner.world,current,Scene_Mesh)
        testing.expect(t,current!=entity && ok && restored.source.root==.Project && len(restored.geometry.indices)==36)
    }
    counter_before,_:=ecs.get_resource(&owner.world,Scene_Identity)
    ids_before:=ecs.entity_ids(&owner.world); defer delete(ids_before)
    failed,no_history:=asset_authoring_execute(&owner,decoded.request); defer editor.tool_result_destroy(&failed); defer editor.undo_group_destroy(&no_history)
    counter_after,_:=ecs.get_resource(&owner.world,Scene_Identity); ids_after:=ecs.entity_ids(&owner.world); defer delete(ids_after)
    testing.expect(t,failed.error==.Invalid_Operation && len(failed.entities)==0 && len(ids_before)==len(ids_after) && counter_before==counter_after && ecs.entity_exists(&owner.world,existing))
    testing.expect_value(t,scene_snapshot_restore(&owner,&snapshot),editor.Scene_Error.None)
    testing.expect(t,!ecs.entity_exists(&owner.world,existing))
    testing.expect_value(t,os.write_entire_file(path,recipe),os.Error(nil))
    testing.expect_value(t,scene_snapshot_restore(&owner,&snapshot),editor.Scene_Error.None)
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    loaded:=false
    for id in ids { value,exists:=ecs.get_component(&owner.world,id,Scene_Mesh); if exists { loaded=value.source.root==.Project && len(value.geometry.vertices)==24 } }
    testing.expect(t,loaded)
    for invalid in ([4]string{
        `{"action":"instantiate","path":"chair.katmesh","scale":[0,1,1]}`,
        `{"action":"instantiate","path":"chair.katmesh","rotation":[0,0,0,0]}`,
        `{"action":"instantiate","path":"chair.katmesh","position":[1e99,0,0]}`,
        `{"action":"instantiate","path":"chair.katmesh","document":{}}`,
    }) {
        value,err:=asset.prefab_decode(transmute([]byte)invalid); defer asset.prefab_destroy(&value); testing.expect_value(t,err,asset.Error.Invalid_Arguments)
    }
}
