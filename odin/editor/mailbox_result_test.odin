#+test
package editor

import ecs "../ecs"
import "core:testing"
import "core:thread"
import "core:sync"
import "core:fmt"

@(test)
test_selective_response_preserves_other_transport_order_and_credits :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    h:Agent_Harness; agent_harness_init(&h,capacity=3); defer agent_harness_destroy(&h)
    a,a_error:=agent_submit(&h,{kind=.Spawn},"transport-a")
    b,b_error:=agent_submit(&h,{kind=.Spawn},"transport-b")
    c,c_error:=agent_submit(&h,{kind=.Spawn},"transport-c")
    testing.expect(t,a_error==.None && b_error==.None && c_error==.None)
    missing,found:=agent_take_result_for(&h,b); testing.expect(t,!found && h.outstanding==3); agent_response_destroy(&missing)
    testing.expect(t,agent_tick(&h,&w,&reg)==3)
    missing,found=agent_take_result_for(&h,0); testing.expect(t,!found); agent_response_destroy(&missing)
    missing,found=agent_take_result_for(&h,c+100); testing.expect(t,!found && h.outstanding==3); agent_response_destroy(&missing)
    selected,ok:=agent_take_result_for(&h,b); testing.expect(t,ok && selected.ticket==b && selected.call_id=="transport-b" && h.outstanding==2); agent_response_destroy(&selected)
    missing,found=agent_take_result_for(&h,b); testing.expect(t,!found && h.outstanding==2); agent_response_destroy(&missing)
    replacement,admitted:=agent_submit(&h,{kind=.Query_Entities},"new-transport")
    testing.expect(t,admitted==.None && replacement>c && h.outstanding==3)
    first,first_ok:=agent_take_result(&h); testing.expect(t,first_ok && first.ticket==a); agent_response_destroy(&first)
    second,second_ok:=agent_take_result(&h); testing.expect(t,second_ok && second.ticket==c); agent_response_destroy(&second)
    testing.expect(t,h.outstanding==1 && len(h.responses)==0)
    testing.expect(t,agent_cancel(&h,replacement) && h.outstanding==0)
}
@(private="package")
Selective_Consumer :: struct { mailbox:^Agent_Harness, tickets:[]u64, indices:[]int, failures:^u32 }
@(private="package")
selective_consume :: proc(th:^thread.Thread) {
    state:=cast(^Selective_Consumer)th.data
    for ticket,i in state.tickets {
        response,found:=agent_take_result_for(state.mailbox,ticket)
        if !found { sync.atomic_add(state.failures,1); continue }
        expected:=fmt.aprintf("transport-call-%d",state.indices[i])
        if response.ticket!=ticket || response.call_id!=expected || response.id!=u64(state.indices[i]) || response.result.error!=.None || len(response.result.entities)!=1 { sync.atomic_add(state.failures,1) }
        delete(expected); agent_response_destroy(&response)
        duplicate,taken:=agent_take_result_for(state.mailbox,ticket)
        if taken { sync.atomic_add(state.failures,1) }; agent_response_destroy(&duplicate)
    }
}
@(test)
test_four_selective_consumers_transfer_concurrent_replies_once :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    h:Agent_Harness; agent_harness_init(&h,capacity=64); defer agent_harness_destroy(&h)
    tickets:[4][16]u64; indices:[4][16]int
    for i in 0..<64 {
        id:=fmt.aprintf("transport-call-%d",i)
        ticket,err:=agent_submit(&h,{kind=.Spawn},id); delete(id); testing.expect(t,err==.None)
        tickets[i%4][i/4]=ticket; indices[i%4][i/4]=i
    }
    agent_finish(&h)
    for !h.session.finished { agent_tick(&h,&w,&reg) }
    testing.expect(t,w.live_count==64 && h.outstanding==64)
    failures:u32; states:[4]Selective_Consumer; workers:[4]^thread.Thread
    for i in 0..<4 {
        states[i]={mailbox=&h,tickets=tickets[i][:],indices=indices[i][:],failures=&failures}
        workers[i]=thread.create(selective_consume); workers[i].data=&states[i]; thread.start(workers[i])
    }
    for worker in workers { thread.join(worker); thread.destroy(worker) }
    testing.expect(t,sync.atomic_load(&failures)==0 && h.outstanding==0 && len(h.responses)==0 && len(h.session.actions)==64)
    testing.expect(t,agent_undo_all(&h.session,&w,&reg)==.None && w.live_count==0)
}
