//! Main-owner nonblocking MCP transport shares only the canonical mailbox with socket clients.
package mcp

import socket "../socket"
import editor "../../editor"
import "core:mem"
import "core:time"

MAX_SOCKET_CLIENTS :: 4
MAX_SOCKET_OUTPUT :: 32<<20
Socket_Error :: enum { None, Unavailable, Endpoint, IO, Backpressure }
@(private="package")
Socket_Client :: struct {
    channel:socket.Channel,
    protocol:Server,
    active,input_closed,oversized:bool,
    input:[dynamic]byte,
    output:[dynamic]string,
    output_bytes,offset:int,
    last_output_progress:time.Duration,
}
/// Stationary listener owns client protocol queues, never a World or graphics pointer.
Socket_Server :: struct { listener:socket.Listener, clients:[MAX_SOCKET_CLIENTS]Socket_Client, mailbox:^editor.Agent_Harness, allocator:mem.Allocator, active:bool }
/// The endpoint parent must be owner-only; existing endpoints are rejected without unlinking.
socket_server_init :: proc(s:^Socket_Server,path:string,mailbox:^editor.Agent_Harness,allocator:=context.allocator)->Socket_Error {
    if s.active || mailbox==nil { return .Endpoint }
    listener,err:=socket.listen(path,allocator)
    if err==.Unavailable { return .Unavailable }; if err!=.None { return .Endpoint }
    s^={listener=listener,mailbox=mailbox,allocator=allocator,active=true}; return .None
}
@(private="package")
client_destroy :: proc(s:^Socket_Server,c:^Socket_Client) {
    socket.close(&c.channel); server_destroy(&c.protocol)
    for output in c.output { delete(output,s.allocator) }
    delete(c.input); delete(c.output); c^={}
}
/// Releases only this transport's tickets and the endpoint inode it created.
socket_server_destroy :: proc(s:^Socket_Server) {
    for &client in s.clients { if client.active { client_destroy(s,&client) } }
    socket.listener_destroy(&s.listener); s^={}
}
@(private="package")
queue_output :: proc(s:^Socket_Server,c:^Socket_Client,line:string,now:time.Duration)->bool {
    if line=="" { return true }
    if len(c.output)>=4 || c.output_bytes+len(line)+1>MAX_SOCKET_OUTPUT { delete(line,s.allocator); return false }
    if len(c.output)==0 { c.last_output_progress=now }
    append(&c.output,line); c.output_bytes+=len(line)+1; return true
}
@(private="package")
client_input :: proc(s:^Socket_Server,c:^Socket_Client,now:time.Duration)->Socket_Error {
    if c.input_closed { return .None }
    buffer:[8192]byte
    for _ in 0..<8 {
        ready,wait_error:=socket.wait(&c.channel,false,0)
        if wait_error==.Closed { c.input_closed=true; server_finish(&c.protocol); return .None }
        if wait_error!=.None { return .IO }; if !ready { return .None }
        n,err:=socket.read(&c.channel,buffer[:])
        if err==.Closed { c.input_closed=true; server_finish(&c.protocol); return .None }
        if err!=.None { return .IO }; if n==0 { return .None }
        for byte in buffer[:n] {
            if byte=='\n' {
                response:=server_receive(&c.protocol,"" if c.oversized else string(c.input[:]),now)
                clear(&c.input); c.oversized=false
                if !queue_output(s,c,response,now) { return .Backpressure }
                continue
            }
            if len(c.input)==MAX_MESSAGE_BYTES { c.oversized=true }
            if !c.oversized { append(&c.input,byte) }
        }
    }
    return .None
}
@(private="package")
client_output :: proc(s:^Socket_Server,c:^Socket_Client,now:time.Duration)->Socket_Error {
    for _ in 0..<8 {
        if len(c.output)==0 { return .None }
        ready,wait_error:=socket.wait(&c.channel,true,0)
        if wait_error!=.None { return .IO }; if !ready { return .None }
        line:=c.output[0]
        bytes:=transmute([]byte)line
        part:=bytes[c.offset:] if c.offset<len(bytes) else ([]byte{'\n'})
        if len(part)>8192 { part=part[:8192] }
        n,err:=socket.write(&c.channel,part); if err!=.None { return .IO }; if n==0 { return .None }
        c.offset+=n; c.output_bytes-=n; c.last_output_progress=now
        if c.offset==len(line)+1 { delete(line,s.allocator); ordered_remove(&c.output,0); c.offset=0 }
    }
    return .None
}
/// Poll before/after owner tick; bounded per-client I/O never waits for GPU completion on this thread.
/// EOF drains accepted replies; output faults or stalled output abandon only this client's credits.
socket_server_tick :: proc(s:^Socket_Server,now:time.Duration)->Socket_Error {
    if !s.active { return .Unavailable }
    failure:=Socket_Error.None
    for &client in s.clients {
        if client.active { continue }
        channel,err,accepted:=socket.accept(&s.listener)
        if err!=.None { failure=.IO; break }; if !accepted { break }
        _,_,buffer_error:=socket.buffer_prepare(&channel,true)
        if buffer_error!=.None { socket.close(&channel); failure=.IO; break }
        client={channel=channel,active=true,input=make([dynamic]byte,s.allocator),output=make([dynamic]string,s.allocator),last_output_progress=now}
        server_init(&client.protocol,s.mailbox,s.allocator)
    }
    for &client in s.clients {
        if !client.active { continue }
        err:=client_input(s,&client,now)
        if err==.None {
            for _ in 0..<4 {
                reply:=server_poll(&client.protocol,now); if reply=="" { break }
                if !queue_output(s,&client,reply,now) { err=.Backpressure; break }
            }
        }
        if err==.None { err=client_output(s,&client,now) }
        if err==.None && len(client.output)>0 && now>=client.last_output_progress && now-client.last_output_progress>15*time.Second { err=.Backpressure }
        if err!=.None { failure=err; client_destroy(s,&client); continue }
        if client.input_closed && len(client.protocol.pending)==0 && len(client.output)==0 { client_destroy(s,&client) }
    }
    return failure
}

/// Reports accepted live transports for application lifecycle/status without exposing client state.
socket_server_client_count :: proc(s:^Socket_Server)->int {
    count:=0; for client in s.clients { if client.active { count+=1 } }; return count
}
