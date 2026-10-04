#+test
#+build darwin, linux
package socket

import "core:testing"
import "core:os"
import "core:strings"
import "core:sys/posix"

@(test)
test_private_endpoint_exclusive_bind_permissions_and_inode_cleanup :: proc(t:^testing.T) {
    directory,err:=os.make_directory_temp("","katla-socket-*",context.allocator); testing.expect(t,err==nil); if err!=nil { return }
    defer { os.remove_all(directory); delete(directory) }
    testing.expect(t,os.chmod(directory,{.Read_User,.Write_User,.Execute_User})==nil)
    path:=strings.concatenate({directory,"/editor.sock"}); defer delete(path)
    l,listen_error:=listen(path); testing.expect_value(t,listen_error,Error.None)
    testing.expect_value(t,private_endpoint(path),Error.None)
    _,again:=listen(path); testing.expect_value(t,again,Error.Exists)
    cpath:=strings.clone_to_cstring(path); defer delete(cpath)
    testing.expect(t,posix.chmod(cpath,{.IRUSR,.IWUSR,.IRGRP})==nil)
    _,insecure:=connect(path); testing.expect_value(t,insecure,Error.Permission)
    testing.expect(t,posix.chmod(cpath,{.IRUSR,.IWUSR})==nil)
    backup:=strings.concatenate({path,".owned"}); defer delete(backup)
    testing.expect(t,os.rename(path,backup)==nil && os.write_entire_file(path,"replacement")==nil)
    listener_destroy(&l)
    bytes,read_error:=os.read_entire_file(path,context.allocator); defer delete(bytes)
    testing.expect(t,read_error==nil && string(bytes)=="replacement")
}
@(test)
test_private_socket_halfclose_drains_reply_and_detects_real_peer_eof :: proc(t:^testing.T) {
    directory,err:=os.make_directory_temp("","katla-socket-*",context.allocator); testing.expect(t,err==nil); if err!=nil { return }
    defer { os.remove_all(directory); delete(directory) }
    testing.expect(t,os.chmod(directory,{.Read_User,.Write_User,.Execute_User})==nil)
    path:=strings.concatenate({directory,"/editor.sock"}); defer delete(path)
    listener,listen_error:=listen(path); testing.expect_value(t,listen_error,Error.None); defer listener_destroy(&listener)
    client,connect_error:=connect(path); testing.expect_value(t,connect_error,Error.None); defer close(&client)
    peer,accept_error,accepted:=accept(&listener); testing.expect(t,accept_error==.None && accepted); defer close(&peer)
    written,write_error:=write(&client,([]byte{'a','b','c'})); testing.expect(t,write_error==.None && written==3)
    testing.expect_value(t,shutdown_write(&client),Error.None)
    buffer:[16]byte; ready,wait_error:=wait(&peer,false,100)
    testing.expect(t,ready && wait_error==.None)
    n,read_error:=read(&peer,buffer[:]); testing.expect(t,n==3 && read_error==.None && string(buffer[:n])=="abc")
    n,read_error=read(&peer,buffer[:]); testing.expect(t,n==0 && read_error==.Closed)
    written,write_error=write(&peer,([]byte{'o','k'})); testing.expect(t,write_error==.None && written==2)
    close(&peer)
    n,read_error=read(&client,buffer[:]); testing.expect(t,n==2 && read_error==.None && string(buffer[:n])=="ok")
    n,read_error=read(&client,buffer[:]); testing.expect(t,n==0 && read_error==.Closed)
}
