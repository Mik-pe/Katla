#+build darwin, linux
//! Directory and deletion operations resolve parents through retained no-follow handles.
package resources
import "core:os"
import "core:strings"
import "core:sys/posix"

@(private="package")
create_directory_native :: proc(root:^Root,path:string)->Error {
    slash:=strings.last_index_byte(path,'/'); parent:=root.file; basename:=path
    if slash>=0 { handle,error:=open_relative_native(root,path[:slash],true); if error!=.None { return error }; parent=handle; basename=path[slash+1:] }
    defer { if parent!=root.file { os.close(parent) } }
    name:=strings.clone_to_cstring(basename); defer delete(name)
    if posix.mkdirat(posix.FD(os.fd(parent)),name,posix.mode_t{.IRUSR,.IWUSR,.IXUSR,.IRGRP,.IXGRP,.IROTH,.IXOTH})!=.OK { return .IO }
    if posix.fsync(posix.FD(os.fd(parent)))!=.OK { return .IO }; return .None
}
@(private="package")
remove_path_native :: proc(root:^Root,path:string,recursive:bool)->Error {
    slash:=strings.last_index_byte(path,'/'); parent:=root.file; basename:=path
    if slash>=0 { handle,error:=open_relative_native(root,path[:slash],true); if error!=.None { return error }; parent=handle; basename=path[slash+1:] }
    defer { if parent!=root.file { os.close(parent) } }
    budget:=MAX_ENTRIES
    return remove_child_native(parent,basename,recursive,false,&budget,0)
}
@(private="package")
remove_child_native :: proc(parent:^os.File,basename:string,recursive,allow_link:bool,budget:^int,depth:int)->Error {
    if depth>256 || budget^==0 { return .Limit }; budget^-=1
    name:=strings.clone_to_cstring(basename); defer delete(name)
    status:posix.stat_t
    if posix.fstatat(posix.FD(os.fd(parent)),name,&status,{.SYMLINK_NOFOLLOW})!=.OK { return .IO }
    kind:=status.st_mode & posix.S_IFMT
    if kind==posix.S_IFDIR {
        if !recursive { return .Not_Regular }
        child,error:=open_child_native(parent,basename,true); if error!=.None { return error }; defer os.close(child)
        iterator:=os.read_directory_iterator_create(child); defer os.read_directory_iterator_destroy(&iterator)
        for info,_ in os.read_directory_iterator(&iterator) {
            if info.name=="." || info.name==".." { continue }
            if child_error:=remove_child_native(child,info.name,true,true,budget,depth+1); child_error!=.None { return child_error }
        }
        if _,error:=os.read_directory_iterator_error(&iterator); error!=nil { return .IO }
        if posix.unlinkat(posix.FD(os.fd(parent)),name,{.REMOVEDIR})!=.OK { return .IO }
    } else {
        if kind!=posix.S_IFREG && !(allow_link && kind==posix.S_IFLNK) { return .Not_Regular }
        if posix.unlinkat(posix.FD(os.fd(parent)),name,{})!=.OK { return .IO }
    }
    if posix.fsync(posix.FD(os.fd(parent)))!=.OK { return .IO }; return .None
}
