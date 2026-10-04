#+test
package editor

import "core:testing"
import "core:fmt"
import "core:strings"
import ecs "../ecs"

@(test)
test_repeated_image_replies_transfer_owners_and_preserve_other_producer_history :: proc(t:^testing.T) {
    h:Agent_Harness; agent_harness_init(&h,capacity=2); defer agent_harness_destroy(&h)
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    mutation,_:=agent_submit(&h,{kind=.Spawn},"world-producer")
    agent_tick(&h,&w,&reg); response,ready:=agent_take_result_for(&h,mutation)
    testing.expect(t,ready && response.call_id=="world-producer" && w.live_count==1); previous:=response.id; agent_response_destroy(&response)
    for frame in 1..=64 {
        caller:=fmt.aprintf("image-%d",frame); defer delete(caller)
        ticket,error:=agent_submit(&h,{kind=.Application,tool_name="editor_view"},caller); testing.expect_value(t,error,Mailbox_Error.None)
        agent_tick(&h,&w,&reg,{begin=test_defer_view})
        payload:=fmt.aprintf(`{{"frame_id":"%d","image_png_base64":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="}}`,frame,allocator=h.allocator)
        result:=Tool_Result{data=transmute([]byte)payload,allocator=h.allocator,entities=make([dynamic]ecs.Entity_Id,h.allocator)}
        bytes:=raw_data(result.data)
        testing.expect(t,agent_complete_reply(&h,ticket,&result) && result.data==nil && h.outstanding==1)
        image,present:=agent_take_result_for(&h,ticket)
        testing.expect(t,present && image.call_id==caller && image.id>previous && raw_data(image.result.data)==bytes && string(image.result.data)==payload)
        previous=image.id; agent_response_destroy(&image)
        testing.expect(t,h.outstanding==0 && len(h.session.actions)==1 && w.live_count==1)
    }
    testing.expect_value(t,agent_undo_last(&h.session,&w,&reg),Scene_Error.None); testing.expect_value(t,w.live_count,0)
    testing.expect_value(t,agent_redo_last(&h.session,&w,&reg),Scene_Error.None); testing.expect_value(t,w.live_count,1)
}
@(test)
test_abandoned_image_reply_retains_caller_payload_and_frees_only_its_credit :: proc(t:^testing.T) {
    h:Agent_Harness; agent_harness_init(&h,capacity=2); defer agent_harness_destroy(&h)
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    view,_:=agent_submit(&h,{kind=.Application,tool_name="editor_view"},"abandoned")
    other,_:=agent_submit(&h,{kind=.Spawn},"other")
    agent_tick(&h,&w,&reg,{begin=test_defer_view}); testing.expect(t,agent_abandon(&h,view) && h.outstanding==1)
    result:=Tool_Result{allocator=h.allocator,data=transmute([]byte)strings.clone("late image",h.allocator)}; defer tool_result_destroy(&result)
    original:=raw_data(result.data); testing.expect(t,!agent_complete_reply(&h,view,&result) && raw_data(result.data)==original)
    response,ready:=agent_take_result_for(&h,other); defer agent_response_destroy(&response)
    testing.expect(t,ready && response.call_id=="other" && response.result.error==.None && h.outstanding==0 && w.live_count==1)
}
