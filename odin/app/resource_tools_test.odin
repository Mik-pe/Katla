#+test
#+build darwin, linux
package app

import asset "../agent/assets"
import editor "../editor"
import resources "../resources"
import "core:testing"
import "core:os"
import "core:strings"
import "core:fmt"
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

@(test)
test_primary_resource_defaults_metadata_unicode_search_and_full_listing :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-resource-contract-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource:=strings.concatenate({directory,"/resources"}); defer delete(resource); testing.expect(t,os.make_directory(resource)==nil)
    for name in ([5]string{"ΟΣ Chair.GLB","İ Chair.glb",".hidden","trailing.","plain"}) { path:=strings.concatenate({resource,"/",name}); testing.expect(t,os.write_entire_file(path,"pixels")==nil); delete(path) }
    for i in 0..<70 { number:=fmt.aprintf("%d",i); defer delete(number); path:=strings.concatenate({resource,"/item",number,".bin"}); testing.expect(t,os.write_entire_file(path,"data")==nil); delete(path) }
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect(t,asset_resources_init(&owner,directory,resource)==.None)
    args:string=`{"path":null,"filter":null}`; decoded,decode_error:=asset.decode("list_resources",transmute([]byte)args); defer asset.destroy(&decoded); testing.expect_value(t,decode_error,asset.Error.None)
    listed,list_group:=asset_execute(&owner,decoded.request); defer editor.tool_result_destroy(&listed); defer editor.undo_group_destroy(&list_group); testing.expect_value(t,listed.error,editor.Scene_Error.None)
    tree,parse_error:=json.parse(listed.data,spec=.JSON,parse_integers=true); defer json.destroy_value(tree); testing.expect(t,parse_error==nil); fields:=tree.(json.Object); entries:=fields["entries"].(json.Array); testing.expect(t,fields["path"].(string)=="." && fields["count"].(json.Integer)==75 && len(entries)==75)
    for entry in entries { object:=entry.(json.Object); name:=object["name"].(string); size:=object["size"].(json.Integer); testing.expect(t,name!="" && strings.has_prefix(object["path"].(string),"resources/") && (size==4 || size==6)) }
    for filter in ([3]string{"GLB","glb",""}) {
        result,result_group:=asset_execute(&owner,{action=.List,path="resources",filter=filter,has_filter=true}); defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&result_group)
        value,value_error:=json.parse(result.data,spec=.JSON,parse_integers=true); defer json.destroy_value(value); testing.expect(t,result.error==.None && value_error==nil && value.(json.Object)["count"].(json.Integer)==json.Integer(1))
    }
    expected_names:=[2]string{"ΟΣ Chair.GLB","İ Chair.glb"}
    for query,index in ([2]string{"ος chair","i\u0307 chair"}) {
        expected:=expected_names[index]
        result,result_group:=asset_execute(&owner,{action=.Search,query=query,extensions={".gLb"},limit=1}); defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&result_group)
        value,value_error:=json.parse(result.data,spec=.JSON,parse_integers=true); defer json.destroy_value(value); testing.expect(t,result.error==.None && value_error==nil)
        object:=value.(json.Object); matches:=object["assets"].(json.Array); testing.expect(t,len(matches)==1 && matches[0].(string)==expected && object["total"].(json.Integer)==1 && !object["truncated"].(bool))
    }
    limited,limited_group:=asset_execute(&owner,{action=.Search,query="item",limit=1}); defer editor.tool_result_destroy(&limited); defer editor.undo_group_destroy(&limited_group)
    limited_value,limited_error:=json.parse(limited.data,spec=.JSON,parse_integers=true); defer json.destroy_value(limited_value); testing.expect(t,limited.error==.None && limited_error==nil && limited_value.(json.Object)["total"].(json.Integer)==70 && limited_value.(json.Object)["truncated"].(bool))
}
