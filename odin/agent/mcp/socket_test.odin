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

socket_test_path :: proc(t:^testing.T)->(string,string) {
    directory,err:=os.make_directory_temp("","katla-mcp-socket-*",context.allocator); testing.expect(t,err==nil)
    if err!=nil { return "","" }; testing.expect(t,os.chmod(directory,{.Read_User,.Write_User,.Execute_User})==nil)
    return directory,strings.concatenate({directory,"/editor.sock"})
}
socket_test_send :: proc(t:^testing.T,c:^socket.Channel,id,method,extra:string) {
    params:=metadata(extra); defer delete(params); body:=request(id,method,params); defer delete(body)
    line:=strings.concatenate({body,"\n"}); defer delete(line)
    n,err:=socket.write(c,transmute([]byte)line); testing.expect(t,err==.None && n==len(line))
}
@(test)
test_socket_real_peer_output_fault_releases_only_its_deferred_credit :: proc(t:^testing.T) {
    directory,path:=socket_test_path(t); if directory=="" { return }; defer { os.remove_all(directory); delete(directory); delete(path) }
    owner:app.Authoring; app.authoring_init(&owner,agent_capacity=2); defer app.authoring_destroy(&owner)
    server:Socket_Server; testing.expect_value(t,socket_server_init(&server,path,&owner.agent),Socket_Error.None); defer socket_server_destroy(&server)
    client,connection:=socket.connect(path); testing.expect_value(t,connection,socket.Error.None); defer socket.close(&client)
    socket_server_tick(&server,0)
    socket_test_send(t,&client,`"view"`,"tools/call",`,"name":"editor_view","arguments":{"action":"observe"}`)
    socket_server_tick(&server,0)
    testing.expect(t,len(server.clients[0].protocol.pending)==1)
    view:=server.clients[0].protocol.pending[0].ticket
    unrelated,admission:=editor.agent_submit(&owner.agent,{kind=.Spawn},"other-producer"); testing.expect_value(t,admission,editor.Mailbox_Error.None)
    editor.agent_tick(&owner.agent,&owner.world,&owner.registry,{begin=view_begin})
    testing.expect(t,editor.agent_is_deferred(&owner.agent,view) && owner.agent.outstanding==2 && owner.world.live_count==1)
    socket.close(&client)
    err:=socket_server_tick(&server,16*time.Second)
    testing.expect(t,err==.IO && socket_server_client_count(&server)==0 && !editor.agent_is_deferred(&owner.agent,view) && owner.agent.outstanding==1)
    reply,ready:=editor.agent_take_result_for(&owner.agent,unrelated); defer editor.agent_response_destroy(&reply)
    testing.expect(t,ready && reply.result.error==.None && reply.call_id=="other-producer" && owner.agent.outstanding==0 && owner.world.live_count==1)
}
@(test)
test_socket_stalled_output_deadline_is_independent_of_incoming_notifications :: proc(t:^testing.T) {
    directory,path:=socket_test_path(t); if directory=="" { return }; defer { os.remove_all(directory); delete(directory); delete(path) }
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    server:Socket_Server; testing.expect_value(t,socket_server_init(&server,path,&owner.agent),Socket_Error.None); defer socket_server_destroy(&server)
    client,connection:=socket.connect(path); testing.expect_value(t,connection,socket.Error.None); defer socket.close(&client)
    socket_server_tick(&server,0)
    output:=make([]byte,8<<20); defer delete(output)
    // An already serialized response fills the actual OS peer receive buffer.
    testing.expect(t,queue_output(&server,&server.clients[0],strings.clone(string(output)),0))
    for _ in 0..<1024 { socket_server_tick(&server,0); if server.clients[0].last_output_progress!=0 { break } }
    testing.expect(t,len(server.clients[0].output)==1 && server.clients[0].output_bytes>0)
    notification:string=`{"jsonrpc":"2.0","method":"notifications/irrelevant"}`+"\n"
    for second in 1..<17 {
        n,err:=socket.write(&client,transmute([]byte)notification); testing.expect(t,err==.None && n==len(notification))
        tick_error:=socket_server_tick(&server,time.Duration(second)*time.Second)
        if second<16 { testing.expect_value(t,tick_error,Socket_Error.None) }
        else { testing.expect(t,tick_error==.Backpressure && socket_server_client_count(&server)==0) }
    }
}
