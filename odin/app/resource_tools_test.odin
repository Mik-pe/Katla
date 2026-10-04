#+test
#+build darwin, linux
package app

import asset "../agent/assets"
import editor "../editor"
import resources "../resources"
import "core:testing"
import "core:os"
import "core:strings"
import "core:encoding/json"

@(test)
test_resource_tools_actual_project_files_and_counts :: proc(t:^testing.T) {
    directory,err:=os.make_directory_temp("","katla-asset-authoring-*",context.allocator)
    testing.expect(t,err==nil); if err!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_path:=strings.concatenate({directory,"/resources"}); defer delete(resource_path)
    testing.expect_value(t,os.make_directory(resource_path),os.Error(nil))
    model_path:=strings.concatenate({resource_path,"/Chair.glb"}); defer delete(model_path)
    testing.expect_value(t,os.write_entire_file(model_path,"actual document"),os.Error(nil))
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app)
    testing.expect_value(t,asset_resources_init(&app,directory,resource_path),resources.Error.None)
    searched,no_undo:=asset_execute(&app,{action=.Search,query="chair",limit=64}); defer editor.tool_result_destroy(&searched); defer editor.undo_group_destroy(&no_undo)
    testing.expect_value(t,searched.error,editor.Scene_Error.None)
    result,json_error:=json.parse(searched.data,spec=.JSON,parse_integers=true); testing.expect(t,json_error==nil); defer json.destroy_value(result)
    object:=result.(json.Object); assets:=object["assets"].(json.Array); paths:=object["project_paths"].(json.Array)
    testing.expect(t,assets[0].(string)=="Chair.glb" && paths[0].(string)=="resources/Chair.glb")
    read,_:=asset_execute(&app,{action=.Read,path=paths[0].(string)}); defer editor.tool_result_destroy(&read)
    testing.expect(t,read.error==.None && strings.contains(string(read.data),"actual document"))
    listed,_:=asset_execute(&app,{action=.List,path="resources",filter="glb",limit=1}); defer editor.tool_result_destroy(&listed)
    testing.expect(t,listed.error==.None && strings.contains(string(listed.data),"resources/Chair.glb"))
    rejected,_:=asset_execute(&app,{action=.Read,path="../outside"}); defer editor.tool_result_destroy(&rejected)
    testing.expect_value(t,rejected.error,editor.Scene_Error.Invalid_Operation)
    args:string=`{"path":"resources/Chair.glb"}`
    decoded,decode_error:=asset.decode("read_resource",transmute([]byte)args); defer asset.destroy(&decoded)
    testing.expect(t,decode_error==.None && decoded.request.path=="resources/Chair.glb")
}
