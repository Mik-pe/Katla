#+build darwin, linux
//! Detached exec reports native admission errors and reaps the intermediate child.
package asset_browser
import "core:sys/posix"
import "core:mem"
import "core:strings"

@(private="package")
reveal_exec_failure :: proc "contextless" (descriptor:posix.FD)->! {
    error:=transmute([4]byte)i32(posix.errno()); sent:=0
    for sent<len(error) { count:=posix.write(descriptor,raw_data(error[sent:]),uint(len(error)-sent)); if count<0 { if posix.errno()==.EINTR { continue }; break }; if count==0 { break }; sent+=int(count) }
    posix._exit(126)
}
@(private="package")
reveal_launch :: proc(arguments:[]string,allocator:mem.Allocator)->int {
    if len(arguments)==0 || arguments[0]=="" { return int(posix.Errno.EINVAL) }
    argv:=make([]cstring,len(arguments)+1,allocator); defer { for argument in argv { delete(argument,allocator) }; delete(argv,allocator) }
    for argument,i in arguments { argv[i]=strings.clone_to_cstring(argument,allocator) }
    channel:[2]posix.FD
    if posix.pipe(&channel)!=.OK { return int(posix.errno()) }
    defer posix.close(channel[0])
    if posix.fcntl(channel[1],.SETFD,i32(posix.FD_CLOEXEC))<0 { error:=posix.errno(); posix.close(channel[1]); return int(error) }
    child:=posix.fork()
    if child<0 { error:=posix.errno(); posix.close(channel[1]); return int(error) }
    if child==0 {
        posix.close(channel[0])
        detached:=posix.fork(); if detached<0 { reveal_exec_failure(channel[1]) }
        if detached>0 { posix._exit(0) }
        posix.execve(argv[0],raw_data(argv),posix.environ); reveal_exec_failure(channel[1])
    }
    posix.close(channel[1])
    status:i32; waited:=posix.pid_t(-1)
    for { waited=posix.waitpid(child,&status,{}); if waited>=0 || posix.errno()!=.EINTR { break } }
    if waited!=child { return int(posix.errno()) }
    bytes:[4]byte; received:=0
    for received<len(bytes) {
        count:=posix.read(channel[0],raw_data(bytes[received:]),uint(len(bytes)-received))
        if count==0 { break }; if count<0 { if posix.errno()==.EINTR { continue }; return int(posix.errno()) }; received+=int(count)
    }
    if received!=0 && received!=len(bytes) { return int(posix.Errno.EIO) }
    if received==len(bytes) { return int(transmute(i32)bytes) }
    if !posix.WIFEXITED(status) || posix.WEXITSTATUS(status)!=0 { return int(posix.Errno.EIO) }
    return 0
}
