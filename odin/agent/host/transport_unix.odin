#+build darwin, linux
//! Direct bounded Unix JSONL transport uses the selected existing app-server control endpoint.
package host

import socket "../socket"
import "core:strings"

Transport :: struct { channel:socket.Channel }

config_validate :: proc(c:Config)->Error {
    if len(c.thread_id)==0 || len(c.thread_id)>4096 || strings.trim_space(c.thread_id)!=c.thread_id || strings.contains(c.thread_id,"\x00") { return .Invalid_Config }
    if socket.private_endpoint(c.socket)!=.None { return .Invalid_Config }
    return .None
}
transport_open :: proc(t:^Transport,c:Config)->Error {
    if config_validate(c)!=.None { return .Invalid_Config }
    channel,err:=socket.connect(c.socket); if err!=.None { return .Transport }; t.channel=channel; return .None
}
transport_close :: proc(t:^Transport) { socket.close(&t.channel) }
transport_wait :: proc(t:^Transport,write:bool,timeout_ms:i32)->(bool,Error) {
    ready,err:=socket.wait(&t.channel,write,timeout_ms)
    if err==.Closed { return false,.Closed }; if err!=.None { return false,.Transport }; return ready,.None
}
transport_read :: proc(t:^Transport,buffer:[]byte)->(int,Error) {
    n,err:=socket.read(&t.channel,buffer)
    if err==.Closed { return n,.Closed }; if err!=.None { return n,.Transport }; return n,.None
}
transport_write :: proc(t:^Transport,buffer:[]byte)->(int,Error) {
    n,err:=socket.write(&t.channel,buffer)
    if err==.Closed { return n,.Closed }; if err!=.None { return n,.Transport }; return n,.None
}
transport_worker_signals :: proc() {}
