#+test
package resources

import "core:testing"
import "core:os"
import "core:strings"

@(test)
test_native_capability_create_replace_list_stat_delete_and_retained_root :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-native-resource-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    root,root_error:=root_open(directory); testing.expect_value(t,root_error,Error.None); if root_error!=.None { return }; defer root_destroy(&root)
    contents:string="authored source"
    published,create_error:=create_atomic(&root,"nested/assets/Öak.katla",transmute([]byte)contents); testing.expect(t,published && create_error==.None)
    duplicate,duplicate_error:=create_atomic(&root,"nested/assets/Öak.katla",[]byte{99}); testing.expect(t,!duplicate && duplicate_error!=.None)
    actual_size,size_error:=file_size(&root,"nested/assets/Öak.katla"); testing.expect(t,size_error==.None && actual_size==i64(len(contents)))
    inventory,inventory_error:=list_directory(&root,"nested/assets"); defer directory_result_destroy(&inventory); testing.expect(t,inventory_error==.None && len(inventory.entries)==1 && inventory.entries[0].name=="Öak.katla" && inventory.entries[0].size==actual_size)
    matches,search_error:=search(&root,"öak",{"katla"}); defer search_result_destroy(&matches); testing.expect(t,search_error==.None && len(matches.assets)==1 && matches.assets[0]=="nested/assets/Öak.katla")
    renamed:=strings.concatenate({directory,"-moved"}); defer delete(renamed); defer os.remove_all(renamed); testing.expect_value(t,os.rename(directory,renamed),os.Error(nil))
    testing.expect_value(t,os.make_directory(directory),os.Error(nil))
    fake:=strings.concatenate({directory,"/decoy"}); defer delete(fake); testing.expect_value(t,os.write_entire_file(fake,"outside retained root"),os.Error(nil))
    after_move,move_error:=list_directory(&root); defer directory_result_destroy(&after_move); testing.expect(t,move_error==.None && len(after_move.entries)==1 && after_move.entries[0].directory && after_move.entries[0].name=="nested")
    replacement:string="replacement"
    replaced,replace_error:=write_atomic(&root,"nested/assets/Öak.katla",transmute([]byte)replacement); testing.expect(t,replaced && replace_error==.None)
    bytes,read_error:=read_text(&root,"nested/assets/Öak.katla"); defer delete(bytes); testing.expect(t,read_error==.None && string(bytes)==replacement)
    limited,limit_error:=read_bytes(&root,"nested/assets/Öak.katla",2); defer delete(limited); testing.expect_value(t,limit_error,Error.Limit)
    for invalid in ([7]string{"../outside","nested/../outside","C:/outside","nested\\outside","nested/file:stream","nested//outside","nested/./outside"}) { rejected,reject_error:=write_atomic(&root,invalid,[]byte{1}); testing.expect(t,!rejected && reject_error==.Invalid_Path) }
    testing.expect_value(t,create_directory(&root,"nested/new"),Error.None)
    testing.expect_value(t,remove_path(&root,"nested/assets",false),Error.Not_Regular)
    testing.expect_value(t,remove_path(&root,"nested",true),Error.None)
    empty,empty_error:=list_directory(&root); defer directory_result_destroy(&empty); testing.expect(t,empty_error==.None && len(empty.entries)==0)
    decoy,decoy_error:=os.read_entire_file(fake,context.allocator); defer delete(decoy); testing.expect(t,decoy_error==nil && string(decoy)=="outside retained root")
}
