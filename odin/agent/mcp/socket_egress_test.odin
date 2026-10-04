#+test
#+build darwin, linux
package mcp

import socket "../socket"
import app "../../app"
import editor "../../editor"
import "core:testing"
import "core:os"
import "core:strings"
import "core:time"

@(test)
test_socket_large_reply_has_bounded_owner_bursts_and_stalled_peer_fairness :: proc(t:^testing.T) {
    directory,path:=socket_test_path(t); if directory=="" { return }; defer { os.remove_all(directory); delete(directory); delete(path) }
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    server:Socket_Server; testing.expect_value(t,socket_server_init(&server,path,&owner.agent),Socket_Error.None); defer socket_server_destroy(&server)
    stalled,error:=socket.connect(path); testing.expect_value(t,error,socket.Error.None); defer socket.close(&stalled)
    responsive:socket.Channel; responsive,error=socket.connect(path); testing.expect_value(t,error,socket.Error.None); defer socket.close(&responsive)
    old_receive,receive_capacity,receive_error:=socket.buffer_prepare(&responsive,false)
    testing.expect(t,old_receive>0 && receive_error==.None && receive_capacity>=64<<10 && receive_capacity<=1<<20)
    testing.expect_value(t,socket_server_tick(&server,0),Socket_Error.None)
    before,capacity,buffer_error:=socket.buffer_prepare(&server.clients[1].channel,true)
    testing.expect(t,buffer_error==.None && before>=64<<10 && capacity>=64<<10 && capacity<=1<<20)
    blocked:=make([]byte,8<<20); defer delete(blocked)
    testing.expect(t,queue_output(&server,&server.clients[0],strings.clone(string(blocked)),0))
    payload:=make([]byte,4<<20); defer delete(payload)
    for &byte,index in payload { byte=u8('a'+index%26) }
    testing.expect(t,queue_output(&server,&server.clients[1],strings.clone(string(payload)),0))
    received:=0; complete:=false; buffer:[65536]byte; steps:=0
    for step in 0..<80 {
        steps=step+1
        testing.expect_value(t,socket_server_tick(&server,time.Duration(step)*50*time.Millisecond),Socket_Error.None)
        burst:=0
        for {
            n,read_error:=socket.read(&responsive,buffer[:]); testing.expect_value(t,read_error,socket.Error.None)
            if n==0 { break }
            for byte in buffer[:n] {
                if received<len(payload) { testing.expect(t,byte==payload[received]) }
                else { testing.expect(t,received==len(payload) && byte=='\n'); complete=true }
                received+=1
            }
            burst+=n
        }
        testing.expect(t,burst<=65536)
        if step==0 { testing.expect_value(t,burst,65536) }
        if complete { break }
    }
    testing.expect(t,complete && received==len(payload)+1 && steps<=65 && len(server.clients[1].output)==0)
    testing.expect(t,len(server.clients[0].output)==1 && server.clients[0].output_bytes>0 && socket_server_client_count(&server)==2)
    unrelated,admission:=editor.agent_submit(&owner.agent,{kind=.Spawn},"other-producer"); testing.expect_value(t,admission,editor.Mailbox_Error.None)
    socket.close(&stalled)
    testing.expect_value(t,socket_server_tick(&server,5*time.Second),Socket_Error.IO)
    testing.expect(t,socket_server_client_count(&server)==1 && owner.agent.outstanding==1)
    editor.agent_tick(&owner.agent,&owner.world,&owner.registry)
    response,ready:=editor.agent_take_result_for(&owner.agent,unrelated); defer editor.agent_response_destroy(&response)
    testing.expect(t,ready && response.result.error==.None && response.call_id=="other-producer" && owner.world.live_count==1)
}
