#+test
package mcp

import "core:testing"
import "core:time"
import app "../../app"
import ecs "../../ecs"
import editor "../../editor"
import "core:encoding/json"
import "core:strings"

@(test)
test_editor_capture_oversize_error_is_owned_bounded_and_actionable :: proc(t:^testing.T) {
    scene:app.Authoring; app.authoring_init(&scene); defer app.authoring_destroy(&scene)
    server:Server; server_init(&server,&scene.agent); defer server_destroy(&server)
    admitted:=receive(&server,`"bounded-capture"`,"tools/call",`,"name":"editor_view","arguments":{"action":"observe"}`)
    testing.expect(t,admitted=="" && len(server.pending)==1)
    ticket:=server.pending[0].ticket; editor.agent_tick(&scene.agent,&scene.world,&scene.registry,{begin=view_begin})
    encoded:=`{"capture_error":"Viewport PNG exceeds 18 MiB. Reduce the viewport size."}`
    result:=editor.Tool_Result{error=.Invalid_Operation,allocator=context.allocator,data=make([]byte,len(encoded))}; defer editor.tool_result_destroy(&result); copy(result.data,transmute([]byte)encoded)
    testing.expect(t,editor.agent_complete_reply(&scene.agent,ticket,&result))
    reply:=server_poll(&server,0); defer delete(reply)
    value,error:=json.parse(reply); testing.expect(t,error==nil); defer json.destroy_value(value)
    root:=value.(json.Object); response:=root["result"].(json.Object); block:=response["content"].(json.Array)[0].(json.Object)
    testing.expect(t,root["id"].(string)=="bounded-capture" && response["isError"].(bool) && strings.contains(block["text"].(string),"Reduce the viewport size"))
    testing.expect(t,scene.agent.outstanding==0 && len(scene.agent.session.actions)==0)
    invalid:=view_error_message(transmute([]byte)string(`{"capture_error":42}`),context.allocator); defer delete(invalid); testing.expect_value(t,invalid,"")
    too_long:=make([]byte,2049); defer delete(too_long); testing.expect_value(t,view_error_message(too_long,context.allocator),"")
}

@(test)
test_editor_capture_returns_separate_image_and_exact_metadata :: proc(t:^testing.T) {
    bytes:=transmute([]byte)string(`{"submission":42,"selected_entity":null,"center":{"raw":17,"entity_id":"18446744073709551615"},"image_png_base64":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="}`)
    output:=view_result(`"capture"`,bytes,context.allocator); defer delete(output)
    tree,err:=json.parse(output); testing.expect(t,err==nil); if err!=nil { return }; defer json.destroy_value(tree)
    root:=tree.(json.Object); result:=root["result"].(json.Object); metadata:=result["structuredContent"].(json.Object)
    _,no_selection:=metadata["selected_entity"].(json.Null)
    testing.expect(t,root["id"].(string)=="capture" && no_selection)
    _,duplicated:=metadata["image_png_base64"]; testing.expect(t,!duplicated)
    blocks:=result["content"].(json.Array); image:=blocks[1].(json.Object)
    testing.expect(t,len(blocks)==2 && image["type"].(string)=="image" && image["mimeType"].(string)=="image/png")
    center:=metadata["center"].(json.Object); testing.expect_value(t,center["entity_id"].(string),"18446744073709551615")
}

view_begin :: proc(_:rawptr,_:^ecs.World,_:^editor.Component_Registry,op:editor.Scene_Op,_:u64)->bool {
    return op.kind==.Application && op.tool_name=="editor_view"
}
@(test)
test_deferred_view_deadline_abandons_only_its_reply_credit :: proc(t:^testing.T) {
    scene:app.Authoring; app.authoring_init(&scene,agent_capacity=1); defer app.authoring_destroy(&scene)
    server:Server; server_init(&server,&scene.agent,timeout=time.Second); defer server_destroy(&server)
    admitted:=receive(&server,`"capture"`,"tools/call",`,"name":"editor_view","arguments":{"action":"observe"}`)
    testing.expect(t,admitted=="" && len(server.pending)==1)
    ticket:=server.pending[0].ticket
    editor.agent_tick(&scene.agent,&scene.world,&scene.registry,{begin=view_begin})
    testing.expect(t,editor.agent_is_deferred(&scene.agent,ticket) && scene.agent.outstanding==1)
    expired:=server_poll(&server,time.Second); defer delete(expired); check_error(t,expired,1004)
    testing.expect(t,!editor.agent_is_deferred(&scene.agent,ticket) && scene.agent.outstanding==0 && len(server.pending)==0)
    result:=editor.Tool_Result{allocator=scene.agent.allocator}; undo:editor.Undo_Group
    testing.expect(t,!editor.agent_complete(&scene.agent,ticket,&result,&undo))
    _,err:=editor.agent_submit(&scene.agent,{kind=.Spawn},"other-consumer")
    testing.expect_value(t,err,editor.Mailbox_Error.None)
}
