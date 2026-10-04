#+build darwin, linux
//! Owner-private Unix streams never unlink a pre-existing endpoint or alter process umask.
package socket

import "core:sys/posix"
import "core:os"
import "core:strings"
import "core:mem"

Error :: enum { None, Invalid_Path, Permission, Exists, Unavailable, Closed, IO }
Channel :: struct { fd:posix.FD, open:bool }
Listener :: struct { channel:Channel, path:string, inode:u64, device:u64, allocator:mem.Allocator }

private_endpoint :: proc(path:string)->Error {
    if len(path)==0 || path[0]!='/' || strings.contains(path,"\x00") { return .Invalid_Path }
    addr:posix.sockaddr_un; if len(path)>=len(addr.sun_path) { return .Invalid_Path }
    cpath:=strings.clone_to_cstring(path); defer delete(cpath)
    st:posix.stat_t
    if posix.lstat(cpath,&st)!=nil || !posix.S_ISSOCK(st.st_mode) { return .Invalid_Path }
    if int(st.st_uid)!=os.get_euid() || (transmute(posix._mode_t)st.st_mode)&0o777!=0o600 { return .Permission }
    return .None
}
@(private="package")
address :: proc(path:string)->(posix.sockaddr_un,Error) {
    addr:posix.sockaddr_un
    if len(path)==0 || path[0]!='/' || strings.contains(path,"\x00") || len(path)>=len(addr.sun_path) { return {},.Invalid_Path }
    addr.sun_family=.UNIX
    for c,i in transmute([]byte)path { addr.sun_path[i]=c }
    when ODIN_OS==.Darwin { addr.sun_len=u8(size_of(addr)) }
    return addr,.None
}
@(private="package")
new_channel :: proc()->(Channel,Error) {
    fd:=posix.socket(.UNIX,.STREAM); if fd<0 { return {},.IO }
    c:=Channel{fd=fd,open=true}
    if posix.fcntl(fd,.SETFD,i32(posix.FD_CLOEXEC))<0 || posix.fcntl(fd,.SETFL,posix.O_NONBLOCK)<0 { close(&c); return {},.IO }
    return c,.None
}
connect :: proc(path:string)->(Channel,Error) {
    if err:=private_endpoint(path); err!=.None { return {},err }
    addr,err:=address(path); if err!=.None { return {},err }
    c,create_error:=new_channel(); if create_error!=.None { return {},create_error }
    if posix.connect(c.fd,cast(^posix.sockaddr)&addr,size_of(addr))!=nil { close(&c); return {},.IO }
    return c,.None
}
/// Binding requires an owner-only parent directory, preventing exposure before chmod(0600).
listen :: proc(path:string,allocator:=context.allocator)->(Listener,Error) {
    addr,err:=address(path); if err!=.None { return {},err }
    parent:=strings.clone_to_cstring(os.dir(path)); defer delete(parent)
    st:posix.stat_t
    if posix.lstat(parent,&st)!=nil || !posix.S_ISDIR(st.st_mode) || int(st.st_uid)!=os.get_euid() || (transmute(posix._mode_t)st.st_mode)&0o077!=0 { return {},.Permission }
    cpath:=strings.clone_to_cstring(path); defer delete(cpath)
    existing:posix.stat_t
    if posix.lstat(cpath,&existing)==nil { return {},.Exists }
    c,create_error:=new_channel(); if create_error!=.None { return {},create_error }
    if posix.bind(c.fd,cast(^posix.sockaddr)&addr,size_of(addr))!=nil { close(&c); return {},.IO }
    if posix.chmod(cpath,{.IRUSR,.IWUSR})!=nil || posix.lstat(cpath,&st)!=nil {
        close(&c); posix.unlink(cpath); return {},.Permission
    }
    listener:=Listener{channel=c,path=strings.clone(path,allocator),inode=u64(st.st_ino),device=u64(st.st_dev),allocator=allocator}
    if posix.listen(c.fd,4)!=nil { listener_destroy(&listener); return {},.IO }
    return listener,.None
}
accept :: proc(listener:^Listener)->(Channel,Error,bool) {
    fd:=posix.accept(listener.channel.fd,nil,nil)
    if fd<0 { if posix.errno()==.EAGAIN || posix.errno()==.EINTR { return {},.None,false }; return {},.IO,false }
    c:=Channel{fd=fd,open=true}
    if posix.fcntl(fd,.SETFD,i32(posix.FD_CLOEXEC))<0 || posix.fcntl(fd,.SETFL,posix.O_NONBLOCK)<0 { close(&c); return {},.IO,false }
    return c,.None,true
}
close :: proc(c:^Channel) { if c.open { posix.close(c.fd) }; c^={} }
/// Bounds kernel buffering for a stream direction; draining peers can accept an owner-tick burst.
buffer_prepare :: proc(c:^Channel,output:bool)->(before,after:i32,error:Error) {
    if !c.open { return 0,0,.Closed }
    option:=posix.Sock_Option.SNDBUF if output else posix.Sock_Option.RCVBUF
    size:=posix.socklen_t(size_of(i32))
    if posix.getsockopt(c.fd,posix.SOL_SOCKET,option,&before,&size)!=nil || size!=size_of(i32) { return 0,0,.IO }
    requested:=i32(256<<10)
    if posix.setsockopt(c.fd,posix.SOL_SOCKET,option,&requested,size_of(requested))!=nil { return before,0,.IO }
    size=posix.socklen_t(size_of(i32))
    if posix.getsockopt(c.fd,posix.SOL_SOCKET,option,&after,&size)!=nil || size!=size_of(i32) || after<64<<10 || after>1<<20 { return before,after,.IO }
    return before,after,.None
}
shutdown_write :: proc(c:^Channel)->Error { if !c.open { return .Closed }; if posix.shutdown(c.fd,.WR)!=nil { return .IO }; return .None }
/// Removes only the inode created by this listener; a replacement pathname is preserved.
listener_destroy :: proc(l:^Listener) {
    close(&l.channel)
    if l.path!="" {
        path:=strings.clone_to_cstring(l.path,l.allocator); defer delete(path,l.allocator)
        st:posix.stat_t
        if posix.lstat(path,&st)==nil && u64(st.st_ino)==l.inode && u64(st.st_dev)==l.device { posix.unlink(path) }
        delete(l.path,l.allocator)
    }
    l^={}
}
wait :: proc(c:^Channel,write:bool,timeout_ms:i32)->(bool,Error) {
    if !c.open { return false,.Closed }
    p:=posix.pollfd{fd=c.fd,events={.OUT} if write else {.IN}}
    n:=posix.poll(&p,1,timeout_ms)
    if n<0 { if posix.errno()==.EINTR { return false,.None }; return false,.IO }
    if n==0 { return false,.None }
    if .NVAL in p.revents || .ERR in p.revents { return false,.IO }
    if .HUP in p.revents && !(.IN in p.revents) { return false,.Closed }
    return (.OUT in p.revents) if write else (.IN in p.revents),.None
}
read :: proc(c:^Channel,buffer:[]byte)->(int,Error) {
    if !c.open { return 0,.Closed }
    n:=posix.recv(c.fd,raw_data(buffer),len(buffer),{})
    if n==0 { return 0,.Closed }
    if n<0 { if posix.errno()==.EAGAIN || posix.errno()==.EINTR { return 0,.None }; return 0,.IO }
    return int(n),.None
}
write :: proc(c:^Channel,buffer:[]byte)->(int,Error) {
    if !c.open { return 0,.Closed }
    n:=posix.send(c.fd,raw_data(buffer),len(buffer),{.NOSIGNAL})
    if n<0 { if posix.errno()==.EAGAIN || posix.errno()==.EINTR { return 0,.None }; return 0,.IO }
    return int(n),.None
}
