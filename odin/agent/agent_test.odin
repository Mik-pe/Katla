#+test
package agent

import ecs "../ecs"
import editor "../editor"
import "core:testing"
import "core:fmt"
import "core:time"
import "core:thread"
import "core:sync"

Position :: struct { x,y,z:f32, scale_x,scale_y,scale_z:f32 }
@(test)
test_scene_call_mailbox_and_undo :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:editor.Component_Registry; editor.editor_registry_init(&reg); defer editor.editor_registry_destroy(&reg)
    editor.editor_register(&w,&reg,"Position",Position{})
    h:editor.Agent_Harness; editor.agent_harness_init(&h); defer editor.agent_harness_destroy(&h)
    ticket,submission_error:=submit_call(&h,{"a","spawn_entity",transmute([]byte)string(`{"position":[1,2,3]}`)})
    testing.expect_value(t,submission_error,Call_Error.None); testing.expect(t,ticket>0)
    testing.expect_value(t,w.live_count,0)
    testing.expect_value(t,editor.agent_tick(&h,&w,&reg),1)
    response,ok:=editor.agent_take_result(&h); testing.expect(t,ok && response.call_id=="a" && response.ticket>0); defer editor.agent_response_destroy(&response)
    id:=response.result.entities[0]
    position,present:=ecs.get_component(&w,id,Position)
    testing.expect(t,present && position.x==1 && position.y==2 && position.scale_x==1)
    args:=fmt.aprintf(`{{"entity_id":"%d","component":"Position","field":"x","value":7}}`,u64(id)); defer delete(args)
    ticket,submission_error=submit_call(&h,{"b","set_field",transmute([]byte)args})
    testing.expect_value(t,submission_error,Call_Error.None); testing.expect(t,ticket>0)
    testing.expect_value(t,editor.agent_tick(&h,&w,&reg),1)
    position,present=ecs.get_component(&w,id,Position); testing.expect(t,present && position.x==7)
    testing.expect_value(t,editor.agent_undo_last(&h.session,&w,&reg),editor.Scene_Error.None)
    position,present=ecs.get_component(&w,id,Position); testing.expect(t,present && position.x==1)
    testing.expect_value(t,editor.agent_undo_all(&h.session,&w,&reg),editor.Scene_Error.None)
    testing.expect_value(t,w.live_count,0)
}
@(test)
test_protocol_rejection_and_lossless_ids :: proc(t:^testing.T) {
    for text in ([]string{"0","9007199254740993","18446744073709551615"}) {
        id,ok:=parse_entity_id(text); testing.expect(t,ok)
        encoded:=fmt.aprintf("%d",u64(id)); testing.expect_value(t,encoded,text); delete(encoded)
    }
    for text in ([]string{"","-1","+1","1.0","1e3","18446744073709551616"," 1"}) { _,ok:=parse_entity_id(text); testing.expect(t,!ok) }
    cases:=[]struct { name,args:string, error:Call_Error }{
        {"unknown","{}",.Unknown_Tool}, {"spawn_entity","{",.Invalid_JSON},
        {"spawn_entity","[]",.Invalid_Arguments}, {"spawn_entity",`{"shape":"unsupported"}`,.Invalid_Arguments},
        {"spawn_entity",`{"position":[1,2]}`,.Invalid_Arguments}, {"spawn_entity",`{"scale":[1,"2",3]}`,.Invalid_Arguments},
        {"destroy_entity",`{"entity_id":9007199254740993}`,.Invalid_Arguments},
        {"set_field",`{"entity_id":"0","component":"Position","field":"x"}`,.Invalid_Arguments},
        {"query_entities",`{"limit":"0"}`,.Invalid_Arguments}, {"query_entities",`{"limit":-1}`,.Invalid_Arguments},
    }
    h:editor.Agent_Harness; editor.agent_harness_init(&h); defer editor.agent_harness_destroy(&h)
    for c in cases {
        ticket,err:=submit_call(&h,{"test",c.name,transmute([]byte)c.args})
        testing.expect_value(t,err,c.error); testing.expect_value(t,ticket,u64(0))
    }
    testing.expect_value(t,len(h.requests),0); testing.expect_value(t,len(h.session.actions),0)
}
@(test)
test_observation_stale_selection :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:editor.Component_Registry; editor.editor_registry_init(&reg); defer editor.editor_registry_destroy(&reg)
    editor.editor_register(&w,&reg,"Position",Position{})
    id:=ecs.spawn(&w,struct { position:Position }{Position{1,2,3,1,1,1}})
    snapshot:=scene_context(&w,&reg,id,true); defer scene_context_destroy(&snapshot)
    testing.expect(t,snapshot.entity_count==1 && snapshot.has_selection && len(snapshot.components)==1 && snapshot.counts[0].count==1)
    ecs.destroy_entity(&w,id)
    ecs.spawn(&w,struct { position:Position }{Position{}})
    stale:=scene_context(&w,&reg,id,true); defer scene_context_destroy(&stale)
    testing.expect(t,!stale.has_selection && len(stale.components)==0 && stale.counts[0].count==1)
}
@(test)
test_rate_window_and_retry :: proc(t:^testing.T) {
    limiter:Rate_Limiter; rate_limiter_init(&limiter,100*time.Millisecond,2); defer rate_limiter_destroy(&limiter)
    decision,wait:=rate_admit(&limiter,0); testing.expect(t,decision==.Allowed && wait==0)
    decision,wait=rate_admit(&limiter,50*time.Millisecond); testing.expect(t,decision==.Wait && wait==50*time.Millisecond)
    testing.expect_value(t,len(limiter.timestamps),1)
    decision,wait=rate_admit(&limiter,100*time.Millisecond); testing.expect(t,decision==.Allowed)
    decision,wait=rate_admit(&limiter,200*time.Millisecond); testing.expect(t,decision==.Exceeded && wait==time.Minute-200*time.Millisecond)
    decision,wait=rate_admit(&limiter,time.Minute); testing.expect(t,decision==.Allowed)
    decision,wait=rate_admit(&limiter,0); testing.expect(t,decision==.Invalid_Clock)
}
Rate_Worker :: struct { limiter:^Rate_Limiter, allowed:int, mutex:^sync.Mutex }
rate_worker :: proc(th:^thread.Thread) {
    worker:=cast(^Rate_Worker)th.data
    for _ in 0..<32 {
        decision,_:=rate_admit(worker.limiter,0)
        if decision==.Allowed { sync.mutex_lock(worker.mutex); worker.allowed+=1; sync.mutex_unlock(worker.mutex) }
    }
}
@(test)
test_rate_concurrent_admission :: proc(t:^testing.T) {
    limiter:Rate_Limiter; rate_limiter_init(&limiter,0,7); defer rate_limiter_destroy(&limiter)
    mutex:sync.Mutex
    workers:[4]Rate_Worker
    threads:[4]^thread.Thread
    for &worker,i in workers {
        worker={limiter=&limiter,mutex=&mutex}
        threads[i]=thread.create(rate_worker); threads[i].data=&worker; thread.start(threads[i])
    }
    total:=0
    for th in threads { thread.join(th); thread.destroy(th) }
    for worker in workers { total+=worker.allowed }
    testing.expect_value(t,total,7)
}

@(test)
test_submission_reports_mailbox_rejection_and_retains_call_identity :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:editor.Component_Registry; editor.editor_registry_init(&reg); defer editor.editor_registry_destroy(&reg)
    h:editor.Agent_Harness; editor.agent_harness_init(&h,capacity=1); defer editor.agent_harness_destroy(&h)
    call:=Tool_Call{"cancelled","spawn_entity",transmute([]byte)string(`{}`)}
    ticket,err:=submit_call(&h,call); testing.expect(t,ticket>0 && err==.None)
    zero,full:=submit_call(&h,call); testing.expect(t,zero==0 && full==.Mailbox_Full)
    testing.expect(t,editor.agent_cancel(&h,ticket))
    call.id="query-α"
    call.name="query_entities"
    next,accepted:=submit_call(&h,call); testing.expect(t,next>ticket && accepted==.None)
    editor.agent_finish(&h)
    closed_ticket,closed:=submit_call(&h,call); testing.expect(t,closed_ticket==0 && closed==.Mailbox_Closed)
    testing.expect_value(t,editor.agent_tick(&h,&w,&reg),1)
    response,ok:=editor.agent_take_result(&h); defer editor.agent_response_destroy(&response)
    testing.expect(t,ok && response.ticket==next && response.call_id=="query-α" && response.result.error==.None && w.live_count==0)
}

@(test)
test_submission_reports_identifier_exhaustion :: proc(t:^testing.T) {
    h:editor.Agent_Harness; editor.agent_harness_init(&h); defer editor.agent_harness_destroy(&h)
    h.next_ticket=max(u64)
    ticket,err:=submit_call(&h,{"exhausted","spawn_entity",transmute([]byte)string(`{}`)})
    testing.expect(t,ticket==0 && err==.Identifier_Exhausted && len(h.requests)==0 && len(h.session.actions)==0)
}
