#+test
#+build darwin, linux
package app

import ecs "../ecs"
import km "../math"
import editor "../editor"
import resources "../resources"
import "core:testing"
import "core:os"
import "core:strings"

@(test)
test_mesh_source_codec_prepares_geometry_without_serializing_runtime_state :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app); scene_components_register(&app); scene_mesh_register(&app)
    project,err:=os.get_working_directory(context.allocator); testing.expect(t,err==nil); defer delete(project)
    resource_path:=strings.concatenate({project,"/resources"}); defer delete(resource_path)
    testing.expect_value(t,asset_resources_init(&app,project,resource_path),resources.Error.None)
    prepared,prepare_error:=scene_mesh_prepare(&app,{kind=.Recipe,path="meshes/chair-frame.katmesh"})
    testing.expect_value(t,prepare_error,Mesh_Error.None)
    entity:=ecs.spawn(&app.world,struct { mesh:Scene_Mesh,transform:Scene_Transform }{prepared,{local=km.TRANSFORM_IDENTITY}})
    entry:=app.registry.entries["SceneMesh"]
    bytes,encode_error:=editor.editor_component_json(&app.world,entity,entry); defer delete(bytes)
    testing.expect(t,encode_error==.None && strings.contains(string(bytes),"meshes/chair-frame.katmesh") && !strings.contains(string(bytes),"vertices") && !strings.contains(string(bytes),"allocator"))
    snapshot,capture_error:=scene_snapshot_capture(&app); defer scene_snapshot_destroy(&snapshot); testing.expect_value(t,capture_error,editor.Scene_Error.None)
    testing.expect_value(t,scene_snapshot_restore(&app,&snapshot),editor.Scene_Error.None)
    ids:=ecs.entity_ids(&app.world); defer delete(ids)
    restored,ok:=ecs.get_component(&app.world,ids[0],Scene_Mesh)
    testing.expect(t,ok && len(restored.geometry.vertices)==144 && len(restored.geometry.indices)==216 && !ecs.entity_exists(&app.world,entity))
}

@(test)
test_mesh_undo_retains_prepared_revision_after_file_changes :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-mesh-undo-*",context.allocator)
    testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_path:=strings.concatenate({directory,"/resources"}); defer delete(resource_path); testing.expect_value(t,os.make_directory(resource_path),os.Error(nil))
    path:=strings.concatenate({resource_path,"/mesh.katmesh"}); defer delete(path)
    recipe:string=`(version:1,name:"Cube",parts:[(id:"part",geometry:(kind:"cube",size:(1,1,1)))])`
    testing.expect_value(t,os.write_entire_file(path,recipe),os.Error(nil))
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app); scene_components_register(&app); scene_mesh_register(&app)
    testing.expect_value(t,asset_resources_init(&app,directory,resource_path),resources.Error.None)
    prepared,prepare_error:=scene_mesh_prepare(&app,{kind=.Recipe,path="mesh.katmesh"}); testing.expect_value(t,prepare_error,Mesh_Error.None)
    entity:=ecs.spawn(&app.world,struct { mesh:Scene_Mesh,transform:Scene_Transform }{prepared,{local=km.TRANSFORM_IDENTITY}})
    result,command:=editor.scene_execute(&app.world,&app.registry,{kind=.Destroy,entity=entity}); defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&command)
    testing.expect_value(t,os.write_entire_file(path,"malformed later revision"),os.Error(nil))
    for _ in 0..<3 {
        testing.expect_value(t,editor.undo_group(&app.world,&app.registry,&command),editor.Scene_Error.None)
        mesh,exists:=ecs.get_component(&app.world,command.entities[0],Scene_Mesh)
        testing.expect(t,exists && len(mesh.geometry.vertices)==24 && len(mesh.geometry.indices)==36)
        testing.expect_value(t,editor.redo_group(&app.world,&app.registry,&command),editor.Scene_Error.None)
    }
}
