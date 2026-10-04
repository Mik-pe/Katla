#+test
package mcp

import app "../../app"
import ecs "../../ecs"
import editor "../../editor"
import "core:testing"
import "core:encoding/json"
import "core:fmt"
import "core:mem"
import "core:time"
import "core:strings"
import "core:thread"
import "core:sync"

request :: proc(id,method:string,params:=`{}`)->string {
    return fmt.aprintf(`{{"jsonrpc":"2.0","id":%s,"method":"%s","params":%s}}`,id,method,params)
}
metadata :: proc(extra:="",version:=PROTOCOL_VERSION)->string {
    return fmt.aprintf(`{{"_meta":{{"io.modelcontextprotocol/protocolVersion":"%s","io.modelcontextprotocol/clientCapabilities":{{}}}}%s}}`,version,extra)
}
receive :: proc(s:^Server,id,method:string,extra:="",now:time.Duration=0)->string {
    params:=metadata(extra); defer delete(params)
    line:=request(id,method,params); defer delete(line)
    return server_receive(s,line,now)
}
check_error :: proc(t:^testing.T,output:string,code:int) {
    tree,err:=json.parse(output,spec=.JSON,parse_integers=true); testing.expect_value(t,err,json.Error.None)
    if err!=nil { return }; defer json.destroy_value(tree)
    object,ok:=tree.(json.Object); testing.expect(t,ok)
    error,error_ok:=object["error"].(json.Object); testing.expect(t,error_ok && error["code"].(json.Integer)==json.Integer(code))
}
@(test)
test_poll_preserves_unrelated_transport_replies_and_reserved_credits :: proc(t:^testing.T) {
    scene:app.Authoring; app.authoring_init(&scene); defer app.authoring_destroy(&scene)
    s:Server; server_init(&s,&scene.agent); defer server_destroy(&s)
    unrelated,admission:=editor.agent_submit(&scene.agent,{kind=.Spawn},"llm-call")
    testing.expect_value(t,admission,editor.Mailbox_Error.None)
    accepted:=receive(&s,`"mcp-call"`,"tools/call",`,"name":"spawn_entity","arguments":{}`); defer delete(accepted)
    testing.expect(t,accepted=="" && scene.agent.outstanding==2)
    testing.expect_value(t,app.authoring_tick(&scene),2)
    reply:=server_poll(&s,0); defer delete(reply)
    tree,err:=json.parse(reply); testing.expect_value(t,err,json.Error.None); defer json.destroy_value(tree)
    testing.expect(t,tree.(json.Object)["id"].(string)=="mcp-call" && scene.agent.outstanding==1)
    again:=server_poll(&s,0); defer delete(again)
    testing.expect(t,again=="" && scene.agent.outstanding==1)
    result,ready:=editor.agent_take_result_for(&scene.agent,unrelated); defer editor.agent_response_destroy(&result)
    testing.expect(t,ready && result.call_id=="llm-call" && result.result.error==.None && scene.agent.outstanding==0)
}

@(test)
test_discovery_requires_metadata_on_every_request :: proc(t:^testing.T) {
    scene:app.Authoring; app.authoring_init(&scene); defer app.authoring_destroy(&scene)
    s:Server; server_init(&s,&scene.agent); defer server_destroy(&s)
    output:=receive(&s,`"probe"`,"server/discover"); defer delete(output)
    tree,err:=json.parse(output); testing.expect_value(t,err,json.Error.None); defer json.destroy_value(tree)
    object:=tree.(json.Object); result:=object["result"].(json.Object)
    testing.expect(t,object["id"].(string)=="probe" && result["resultType"].(string)=="complete")
    versions:=result["supportedVersions"].(json.Array); testing.expect(t,len(versions)==1 && versions[0].(string)==PROTOCOL_VERSION)
    params:=metadata(version="2025-11-25"); defer delete(params)
    wrong:=request("1","ping",params); defer delete(wrong)
    rejected:=server_receive(&s,wrong,0); defer delete(rejected); check_error(t,rejected,-32022)
    missing:=request("2","ping"); defer delete(missing)
    rejected_meta:=server_receive(&s,missing,0); defer delete(rejected_meta); check_error(t,rejected_meta,-32602)
    legacy:=receive(&s,"3","initialize"); defer delete(legacy); check_error(t,legacy,-32601)
    unknown:=receive(&s,"4","unsupported"); defer delete(unknown); check_error(t,unknown,-32601)
    malformed_info:=metadata(``,PROTOCOL_VERSION); defer delete(malformed_info)
    bad_info,_:=strings.replace_all(malformed_info,`"io.modelcontextprotocol/clientCapabilities":{}`,`"io.modelcontextprotocol/clientCapabilities":{},"io.modelcontextprotocol/clientInfo":5`); defer delete(bad_info)
    bad_request:=request("5","ping",bad_info); defer delete(bad_request)
    bad_reply:=server_receive(&s,bad_request,0); defer delete(bad_reply); check_error(t,bad_reply,-32602)
    testing.expect(t,scene.world.live_count==0 && len(scene.agent.session.actions)==0)
}
@(test)
test_tool_calls_keep_typed_ids_and_validate_before_scene_execution :: proc(t:^testing.T) {
    scene:app.Authoring; app.authoring_init(&scene); defer app.authoring_destroy(&scene)
    s:Server; server_init(&s,&scene.agent); defer server_destroy(&s)
    first:=receive(&s,"9007199254740993","tools/call",`,"name":"spawn_entity","arguments":{}`); defer delete(first)
    second:=receive(&s,`"9007199254740993"`,"tools/call",`,"name":"spawn_entity","arguments":{}`); defer delete(second)
    testing.expect(t,first=="" && second=="" && scene.world.live_count==0 && len(s.pending)==2)
    duplicate:=receive(&s,"9007199254740993","tools/call",`,"name":"spawn_entity","arguments":{}`); defer delete(duplicate)
    check_error(t,duplicate,-32600)
    malformed:=receive(&s,"0","tools/call",`,"name":"spawn_entity","arguments":{"shape":"cube"}`); defer delete(malformed)
    invalid,err:=json.parse(malformed); testing.expect_value(t,err,json.Error.None); defer json.destroy_value(invalid)
    testing.expect(t,bool(invalid.(json.Object)["result"].(json.Object)["isError"].(json.Boolean)))
    absent:=receive(&s,"1","tools/call",`,"name":"missing"`); defer delete(absent); check_error(t,absent,-32602)
    testing.expect_value(t,app.authoring_tick(&scene),2)
    for is_string in ([2]bool{false,true}) {
        reply:=server_poll(&s,0)
        tree,parse_err:=json.parse(reply,parse_integers=true); testing.expect_value(t,parse_err,json.Error.None)
        object:=tree.(json.Object)
        if is_string { testing.expect_value(t,object["id"].(string),"9007199254740993") }
        else { testing.expect_value(t,object["id"].(json.Integer),json.Integer(9007199254740993)) }
        payload:=object["result"].(json.Object)["structuredContent"].(json.Object)
        entities:=payload["entity_ids"].(json.Array); testing.expect_value(t,len(entities),1)
        id,valid:=entities[0].(string); testing.expect(t,valid && len(id)>0)
        json.destroy_value(tree); delete(reply)
    }
    testing.expect(t,len(s.pending)==0 && scene.world.live_count==2)
    testing.expect_value(t,editor.agent_undo_all(&scene.agent.session,&scene.world,&scene.registry),editor.Scene_Error.None)
    testing.expect_value(t,scene.world.live_count,0)
}
@(test)
test_cancel_suppresses_queued_and_already_accepted_replies :: proc(t:^testing.T) {
    scene:app.Authoring; app.authoring_init(&scene); defer app.authoring_destroy(&scene)
    s:Server; server_init(&s,&scene.agent); defer server_destroy(&s)
    for queued in ([2]bool{true,false}) {
        output:=receive(&s,`"cancel"`,"tools/call",`,"name":"spawn_entity"`); testing.expect_value(t,output,"")
        if !queued { app.authoring_tick(&scene) }
        cancelled:=server_receive(&s,`{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"cancel"}}`,0)
        testing.expect_value(t,cancelled,"")
        app.authoring_tick(&scene)
        testing.expect_value(t,server_poll(&s,0),"")
        testing.expect_value(t,len(s.pending),0)
    }
    testing.expect(t,scene.world.live_count==1 && len(scene.agent.session.actions)==1)
    unknown:=server_receive(&s,`{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":999}}`,0)
    testing.expect_value(t,unknown,"")
    notification:=server_receive(&s,`{"jsonrpc":"2.0","method":"tools/call","params":{"name":"spawn_entity"}}`,0)
    testing.expect_value(t,notification,"")
    testing.expect_value(t,len(scene.agent.requests),0)
}
@(test)
test_deadline_cancels_stopped_owner_and_closure_drains_accepted_work :: proc(t:^testing.T) {
    scene:app.Authoring; app.authoring_init(&scene,agent_capacity=1); defer app.authoring_destroy(&scene)
    s:Server; server_init(&s,&scene.agent,timeout=time.Second); defer server_destroy(&s)
    output:=receive(&s,"1","tools/call",`,"name":"spawn_entity"`); testing.expect_value(t,output,"")
    full:=receive(&s,"2","tools/call",`,"name":"spawn_entity"`); defer delete(full); check_error(t,full,1002)
    testing.expect_value(t,server_poll(&s,time.Second-1),"")
    expired:=server_poll(&s,time.Second); defer delete(expired); check_error(t,expired,1004)
    testing.expect(t,app.authoring_tick(&scene)==0 && scene.world.live_count==0 && len(s.pending)==0)
    accepted:=receive(&s,"3","tools/call",`,"name":"spawn_entity"`,time.Second); testing.expect_value(t,accepted,"")
    server_finish(&s)
    closed:=receive(&s,"4","tools/call",`,"name":"spawn_entity"`,time.Second); defer delete(closed); check_error(t,closed,1003)
    testing.expect_value(t,app.authoring_tick(&scene),1)
    reply:=server_poll(&s,time.Second); defer delete(reply)
    testing.expect(t,len(reply)>0 && scene.world.live_count==1 && !scene.agent.session.finished && len(s.pending)==0)
}
@(test)
test_strict_json_guards_depth_trailing_data_numbers_and_duplicate_keys :: proc(t:^testing.T) {
    for input in ([]string{`{} {}`,`{"id":1,"id":2}`,`{"":"ignored"}`,`{"n":9223372036854775808}`,`{"n":1e999}`,`{"n":01}`,`{"n":00.1}`,`{"n":1.}`,`{"n":NaN}`,`{"x":`,"\xff","{}\x00{}"}) {
        tree,valid:=parse_message(input,context.allocator); testing.expect(t,!valid,input)
        if valid { json.destroy_value(tree) }
    }
    opening:=strings.repeat("[",MAX_JSON_DEPTH+1); defer delete(opening)
    closing:=strings.repeat("]",MAX_JSON_DEPTH+1); defer delete(closing)
    deep:=strings.concatenate({opening,closing}); defer delete(deep)
    tree,valid:=parse_message(deep,context.allocator); testing.expect(t,!valid)
    if valid { json.destroy_value(tree) }
    scene:app.Authoring; app.authoring_init(&scene); defer app.authoring_destroy(&scene)
    s:Server; server_init(&s,&scene.agent); defer server_destroy(&s)
    for input in ([]string{`[]`,`{"jsonrpc":"2.0","id":null,"method":"ping"}`,`{"jsonrpc":"2.0","id":1.5,"method":"ping"}`}) {
        output:=server_receive(&s,input,0); check_error(t,output,-32600); delete(output)
    }
}
@(test)
test_server_releases_pending_cancelled_unread_and_transferred_ownership :: proc(t:^testing.T) {
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing); defer mem.tracking_allocator_destroy(&tracker)
    context.allocator=mem.tracking_allocator(&tracker)
    scene:app.Authoring; app.authoring_init(&scene)
    s:Server; server_init(&s,&scene.agent)
    output:=receive(&s,`"owned"`,"tools/call",`,"name":"spawn_entity"`); testing.expect_value(t,output,"")
    app.authoring_tick(&scene)
    response:=server_poll(&s,0)
    pending:=receive(&s,`"pending"`,"tools/call",`,"name":"spawn_entity"`); testing.expect_value(t,pending,"")
    context.allocator=backing
    delete(response,s.allocator); server_destroy(&s); app.authoring_destroy(&scene)
    testing.expect_value(t,len(tracker.allocation_map),0)
}

Stalled_Owner :: struct { mailbox:^editor.Agent_Harness, queued,entered,release,complete:sync.Sema, error:string, suppressed:bool }
stalled_execute :: proc(state:rawptr,w:^ecs.World,reg:^editor.Component_Registry,op:editor.Scene_Op)->(editor.Tool_Result,editor.Undo_Group) {
    probe:=cast(^Stalled_Owner)state
    sync.sema_post(&probe.entered); sync.sema_wait(&probe.release)
    return editor.scene_execute(w,reg,op)
}
stalled_host :: proc(th:^thread.Thread) {
    probe:=cast(^Stalled_Owner)th.data
    server:Server; server_init(&server,probe.mailbox,probe.mailbox.allocator,timeout=time.Nanosecond)
    defer server_destroy(&server)
    started:=time.tick_now()
    output:=receive(&server,`"stalled"`,"tools/call",`,"name":"spawn_entity"`)
    assert(output=="")
    sync.sema_post(&probe.queued); sync.sema_wait(&probe.entered)
    probe.error=server_poll(&server,time.tick_since(started))
    sync.sema_post(&probe.release); sync.sema_wait(&probe.complete)
    probe.suppressed=server_poll(&server,time.tick_since(started))=="" && len(server.pending)==0
}
@(test)
test_real_stalled_owner_deadline_suppresses_reply_without_reverting_accepted_mutation :: proc(t:^testing.T) {
    scene:app.Authoring; app.authoring_init(&scene); defer app.authoring_destroy(&scene)
    probe:=Stalled_Owner{mailbox=&scene.agent}
    worker:=thread.create(stalled_host); worker.data=&probe; thread.start(worker)
    sync.sema_wait(&probe.queued)
    processed:=editor.agent_tick(&scene.agent,&scene.world,&scene.registry,{&probe,stalled_execute})
    sync.sema_post(&probe.complete)
    thread.join(worker); thread.destroy(worker)
    check_error(t,probe.error,1004); delete(probe.error,scene.agent.allocator)
    testing.expect(t,processed==1 && probe.suppressed && scene.world.live_count==1 && len(scene.agent.session.actions)==1)
    testing.expect_value(t,editor.agent_undo_all(&scene.agent.session,&scene.world,&scene.registry),editor.Scene_Error.None)
    testing.expect_value(t,scene.world.live_count,0)
}

@(test)
test_connection_shutdown_leaves_other_producers_and_action_history_alive :: proc(t:^testing.T) {
    scene:app.Authoring; app.authoring_init(&scene,agent_capacity=3); defer app.authoring_destroy(&scene)
    server:Server; server_init(&server,&scene.agent)
    output:=receive(&server,"1","tools/call",`,"name":"spawn_entity"`); testing.expect_value(t,output,"")
    other,_:=editor.agent_submit(&scene.agent,{kind=.Query_Entities},"assistant")
    app.authoring_tick(&scene)
    server_finish(&server); server_destroy(&server)
    testing.expect(t,!scene.agent.finished_requested && scene.agent.outstanding==1 && len(scene.agent.session.actions)==2)
    response,ready:=editor.agent_take_result_for(&scene.agent,other); defer editor.agent_response_destroy(&response)
    testing.expect(t,ready && response.call_id=="assistant" && len(response.result.entities)==1)
    _,error:=editor.agent_submit(&scene.agent,{kind=.Spawn},"still-connected")
    testing.expect_value(t,error,editor.Mailbox_Error.None)
    testing.expect_value(t,editor.agent_undo_all(&scene.agent.session,&scene.world,&scene.registry),editor.Scene_Error.None)
    testing.expect_value(t,scene.world.live_count,0)
}
