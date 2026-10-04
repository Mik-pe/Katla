//! Real host submission/result consumption stays separate from the scene owner.
package main

import agent "../../agent"
import app "../../app"
import ecs "../../ecs"
import editor "../../editor"
import "core:thread"
import "core:sync"
import "core:mem"
import "core:fmt"

CALLS :: 512
Host :: struct { mailbox:^editor.Agent_Harness, saturated:sync.Sema, submitted,received,rejected:int, last_ticket,last_action:u64 }
consume :: proc(host:^Host)->bool {
    response,ok:=editor.agent_take_result(host.mailbox)
    if !ok { return false }; defer editor.agent_response_destroy(&response)
    assert(response.result.error==.None && len(response.result.entities)==1)
    assert(response.ticket>host.last_ticket && (host.received==0 || response.id>host.last_action))
    expected:=fmt.aprintf("spawn-%d",host.received); defer delete(expected)
    assert(response.call_id==expected)
    host.last_ticket=response.ticket; host.last_action=response.id; host.received+=1
    return true
}
producer :: proc(th:^thread.Thread) {
    host:=cast(^Host)th.data
    cancelled,err:=agent.submit_call(host.mailbox,{"cancelled","spawn_entity",transmute([]byte)string(`{}`)})
    assert(err==.None && editor.agent_cancel(host.mailbox,cancelled))
    for host.submitted<CALLS {
        call_id:=fmt.aprintf("spawn-%d",host.submitted)
        ticket,result:=agent.submit_call(host.mailbox,{call_id,"spawn_entity",transmute([]byte)string(`{}`)})
        delete(call_id)
        switch result {
        case .None: assert(ticket>0); host.submitted+=1
        case .Mailbox_Full:
            assert(ticket==0); host.rejected+=1
            if host.rejected==1 { sync.sema_post(&host.saturated) }
            if !consume(host) { thread.yield() }
        case .Unknown_Tool,.Invalid_JSON,.Invalid_Arguments,.Mailbox_Closed,.Identifier_Exhausted:
            panic("host submission rejected unexpectedly")
        }
    }
    editor.agent_finish(host.mailbox)
    rejected,closed:=agent.submit_call(host.mailbox,{"after-finish","spawn_entity",transmute([]byte)string(`{}`)})
    assert(rejected==0 && closed==.Mailbox_Closed)
    for host.received<CALLS { if !consume(host) { thread.yield() } }
}
main :: proc() {
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0); mem.tracking_allocator_destroy(&tracker) }
    context.allocator=mem.tracking_allocator(&tracker)
    scene:app.Authoring; app.authoring_init(&scene,agent_capacity=3); defer app.authoring_destroy(&scene)
    existing:=ecs.spawn(&scene.world,struct { surface:app.Surface_Material }{app.Surface_Material{roughness=0.5,ao=1}})
    host:=Host{mailbox=&scene.agent}
    worker:=thread.create(producer); worker.data=&host; thread.start(worker)
    sync.sema_wait(&host.saturated)
    assert(scene.world.live_count==1 && ecs.entity_exists(&scene.world,existing))
    for !scene.agent.session.finished { app.authoring_tick(&scene); thread.yield() }
    thread.join(worker); thread.destroy(worker)
    assert(host.submitted==CALLS && host.received==CALLS && host.rejected>0)
    assert(scene.world.live_count==CALLS+1 && scene.agent.outstanding==0 && len(scene.agent.session.actions)==CALLS)
    assert(editor.agent_undo_all(&scene.agent.session,&scene.world,&scene.registry)==.None)
    assert(scene.world.live_count==1 && ecs.entity_exists(&scene.world,existing))
    fmt.printf("%d correlated calls/replies, capacity 3, queued cancellation and closed admission; undo preserved the existing scene\n",host.received)
}
