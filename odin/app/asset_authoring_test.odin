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

@(test)
test_actual_mesh_authoring_write_read_compile_and_rejected_replacement :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-mesh-authoring-*",context.allocator)
    testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_path:=strings.concatenate({directory,"/resources"}); defer delete(resource_path)
    testing.expect_value(t,os.make_directory(resource_path),os.Error(nil))
    meshes:=strings.concatenate({resource_path,"/meshes"}); defer delete(meshes); testing.expect_value(t,os.make_directory(meshes),os.Error(nil))
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app)
    testing.expect_value(t,asset_resources_init(&app,directory,resource_path),resources.Error.None)
    args:string=`{"action":"write","path":"resources/meshes/seat.katmesh","document":{"version":1,"name":"Seat","parts":[{"id":"seat","transform":{"position":[0,0.45,0]},"geometry":{"kind":"cube","size":[0.8,0.1,0.8]}}]}}`
    decoded,decode_error:=asset.prefab_decode(transmute([]byte)args); defer asset.prefab_destroy(&decoded); testing.expect_value(t,decode_error,asset.Error.None)
    written,no_undo:=asset_authoring_execute(&app,decoded.request); defer editor.tool_result_destroy(&written); defer editor.undo_group_destroy(&no_undo)
    testing.expect(t,written.error==.None && strings.contains(string(written.data),`"published":true`))
    roots:=ecs.get_resource_mut(&app.world,Asset_Roots)
    bytes,read_error:=resources.read_text(&roots.project,"resources/meshes/seat.katmesh"); defer delete(bytes)
    testing.expect(t,read_error==.None && strings.contains(string(bytes),"position:(") && strings.contains(string(bytes),"size:("))
    mesh,mesh_error:=mesh_recipe_load(&roots.resource,"meshes/seat.katmesh"); defer mesh_geometry_destroy(&mesh)
    testing.expect(t,mesh_error==.None && len(mesh.vertices)==24 && len(mesh.indices)==36 && abs(mesh.bounds.center[1]-0.45)<0.00001)
    read,_:=asset_authoring_execute(&app,{action=.Read,path="resources/meshes/seat.katmesh"}); defer editor.tool_result_destroy(&read); testing.expect(t,read.error==.None && strings.contains(string(read.data),"Seat"))
    object:=decoded.request.document.(json.Object); parts:=object["parts"].(json.Array); geometry:=parts[0].(json.Object)["geometry"].(json.Object)
    size:=geometry["size"].(json.Array); size[0]=json.Integer(-1)
    rejected,_:=asset_authoring_execute(&app,decoded.request); defer editor.tool_result_destroy(&rejected); testing.expect_value(t,rejected.error,editor.Scene_Error.Invalid_Operation)
    unchanged,unchanged_error:=resources.read_text(&roots.project,"resources/meshes/seat.katmesh"); defer delete(unchanged); testing.expect(t,unchanged_error==.None && string(unchanged)==string(bytes))
    size[0]=json.Float(0.8); app.mode=.Playing
    guarded,_:=asset_authoring_execute(&app,decoded.request); defer editor.tool_result_destroy(&guarded); testing.expect_value(t,guarded.error,editor.Scene_Error.Editing_Required)
}
