#+test
#+build darwin, linux
package asset_browser
import app ".."
import resources "../../resources"
import "core:testing"
import "core:os"
import "core:strings"

@(test)
test_browser_real_inventory_folders_selection_refresh_and_root_confined_drag :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-asset-browser-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource:=strings.concatenate({directory,"/resources"}); defer delete(resource); testing.expect(t,os.make_directory(resource)==nil)
    for name in ([2]string{"z-folder","a-folder"}) { path:=strings.concatenate({resource,"/",name}); testing.expect(t,os.make_directory(path)==nil); delete(path) }
    for name in ([7]string{"a.gltf","b.stl","c.katprefab","d.wav","e.jpeg","f.toml",".hidden"}) { path:=strings.concatenate({resource,"/",name}); testing.expect(t,os.write_entire_file(path,"actual asset bytes")==nil); delete(path) }
    linked:=strings.concatenate({resource,"/outside"}); defer delete(linked); testing.expect(t,os.symlink(directory,linked)==nil)
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); testing.expect_value(t,app.asset_resources_init(&owner,directory,resource),resources.Error.None)
    state:State; init(&state,&owner); defer destroy(&state); testing.expect_value(t,refresh(&state),resources.Error.None)
    testing.expect_value(t,len(state.entries),8); testing.expect(t,state.entries[0].kind==.Folder && state.entries[0].name=="a-folder" && state.entries[1].name=="z-folder")
    testing.expect(t,select(&state,"b.stl")); state.entries[3].thumbnail=.Ready
    selected,valid:=drag(&state); testing.expect(t,valid && selected.path=="b.stl" && selected.root==.Resource && selected.kind==.Model)
    new_path:=strings.concatenate({resource,"/aaa.glb"}); defer delete(new_path); testing.expect(t,os.write_entire_file(new_path,"new actual asset")==nil)
    testing.expect_value(t,refresh(&state),resources.Error.None); selected,valid=drag(&state); testing.expect(t,valid && selected.path=="b.stl")
    for entry in state.entries { if entry.path=="b.stl" { testing.expect_value(t,entry.thumbnail,Thumbnail_State.Ready) } }
    testing.expect_value(t,navigate(&state,"outside"),resources.Error.IO); testing.expect_value(t,state.directory,"")
    testing.expect_value(t,navigate(&state,"a-folder"),resources.Error.None); testing.expect_value(t,len(state.entries),0); testing.expect(t,state.selected=="")
    testing.expect_value(t,parent(&state),resources.Error.None); testing.expect_value(t,parent(&state),resources.Error.None); testing.expect_value(t,state.directory,"")
    testing.expect_value(t,search_set(&state,".wav"),resources.Error.None); testing.expect(t,len(state.entries)==1 && state.entries[0].kind==.Audio)
    testing.expect_value(t,navigate(&state,"../escape"),resources.Error.Invalid_Path)
}

@(test)
test_browser_range_drag_batch_create_and_confirmed_recursive_delete_preserves_link_target :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-browser-actions-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource:=strings.concatenate({directory,"/resources"}); defer delete(resource); testing.expect(t,os.make_directory(resource)==nil)
    for name in ([3]string{"a.glb","b.stl","c.katprefab"}) { path:=strings.concatenate({resource,"/",name}); testing.expect(t,os.write_entire_file(path,"actual")==nil); delete(path) }
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); testing.expect_value(t,app.asset_resources_init(&owner,directory,resource),resources.Error.None)
    state:State; init(&state,&owner); defer destroy(&state); testing.expect_value(t,refresh(&state),resources.Error.None)
    testing.expect(t,select_mode(&state,"a.glb",.Replace) && select_mode(&state,"c.katprefab",.Range)); testing.expect_value(t,len(state.selected_paths),3)
    batch:=drag_batch(&state); defer drag_batch_destroy(&batch); testing.expect_value(t,len(batch.items),3)
    project,valid:=project_path(&state,batch.items[1].path); defer delete(project); testing.expect(t,valid && project=="resources/b.stl")
    testing.expect(t,select_mode(&state,"b.stl",.Toggle)); testing.expect_value(t,len(state.selected_paths),2)
    testing.expect(t,delete_request(&state)); testing.expect(t,select(&state,"b.stl")); delete_cancel(&state); testing.expect_value(t,len(state.entries),3)
    testing.expect_value(t,create_folder(&state,"nested"),resources.Error.None); testing.expect_value(t,create_folder(&state,"../escape"),resources.Error.Invalid_Path)
    nested_file:=strings.concatenate({resource,"/nested/file.txt"}); defer delete(nested_file); testing.expect(t,os.write_entire_file(nested_file,"nested actual")==nil)
    outside:=strings.concatenate({directory,"/outside.txt"}); defer delete(outside); testing.expect(t,os.write_entire_file(outside,"retained outside")==nil)
    link:=strings.concatenate({resource,"/nested/link"}); defer delete(link); testing.expect(t,os.symlink(outside,link)==nil)
    testing.expect(t,select(&state,"nested") && delete_request(&state)); testing.expect(t,select(&state,"a.glb"))
    deleted:=delete_confirm(&state); testing.expect(t,deleted.error==.None && deleted.removed==1)
    actual,read_error:=os.read_entire_file(outside,context.allocator); defer delete(actual); testing.expect(t,read_error==nil && string(actual)=="retained outside")
    testing.expect_value(t,len(state.entries),3); testing.expect(t,state.selected=="a.glb")
    for entry in state.entries { testing.expect(t,entry.path!="nested") }
}
