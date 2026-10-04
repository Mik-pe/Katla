#+test
package host

import "core:testing"
import "core:os"
import "core:mem"
import "core:strings"
import "core:encoding/json"

TEST_PNG :: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
@(test)
test_host_queue_failure_keeps_progress_and_terminal_ownership :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    a:=mem.tracking_allocator(&tracker)
    b:=Bridge{allocator=a,events=make([dynamic]Event,a),commands=make([dynamic]Command,a),connected=true}
    for _ in 0..<MAX_EVENTS { testing.expect(t,emit(&b,{kind=.Text,turn_id="turn",item_id="item",text="delta"})) }
    testing.expect(t,!emit(&b,{kind=.Text,text="overflow"}) && !bridge_connected(&b))
    count:=0
    for { event,ready:=bridge_poll(&b); if !ready { break }; count+=1; event_destroy(&event) }
    testing.expect_value(t,count,MAX_EVENTS+1)
    bridge_destroy(&b); testing.expect_value(t,len(tracker.allocation_map),0)
}
@(test)
test_host_submit_clones_bounded_payload_and_exact_cancel_scope :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    a:=mem.tracking_allocator(&tracker)
    b:=Bridge{allocator=a,events=make([dynamic]Event,a),commands=make([dynamic]Command,a),connected=true}
    text:=strings.clone("question",a)
    testing.expect_value(t,bridge_submit(&b,text,`{"selected":null}`,TEST_PNG),Error.None)
    delete(text,a)
    command,ready:=pop_command(&b); testing.expect(t,ready && command.text=="question" && command.png==TEST_PNG)
    command_destroy(&command,a)
    testing.expect_value(t,bridge_cancel(&b,"not-accepted"),Error.Invalid_Config)
    emit(&b,{kind=.Accepted,turn_id="accepted-turn"})
    testing.expect_value(t,bridge_cancel(&b,"other-turn"),Error.Invalid_Config)
    testing.expect_value(t,bridge_cancel(&b,"accepted-turn"),Error.None)
    for _ in 0..<3 { testing.expect_value(t,bridge_submit(&b,"next",`{}`,TEST_PNG),Error.None) }
    testing.expect_value(t,bridge_submit(&b,"overflow",`{}`,TEST_PNG),Error.Full)
    testing.expect_value(t,bridge_submit(&b,"question",`[]`,TEST_PNG),Error.Protocol)
    testing.expect_value(t,bridge_submit(&b,"question",`{}`,"invalid"),Error.Protocol)
    bridge_destroy(&b); testing.expect_value(t,len(tracker.allocation_map),0)
}
@(test)
test_host_notifications_filter_thread_keep_turn_item_and_host_attention :: proc(t:^testing.T) {
    b:=Bridge{allocator=context.allocator,config={thread_id=strings.clone("selected")},events=make([dynamic]Event)}
    defer bridge_destroy(&b)
    s:=Session{bridge=&b}
    messages:=[]string{
        `{"method":"item/agentMessage/delta","params":{"threadId":"other","turnId":"turn","itemId":"item","delta":"ignore"}}`,
        `{"method":"item/agentMessage/delta","params":{"threadId":"selected","turnId":"turn","itemId":"item","delta":"text"}}`,
        `{"id":"approval","method":"approval/request","params":{}}`,
        `{"method":"turn/completed","params":{"threadId":"selected","turn":{"id":"turn","status":"completed"}}}`,
    }
    for text in messages {
        tree,valid:=parse(text,context.allocator); testing.expect(t,valid)
        object,ok:=tree.(json.Object); testing.expect(t,ok); notification(&s,object); json.destroy_value(tree)
    }
    e,ready:=bridge_poll(&b); testing.expect(t,ready && e.kind==.Text && e.turn_id=="turn" && e.item_id=="item" && e.text=="text"); event_destroy(&e)
    e,ready=bridge_poll(&b); testing.expect(t,ready && e.kind==.Attention); event_destroy(&e)
    e,ready=bridge_poll(&b); testing.expect(t,ready && e.kind==.Finished && e.turn_id=="turn"); event_destroy(&e)
    _,ready=bridge_poll(&b); testing.expect(t,!ready)
}
@(test)
test_host_parser_rejects_deep_invalid_utf8_and_trailing_documents :: proc(t:^testing.T) {
    deep:=strings.repeat("[",65); defer delete(deep)
    for text in ([]string{"\xff",`{} {}`,deep}) {
        tree,valid:=parse(text,context.allocator); testing.expect(t,!valid); if valid { json.destroy_value(tree) }
    }
}

@(test)
test_connection_preferences_roundtrip_offline_keeps_captured_owners :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-host-preferences-*",context.allocator)
    testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    path:=strings.concatenate({directory,"/connection.json"}); defer delete(path)
    testing.expect_value(t,config_save({socket="/private/offline.sock",thread_id="selected-existing"},path),Error.None)
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    a:=mem.tracking_allocator(&tracker)
    c,err:=config_load(path,a); testing.expect(t,err==.None && c.socket=="/private/offline.sock" && c.thread_id=="selected-existing")
    config_destroy(&c,a)
    testing.expect_value(t,len(tracker.allocation_map),0)
    testing.expect_value(t,config_save({socket="relative.sock",thread_id="selected-existing"},path),Error.Invalid_Config)
    restored,restore_error:=config_load(path); defer config_destroy(&restored)
    testing.expect(t,restore_error==.None && restored.socket=="/private/offline.sock")
}
