#+test
package editor

import ecs "../ecs"
import "core:testing"
import "core:thread"
import "core:sync"

@(test)
test_abandon_queued_and_completed_tickets_preserves_other_reply :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    h:Agent_Harness; agent_harness_init(&h,capacity=3); defer agent_harness_destroy(&h)
    queued,_:=agent_submit(&h,{kind=.Spawn},"disconnected")
    completed,_:=agent_submit(&h,{kind=.Spawn},"unread")
    retained,_:=agent_submit(&h,{kind=.Query_Entities},"other-producer")
    testing.expect(t,agent_abandon(&h,queued) && !agent_abandon(&h,queued))
    testing.expect_value(t,agent_tick(&h,&w,&reg),2)
    testing.expect(t,agent_abandon(&h,completed) && !agent_abandon(&h,completed) && h.outstanding==1)
    response,ready:=agent_take_result_for(&h,retained); defer agent_response_destroy(&response)
    testing.expect(t,ready && response.call_id=="other-producer" && len(response.result.entities)==1 && h.outstanding==0)
    testing.expect(t,len(h.session.actions)==2 && w.live_count==1)
    testing.expect_value(t,agent_undo_all(&h.session,&w,&reg),Scene_Error.None)
    testing.expect_value(t,w.live_count,0)
}
Abandon_Probe :: struct { harness:^Agent_Harness,ticket:u64,entered,release:sync.Sema,abandoned,twice:bool,error:Mailbox_Error }
test_abandon_executor :: proc(state:rawptr,w:^ecs.World,reg:^Component_Registry,op:Scene_Op)->(Tool_Result,Undo_Group) {
    probe:=cast(^Abandon_Probe)state
    sync.sema_post(&probe.entered); sync.sema_wait(&probe.release)
    return scene_execute(w,reg,op)
}
test_abandon_producer :: proc(th:^thread.Thread) {
    probe:=cast(^Abandon_Probe)th.data
    sync.sema_wait(&probe.entered)
    probe.abandoned=agent_abandon(probe.harness,probe.ticket)
    probe.twice=agent_abandon(probe.harness,probe.ticket)
    _,probe.error=agent_submit(probe.harness,{kind=.Spawn})
    sync.sema_post(&probe.release)
}
@(test)
test_abandon_executing_reply_retains_credit_until_action_completes :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    h:Agent_Harness; agent_harness_init(&h,capacity=1); defer agent_harness_destroy(&h)
    ticket,_:=agent_submit(&h,{kind=.Spawn},"disconnected-executing")
    probe:=Abandon_Probe{harness=&h,ticket=ticket}
    worker:=thread.create(test_abandon_producer); worker.data=&probe; thread.start(worker)
    processed:=agent_tick(&h,&w,&reg,{state=&probe,execute=test_abandon_executor})
    thread.join(worker); thread.destroy(worker)
    testing.expect(t,processed==1 && probe.abandoned && !probe.twice && probe.error==.Full)
    testing.expect(t,h.outstanding==0 && len(h.responses)==0 && w.live_count==1 && len(h.session.actions)==1)
    _,error:=agent_submit(&h,{kind=.Query_Entities},"still-open")
    testing.expect_value(t,error,Mailbox_Error.None)
    testing.expect_value(t,agent_undo_last(&h.session,&w,&reg),Scene_Error.None)
    testing.expect_value(t,w.live_count,0)
}
