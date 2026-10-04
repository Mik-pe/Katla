#+build darwin, linux
//! Native path traversal opens each component without following links.
package resources

import "core:os"
import "core:strings"
import "core:crypto"
import "core:sys/posix"

@(private="package")
root_open_native :: proc(path:string)->(^os.File,Error) {
    when ODIN_OS==.Darwin || ODIN_OS==.Linux {
        name:=strings.clone_to_cstring(path); defer delete(name)
        fd:=posix.open(name,{.DIRECTORY,.NOFOLLOW,.CLOEXEC})
        if fd<0 { return nil,.IO }
        file:=os.new_file(uintptr(fd),path)
        if file==nil { posix.close(fd); return nil,.IO }
        return file,.None
    } else { return nil,.Unsupported_Platform }
}

@(private="package")
open_child_native :: proc(parent:^os.File,name:string,directory:bool)->(^os.File,Error) {
    when ODIN_OS==.Darwin || ODIN_OS==.Linux {
        cname:=strings.clone_to_cstring(name); defer delete(cname)
        flags:=posix.O_Flags{.NOFOLLOW,.CLOEXEC,.NONBLOCK}
        if directory { flags+= {.DIRECTORY} }
        fd:=posix.openat(posix.FD(os.fd(parent)),cname,flags)
        if fd<0 { return nil,.IO }
        file:=os.new_file(uintptr(fd),name)
        if file==nil { posix.close(fd); return nil,.IO }
        return file,.None
    } else { return nil,.Unsupported_Platform }
}

@(private="package")
open_relative_native :: proc(root:^Root,path:string,directory:=false)->(^os.File,Error) {
    if root.file==nil || !valid_relative_path(path) { return nil,.Invalid_Path }
    remaining:=path; current:=root.file
    defer { if current!=root.file { os.close(current) } }
    for part in strings.split_iterator(&remaining,"/") {
        is_directory:=len(remaining)>0 || directory
        next,err:=open_child_native(current,part,is_directory)
        if err!=.None { return nil,err }
        if len(remaining)==0 { return next,.None }
        if current!=root.file { os.close(current) }
        current=next
    }
    return nil,.Invalid_Path
}

@(private="package")
write_atomic_native :: proc(root:^Root,path:string,data:[]byte)->(bool,Error) {
    if root.file==nil { return false,.Invalid_Path }
    slash:=strings.last_index(path,"/"); basename:=path; parent:=root.file
    if slash>=0 {
        dir,err:=open_relative_native(root,path[:slash],true)
        if err!=.None { return false,err }; parent=dir; basename=path[slash+1:]
    }
    defer { if parent!=root.file { os.close(parent) } }
    target:=strings.clone_to_cstring(basename); defer delete(target)
    status:posix.stat_t
    if posix.fstatat(posix.FD(os.fd(parent)),target,&status,{.SYMLINK_NOFOLLOW})==.OK {
        if status.st_mode & posix.S_IFMT != posix.S_IFREG { return false,.Not_Regular }
    } else if posix.errno()!=.ENOENT { return false,.IO }
    random:[16]byte; crypto.rand_bytes(random[:]); temporary:[44]byte
    prefix:=string(".katla-tmp-"); copy(temporary[:],prefix)
    digits:="0123456789abcdef"
    for number,i in random { temporary[len(prefix)+i*2]=digits[number>>4]; temporary[len(prefix)+i*2+1]=digits[number&15] }
    temporary[len(temporary)-1]=0
    name:=cstring(raw_data(temporary[:]))
    fd:=posix.openat(posix.FD(os.fd(parent)),name,{.WRONLY,.CREAT,.EXCL,.NOFOLLOW,.CLOEXEC},posix.mode_t{.IRUSR,.IWUSR,.IRGRP,.IROTH})
    if fd<0 { return false,.IO }
    defer posix.close(fd)
    renamed:=false; defer { if !renamed { posix.unlinkat(posix.FD(os.fd(parent)),name,{}) } }
    offset:=0
    for offset<len(data) {
        count:=posix.write(fd,raw_data(data[offset:]),uint(len(data))-uint(offset))
        if count<0 { if posix.errno()==.EINTR { continue }; return false,.IO }
        if count==0 { return false,.IO }; offset+=int(count)
    }
    if posix.fsync(fd)!=.OK { return false,.IO }
    if posix.renameat(posix.FD(os.fd(parent)),name,posix.FD(os.fd(parent)),target)!=.OK { return false,.IO }
    renamed=true
    if posix.fsync(posix.FD(os.fd(parent)))!=.OK { return true,.IO }
    return true,.None
}
