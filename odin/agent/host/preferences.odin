//! Connection preferences contain only explicit endpoint/thread selection, never host authentication.
package host

import "core:os"
import "core:io"
import "core:encoding/json"
import "core:strings"

/// Clones initial environment selection without connecting or choosing a fallback thread.
config_environment :: proc(allocator:=context.allocator)->Config {
    return {socket=os.get_env("KATLA_CODEX_SOCKET",allocator),thread_id=os.get_env("KATLA_CODEX_THREAD",allocator)}
}
config_destroy :: proc(c:^Config,allocator:=context.allocator) { delete(c.socket,allocator); delete(c.thread_id,allocator); c^={} }
@(private="package")
config_syntax :: proc(c:Config)->bool {
    return len(c.socket)>0 && len(c.socket)<=4096 && c.socket[0]=='/' && !strings.contains(c.socket,"\x00") && len(c.thread_id)>0 && len(c.thread_id)<=4096 && strings.trim_space(c.thread_id)==c.thread_id && !strings.contains(c.thread_id,"\x00")
}
/// Reads only the explicitly named Katla preference file; missing remains an explicit disabled selection.
config_load :: proc(path:string,allocator:=context.allocator)->(Config,Error) {
    context.allocator=allocator
    file,err:=os.open(path); if err!=nil { return {},.Invalid_Config }; defer os.close(file)
    data:[16385]byte; used:=0
    for used<len(data) {
        n,read_error:=os.read(file,data[used:]); used+=n
        if read_error==io.Error.EOF || n==0 && read_error==nil { break }
        if read_error!=nil { return {},.Invalid_Config }
    }
    if used>16384 { return {},.Limit }
    tree,valid:=parse(string(data[:used]),allocator); if !valid { return {},.Invalid_Config }; defer json.destroy_value(tree)
    object,ok:=tree.(json.Object); if !ok || len(object)!=2 { return {},.Invalid_Config }
    socket_path,socket_ok:=object["socket"].(string); thread_id,thread_ok:=object["thread_id"].(string)
    candidate:=Config{socket_path,thread_id}; if !socket_ok || !thread_ok || !config_syntax(candidate) { return {},.Invalid_Config }
    return {strings.clone(socket_path,allocator),strings.clone(thread_id,allocator)},.None
}
/// Atomically saves an explicit selection in a 0600 file. Offline endpoints can be stored without connecting.
config_save :: proc(c:Config,path:string,allocator:=context.allocator)->Error {
    if !config_syntax(c) { return .Invalid_Config }
    data,err:=json.marshal(c,allocator=allocator); if err!=nil { return .Invalid_Config }; defer delete(data,allocator)
    file,file_error:=os.create_temp_file(os.dir(path),".katla-connection-*"); if file_error!=nil { return .Transport }
    temp_path:=strings.clone(os.name(file),allocator); defer delete(temp_path,allocator)
    open,moved:=true,false
    defer { if open { os.close(file) }; if !moved { os.remove(temp_path) } }
    if os.fchmod(file,{.Read_User,.Write_User})!=nil { return .Transport }
    written:=0
    for written<len(data) { n,write_error:=os.write(file,data[written:]); if write_error!=nil || n==0 { return .Transport }; written+=n }
    if os.sync(file)!=nil { return .Transport }; close_error:=os.close(file); open=false; if close_error!=nil { return .Transport }
    if os.rename(temp_path,path)!=nil { return .Transport }; moved=true; return .None
}
