#+test
#+build darwin, linux
package resources

import "core:testing"
import "core:os"
import "core:strings"

@(test)
test_search_read_bounds_and_symlink_confinement :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-resource-test-*",context.allocator)
    testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    models:=strings.concatenate({directory,"/models"}); defer delete(models)
    testing.expect_value(t,os.make_directory(models),os.Error(nil))
    for name in ([3]string{"Oak Chair.glb","Oak Table.glb","Oak Chair.png"}) {
        path:=strings.concatenate({models,"/",name}); defer delete(path)
        testing.expect_value(t,os.write_entire_file(path,"test text"),os.Error(nil))
    }
    loop:=strings.concatenate({directory,"/loop"}); defer delete(loop)
    testing.expect_value(t,os.symlink(directory,loop),os.Error(nil))
    root,err:=root_open(directory); testing.expect_value(t,err,Error.None); defer root_destroy(&root)
    result,search_error:=search(&root,"MODELS oak",{"GLB"},1); defer search_result_destroy(&result)
    testing.expect(t,search_error==.None && result.total==2 && result.truncated && len(result.assets)==1 && result.assets[0]=="models/Oak Chair.glb")
    repeated,repeated_error:=search(&root,"oak",{"glb"},64); defer search_result_destroy(&repeated)
    testing.expect(t,repeated_error==.None && repeated.total==2)
    bytes,read_error:=read_text(&root,result.assets[0]); defer delete(bytes)
    testing.expect(t,read_error==.None && string(bytes)=="test text")
    limited,limit_error:=read_text(&root,result.assets[0],3); defer delete(limited); testing.expect_value(t,limit_error,Error.Limit)
    linked,link_error:=read_text(&root,"loop/models/Oak Chair.glb"); defer delete(linked); testing.expect_value(t,link_error,Error.IO)
    for path in ([8]string{"/etc/passwd","../x","models/../x","models//x","models/./x","models/x/","C:/x","models\\x"}) {
        testing.expect(t,!valid_relative_path(path))
    }
    missing,missing_error:=read_text(&root,"models/missing.glb"); defer delete(missing); testing.expect_value(t,missing_error,Error.IO)
}

@(test)
test_root_retains_directory_and_rejects_invalid_utf8 :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-resource-root-*",context.allocator)
    testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    path:=strings.concatenate({directory,"/data"}); defer delete(path)
    testing.expect_value(t,os.write_entire_file(path,[]byte{255}),os.Error(nil))
    root,err:=root_open(directory); testing.expect_value(t,err,Error.None); defer root_destroy(&root)
    renamed:=strings.concatenate({directory,"-moved"}); defer delete(renamed); defer os.remove_all(renamed)
    testing.expect_value(t,os.rename(directory,renamed),os.Error(nil))
    bytes,read_error:=read_text(&root,"data"); defer delete(bytes); testing.expect_value(t,read_error,Error.Invalid_UTF8)
}

@(test)
test_atomic_publication_confines_parent_and_preserves_rejected_target :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-resource-write-*",context.allocator)
    testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    root,open_error:=root_open(directory); testing.expect_value(t,open_error,Error.None); defer root_destroy(&root)
    old_text:string="previous document"; new_text:string="new validated document"
    published,err:=write_atomic(&root,"scene.katmesh",transmute([]byte)old_text); testing.expect(t,published && err==.None)
    published,err=write_atomic(&root,"scene.katmesh",transmute([]byte)new_text); testing.expect(t,published && err==.None)
    bytes,read_error:=read_text(&root,"scene.katmesh"); defer delete(bytes); testing.expect(t,read_error==.None && string(bytes)==new_text)
    published,err=write_atomic(&root,"missing/scene.katmesh",transmute([]byte)new_text); testing.expect(t,!published && err==.IO)
    target:=strings.concatenate({directory,"/scene.katmesh"}); defer delete(target)
    link:=strings.concatenate({directory,"/link.katmesh"}); defer delete(link)
    testing.expect_value(t,os.symlink(target,link),os.Error(nil))
    published,err=write_atomic(&root,"link.katmesh",transmute([]byte)old_text); testing.expect(t,!published && err==.Not_Regular)
    unchanged,unchanged_error:=read_text(&root,"scene.katmesh"); defer delete(unchanged); testing.expect(t,unchanged_error==.None && string(unchanged)==new_text)
    matches,search_error:=search(&root,""); defer search_result_destroy(&matches)
    testing.expect(t,search_error==.None && matches.total==1 && matches.assets[0]=="scene.katmesh")
}
