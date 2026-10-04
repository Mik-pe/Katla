#+test
package editor

import "core:testing"
import ecs "../ecs"

test_defer_view :: proc(_:rawptr,_:^ecs.World,_:^Component_Registry,op:Scene_Op,_:u64)->bool {
    return op.kind==.Application && op.tool_name=="editor_view"
}

@(test)
test_deferred_reply_waits_preserves_credit_and_correlation :: proc(t:^testing.T) {
    h:Agent_Harness; agent_harness_init(&h,capacity=2); defer agent_harness_destroy(&h)
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    view,err:=agent_submit(&h,{kind=.Application,tool_name="editor_view"},"capture")
    testing.expect_value(t,err,Mailbox_Error.None)
    other,_:=agent_submit(&h,{kind=.Spawn},"other")
    agent_finish(&h)
    testing.expect_value(t,agent_tick(&h,&w,&reg,{begin=test_defer_view}),2)
    testing.expect(t,agent_is_deferred(&h,view) && !h.session.finished && h.outstanding==2)
    _,ready:=agent_take_result_for(&h,view); testing.expect(t,!ready)
    unrelated,other_ready:=agent_take_result_for(&h,other); testing.expect(t,other_ready && unrelated.call_id=="other")
    agent_response_destroy(&unrelated)
    result:=Tool_Result{allocator=h.allocator}; undo:Undo_Group
    testing.expect(t,agent_complete(&h,view,&result,&undo))
    testing.expect(t,!agent_complete(&h,view,&result,&undo) && !agent_is_deferred(&h,view))
    response,view_ready:=agent_take_result_for(&h,view); testing.expect(t,view_ready && response.call_id=="capture" && response.ticket==view)
    agent_response_destroy(&response)
    agent_tick(&h,&w,&reg)
    testing.expect(t,h.session.finished && h.outstanding==0)
}

@(test)
test_deferred_abandon_releases_only_own_ticket :: proc(t:^testing.T) {
    h:Agent_Harness; agent_harness_init(&h,capacity=1); defer agent_harness_destroy(&h)
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    ticket,_:=agent_submit(&h,{kind=.Application,tool_name="editor_view"},"cancel")
    agent_tick(&h,&w,&reg,{begin=test_defer_view})
    testing.expect(t,!agent_cancel(&h,ticket) && agent_abandon(&h,ticket) && !agent_abandon(&h,ticket))
    result:=Tool_Result{allocator=h.allocator}; undo:Undo_Group
    testing.expect(t,!agent_complete(&h,ticket,&result,&undo) && h.outstanding==0 && len(h.session.actions)==0)
    _,err:=agent_submit(&h,{kind=.Spawn},"still-open"); testing.expect_value(t,err,Mailbox_Error.None)
}
