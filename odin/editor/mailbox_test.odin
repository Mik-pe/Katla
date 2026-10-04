#+test
package editor

import ecs "../ecs"
import "core:testing"
import "core:strings"
import "core:mem"
import "core:thread"
import "core:sync"

@(test)
test_mailbox_credit_survives_execution_and_unread_reply :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    h:Agent_Harness; agent_harness_init(&h,capacity=1); defer agent_harness_destroy(&h)
    call_id:=strings.clone("request-α")
    ticket,err:=agent_submit(&h,{kind=.Spawn},call_id); delete(call_id)
    testing.expect(t,err==.None && ticket>0 && w.live_count==0)
    rejected,full:=agent_submit(&h,{kind=.Spawn},"queued-overflow")
    testing.expect(t,rejected==0 && full==.Full)
    testing.expect_value(t,agent_tick(&h,&w,&reg),1)
    rejected,full=agent_submit(&h,{kind=.Spawn},"reply-overflow")
    testing.expect(t,rejected==0 && full==.Full && w.live_count==1 && len(h.session.actions)==1)
    testing.expect(t,!agent_cancel(&h,ticket))
    response,ok:=agent_take_result(&h)
    testing.expect(t,ok && response.ticket==ticket && response.call_id=="request-α" && response.id==0 && response.result.error==.None)
    agent_response_destroy(&response)
    next,accepted:=agent_submit(&h,{kind=.Query_Entities},"same-scene")
    testing.expect(t,accepted==.None && next>ticket)
    agent_finish(&h)
    testing.expect_value(t,agent_tick(&h,&w,&reg),1)
    response,ok=agent_take_result(&h)
    testing.expect(t,ok && response.ticket==next && response.call_id=="same-scene" && len(response.result.entities)==1 && h.session.finished)
    agent_response_destroy(&response)
    closed_ticket,closed:=agent_submit(&h,{kind=.Spawn})
    testing.expect(t,closed_ticket==0 && closed==.Closed && w.live_count==1)
}

@(test)
test_cancel_queued_request_releases_credit_and_never_mutates :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    h:Agent_Harness; agent_harness_init(&h,capacity=2); defer agent_harness_destroy(&h)
    cancelled,first:=agent_submit(&h,{kind=.Spawn},"cancelled")
    retained,second:=agent_submit(&h,{kind=.Spawn},"retained")
    testing.expect(t,first==.None && second==.None)
    testing.expect(t,agent_cancel(&h,cancelled) && !agent_cancel(&h,cancelled) && !agent_cancel(&h,0))
    replacement,third:=agent_submit(&h,{kind=.Query_Entities},"replacement")
    testing.expect(t,third==.None && replacement>retained)
    agent_finish(&h)
    // Closure preserves cancellation rights for work the owner has not taken.
    testing.expect(t,agent_cancel(&h,retained))
    testing.expect_value(t,agent_tick(&h,&w,&reg),1)
    response,ok:=agent_take_result(&h); defer agent_response_destroy(&response)
    testing.expect(t,ok && response.ticket==replacement && response.call_id=="replacement" && w.live_count==0 && len(response.result.entities)==0)
    testing.expect(t,h.session.finished && h.outstanding==0 && len(h.session.actions)==1)
    _,another:=agent_take_result(&h); testing.expect(t,!another)
}

@(test)
test_action_ids_are_not_reused_after_undo :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    session:Agent_Session; agent_session_init(&session); defer agent_session_destroy(&session)
    first:=agent_execute(&session,&w,&reg,{kind=.Spawn}).id
    testing.expect_value(t,agent_undo_last(&session,&w,&reg),Scene_Error.None)
    second:=agent_execute(&session,&w,&reg,{kind=.Spawn}).id
    testing.expect(t,second>first && w.live_count==1)
}

@(test)
test_mailbox_identifier_exhaustion_rejects_before_mutation :: proc(t:^testing.T) {
    h:Agent_Harness; agent_harness_init(&h); defer agent_harness_destroy(&h)
    h.next_ticket=max(u64)-1
    last,err:=agent_submit(&h,{kind=.Spawn},"last")
    testing.expect(t,last==max(u64)-1 && err==.None)
    zero,exhausted:=agent_submit(&h,{kind=.Spawn},"overflow")
    testing.expect(t,zero==0 && exhausted==.Identifier_Exhausted && len(h.requests)==1 && h.outstanding==1)
    testing.expect(t,agent_cancel(&h,last))
    zero,exhausted=agent_submit(&h,{kind=.Spawn})
    testing.expect(t,zero==0 && exhausted==.Identifier_Exhausted && h.outstanding==0)
}

@(test)
test_mailbox_captured_allocator_releases_all_ownership_paths :: proc(t:^testing.T) {
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    defer mem.tracking_allocator_destroy(&tracker)
    allocator:=mem.tracking_allocator(&tracker)
    context.allocator=allocator
    w:ecs.World; ecs.world_init(&w,allocator=allocator)
    reg:Component_Registry; editor_registry_init(&reg,allocator)
    h:Agent_Harness; agent_harness_init(&h,allocator,capacity=3)
    one,err:=agent_submit(&h,{kind=.Spawn,name="temporary"},"cancelled")
    testing.expect(t,err==.None && agent_cancel(&h,one))
    _,accepted:=agent_submit(&h,{kind=.Query_Entities},"transferred")
    testing.expect_value(t,accepted,Mailbox_Error.None)
    agent_tick(&h,&w,&reg)
    response,ok:=agent_take_result(&h); testing.expect(t,ok)
    _,accepted=agent_submit(&h,{kind=.Query_Entities},"unread")
    testing.expect_value(t,accepted,Mailbox_Error.None)
    agent_tick(&h,&w,&reg)
    _,accepted=agent_submit(&h,{kind=.Spawn,name="queued"},"unprocessed")
    testing.expect_value(t,accepted,Mailbox_Error.None)
    context.allocator=backing
    agent_response_destroy(&response)
    agent_harness_destroy(&h); editor_registry_destroy(&reg); ecs.world_destroy(&w)
    testing.expect_value(t,len(tracker.allocation_map),0)
}

Mailbox_Producer :: struct { harness:^Agent_Harness, tickets:[64]u64, accepted,full:int }
test_mailbox_admission_producer :: proc(th:^thread.Thread) {
    state:=cast(^Mailbox_Producer)th.data
    for _ in 0..<len(state.tickets) {
        ticket,err:=agent_submit(state.harness,{kind=.Spawn},"concurrent")
        switch err {
        case .None: state.tickets[state.accepted]=ticket; state.accepted+=1
        case .Full: assert(ticket==0); state.full+=1
        case .Closed,.Identifier_Exhausted: panic("unexpected admission error")
        }
    }
}
@(test)
test_concurrent_mailbox_admission_has_unique_tickets_and_exact_capacity :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    h:Agent_Harness; agent_harness_init(&h,capacity=7); defer agent_harness_destroy(&h)
    workers:[8]Mailbox_Producer; threads:[8]^thread.Thread
    for &worker,i in workers {
        worker.harness=&h
        threads[i]=thread.create(test_mailbox_admission_producer); threads[i].data=&worker; thread.start(threads[i])
    }
    for th in threads { thread.join(th); thread.destroy(th) }
    accepted,full:=0,0
    tickets:[8]bool
    for &worker in workers {
        accepted+=worker.accepted; full+=worker.full
        for ticket in worker.tickets[:worker.accepted] {
            testing.expect(t,ticket>=1 && ticket<=7)
            if ticket>=1 && ticket<=7 { testing.expect(t,!tickets[ticket]); tickets[ticket]=true }
        }
    }
    testing.expect(t,accepted==7 && full==505 && w.live_count==0 && h.outstanding==7)
    agent_finish(&h)
    testing.expect_value(t,agent_tick(&h,&w,&reg),7)
    for expected in 1..=7 {
        response,ok:=agent_take_result(&h)
        testing.expect(t,ok && response.ticket==u64(expected) && response.call_id=="concurrent" && response.result.error==.None)
        agent_response_destroy(&response)
    }
    testing.expect(t,h.session.finished && w.live_count==7 && h.outstanding==0)
}

In_Flight_Test :: struct { harness:^Agent_Harness, ticket:u64, entered,release:sync.Sema, rejected:u64, error:Mailbox_Error, cancelled:bool }
test_in_flight_executor :: proc(state:rawptr,w:^ecs.World,reg:^Component_Registry,op:Scene_Op)->(Tool_Result,Undo_Group) {
    probe:=cast(^In_Flight_Test)state
    sync.sema_post(&probe.entered); sync.sema_wait(&probe.release)
    return scene_execute(w,reg,op)
}
test_in_flight_producer :: proc(th:^thread.Thread) {
    probe:=cast(^In_Flight_Test)th.data
    sync.sema_wait(&probe.entered)
    probe.rejected,probe.error=agent_submit(probe.harness,{kind=.Spawn},"while-executing")
    probe.cancelled=agent_cancel(probe.harness,probe.ticket)
    agent_finish(probe.harness)
    sync.sema_post(&probe.release)
}
@(test)
test_executing_request_retains_credit_and_cannot_be_cancelled :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    h:Agent_Harness; agent_harness_init(&h,capacity=1); defer agent_harness_destroy(&h)
    ticket,err:=agent_submit(&h,{kind=.Spawn},"executing")
    testing.expect_value(t,err,Mailbox_Error.None)
    probe:=In_Flight_Test{harness=&h,ticket=ticket}
    worker:=thread.create(test_in_flight_producer); worker.data=&probe; thread.start(worker)
    processed:=agent_tick(&h,&w,&reg,{state=&probe,execute=test_in_flight_executor})
    thread.join(worker); thread.destroy(worker)
    testing.expect(t,processed==1 && probe.rejected==0 && probe.error==.Full && !probe.cancelled && h.session.finished && w.live_count==1)
    response,ok:=agent_take_result(&h); defer agent_response_destroy(&response)
    testing.expect(t,ok && response.ticket==ticket && response.call_id=="executing" && h.outstanding==0)
}
