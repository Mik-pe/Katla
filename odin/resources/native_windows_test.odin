#+test
#+build windows
package resources

import "core:testing"
import "core:os"
import "core:strings"
import win "core:sys/windows"

@(private="file")
windows_test_junction :: proc(root:^Root,name,target:string)->Error {
    if error:=create_directory(root,name); error!=.None { return error }
    file,error,_:=windows_open_child(root.file,name,true,write=true); if error!=.None { return error }; defer os.close(file)
    absolute,path_error:=os.get_absolute_path(target,context.allocator); if path_error!=nil { return .IO }; defer delete(absolute)
    converted,allocated:=strings.replace_all(absolute,"/","\\",context.allocator); defer { if allocated { delete(converted) } }
    nt_path:=strings.concatenate({"\\??\\",converted}); defer delete(nt_path)
    encoded:=win.utf8_to_utf16(nt_path,context.allocator); defer delete(encoded)
    data_length:=8+(len(encoded)+2)*2; buffer_length:=size_of(win.REPARSE_DATA_BUFFER)+data_length
    buffer:=make([]u64,(buffer_length+7)/8); defer delete(buffer)
    header:=cast(^win.REPARSE_DATA_BUFFER)raw_data(buffer); header.ReparseTag=win.IO_REPARSE_TAG_MOUNT_POINT; header.ReparseDataLength=u16(data_length)
    mount:=cast(^win.MOUNT_POINT_REPARSE_BUFFER)(uintptr(header)+uintptr(size_of(win.REPARSE_DATA_BUFFER)))
    mount.SubstituteNameLength=u16(len(encoded)*2); mount.PrintNameOffset=u16((len(encoded)+1)*2)
    units:=cast([^]u16)&mount.PathBuffer; copy(units[:len(encoded)],encoded)
    count:u32
    if !bool(win.DeviceIoControl(windows_handle(file),win.FSCTL_SET_REPARSE_POINT,header,u32(buffer_length),nil,0,&count,nil)) { return .IO }; return .None
}

@(test)
test_windows_junction_is_omitted_rejected_and_deleted_without_following :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-windows-junction-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    inside:=strings.concatenate({directory,"/inside"}); defer delete(inside); outside:=strings.concatenate({directory,"/outside"}); defer delete(outside)
    testing.expect(t,os.make_directory(inside)==nil && os.make_directory(outside)==nil)
    secret:=strings.concatenate({outside,"/secret"}); defer delete(secret); testing.expect(t,os.write_entire_file(secret,"outside bytes")==nil)
    root,root_error:=root_open(inside); testing.expect_value(t,root_error,Error.None); if root_error!=.None { return }; defer root_destroy(&root)
    testing.expect_value(t,create_directory(&root,"tree"),Error.None)
    tree,tree_error:=open_relative_native(&root,"tree",true); testing.expect_value(t,tree_error,Error.None); if tree_error!=.None { return }; defer os.close(tree)
    tree_root:=Root{file=tree,allocator=context.allocator}; testing.expect_value(t,windows_test_junction(&tree_root,"escape",outside),Error.None)
    denied,read_error:=read_bytes(&root,"tree/escape/secret"); defer delete(denied); testing.expect(t,read_error!=.None && len(denied)==0)
    published,write_error:=write_atomic(&root,"tree/escape/secret",[]byte{1}); testing.expect(t,!published && write_error!=.None)
    directory_result,directory_error:=list_directory(&root,"tree"); defer directory_result_destroy(&directory_result); testing.expect(t,directory_error==.None && len(directory_result.entries)==0)
    found,search_error:=search(&root,""); defer search_result_destroy(&found); testing.expect(t,search_error==.None && found.total==0)
    testing.expect_value(t,remove_path(&root,"tree/escape",true),Error.Not_Regular)
    testing.expect_value(t,remove_path(&root,"tree",true),Error.None)
    bytes,secret_error:=os.read_entire_file(secret,context.allocator); defer delete(bytes); testing.expect(t,secret_error==nil && string(bytes)=="outside bytes")
}
