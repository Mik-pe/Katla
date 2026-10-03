//! Agent requests cross a mutex-protected mailbox; only tick mutates ecs.World.
package editor

import ecs "../ecs"
import "core:sync"
import "core:mem"
import "core:strings"




/// Owns a session action, its result and reversible snapshots.
Agent_Action :: struct { id:u64, operation:Scene_Op, result:Tool_Result, undo:Undo_Group }
/// Owns ordered action history and per-session undo state.
Agent_Session :: struct { actions:[dynamic]Agent_Action, next_id:u64, paused,finished:bool, allocator:mem.Allocator }
/// Owns an entity count and sorted available-component names.
Observation :: struct { entity_count:int, available_components:[dynamic]string }
/// Transfers an action ID and owned result to a background agent.
Agent_Response :: struct { id:u64, result:Tool_Result }
/// Owns a synchronized mailbox and caller-thread scene session.
Agent_Harness :: struct {
    session:Agent_Session,
    requests:[dynamic]Scene_Op,
    responses:[dynamic]Agent_Response,
    mutex:sync.Mutex,
    finished_requested:bool,
    allocator:mem.Allocator,
}
/// Initializes owned action history and session controls.
agent_session_init :: proc(s:^Agent_Session,allocator:=context.allocator) {
    s.allocator=allocator; s.actions=make([dynamic]Agent_Action,allocator)
}
/// Releases owned operations, results and scene snapshots.
agent_session_destroy :: proc(s:^Agent_Session) {
    for &action in s.actions {
        scene_op_destroy(&action.operation,s.allocator)
        tool_result_destroy(&action.result); undo_group_destroy(&action.undo)
    }
    delete(s.actions); s^={}
}
/// Undo preserves action IDs on failure and releases the popped action on success.
agent_undo_last :: proc(s:^Agent_Session,w:^ecs.World,reg:^Component_Registry)->Scene_Error {
    if len(s.actions)==0 { return .None }
    a:=&s.actions[len(s.actions)-1]
    err:=undo_group(w,reg,&a.undo)
    if err!=.None { return err }
    s.next_id=a.id
    scene_op_destroy(&a.operation,s.allocator)
    tool_result_destroy(&a.result); undo_group_destroy(&a.undo)
    resize(&s.actions,len(s.actions)-1)
    return .None
}
/// Restores session mutations in reverse action order.
agent_undo_all :: proc(s:^Agent_Session,w:^ecs.World,reg:^Component_Registry)->Scene_Error {
    for len(s.actions)>0 { err:=agent_undo_last(s,w,reg); if err!=.None { return err } }
    return .None
}
/// Returns an owned snapshot of entity count and available component names.
build_observation :: proc(w:^ecs.World,reg:^Component_Registry)->Observation {
    return Observation{w.live_count,editor_type_names(reg)}
}
/// Releases the observation's owned name array.
observation_destroy :: proc(o:^Observation) { delete(o.available_components); o^={} }
/// Records one caller-thread operation with an owned result and undo snapshot.
agent_execute :: proc(s:^Agent_Session,w:^ecs.World,reg:^Component_Registry,op:Scene_Op)->^Agent_Action {
    result,group:=scene_execute(w,reg,op)
    assert(s.next_id<max(u64))
    append(&s.actions,Agent_Action{s.next_id,scene_op_clone(op,s.allocator),result,group}); s.next_id+=1
    return &s.actions[len(s.actions)-1]
}
/// Synchronous agents observe, decide and receive each action result on the caller thread.
agent_run_sync :: proc(s:^Agent_Session,w:^ecs.World,reg:^Component_Registry,state:^$S,
                       decide:proc(^S,Observation)->(Scene_Op,bool),on_result:proc(^S,^Agent_Action)) {
    for !s.paused && !s.finished {
        obs:=build_observation(w,reg)
        op,has_op:=decide(state,obs); observation_destroy(&obs)
        if !has_op { s.finished=true; break }
        action:=agent_execute(s,w,reg,op); on_result(state,action)
    }
}
@(private="package")
scene_op_clone :: proc(op:Scene_Op,allocator:mem.Allocator)->Scene_Op {
    cloned:=op
    cloned.component=strings.clone(op.component,allocator); cloned.field=strings.clone(op.field,allocator)
    cloned.name=strings.clone(op.name,allocator); cloned.path=strings.clone(op.path,allocator)
    cloned.value=make([]byte,len(op.value),allocator); copy(cloned.value,op.value)
    return cloned
}
@(private="package")
scene_op_destroy :: proc(op:^Scene_Op,allocator:mem.Allocator) {
    delete(op.component,allocator); delete(op.field,allocator); delete(op.name,allocator)
    delete(op.path,allocator); delete(op.value,allocator); op^={}
}
@(private="package")
tool_result_clone :: proc(result:Tool_Result,allocator:mem.Allocator)->Tool_Result {
    cloned:=Tool_Result{allocator=allocator,error=result.error,entities=make([dynamic]ecs.Entity_Id,allocator),data=make([]byte,len(result.data),allocator)}
    append(&cloned.entities,..result.entities[:]); copy(cloned.data,result.data)
    return cloned
}
/// Initializes a stationary mailbox with a thread-safe allocator.
agent_harness_init :: proc(h:^Agent_Harness,allocator:=context.allocator) {
    h.allocator=allocator; agent_session_init(&h.session,allocator)
    h.requests=make([dynamic]Scene_Op,allocator); h.responses=make([dynamic]Agent_Response,allocator)
}
/// Join the producer before destruction; queued requests and responses are owned here.
agent_harness_destroy :: proc(h:^Agent_Harness) {
    for &op in h.requests { scene_op_destroy(&op,h.allocator) }
    for &response in h.responses { tool_result_destroy(&response.result) }
    agent_session_destroy(&h.session); delete(h.requests); delete(h.responses); h^={}
}
/// May run on a background thread; borrowed operation data is cloned before return.
agent_submit :: proc(h:^Agent_Harness,op:Scene_Op) {
    sync.mutex_lock(&h.mutex); defer sync.mutex_unlock(&h.mutex)
    assert(!h.finished_requested,"agent already finished")
    append(&h.requests,scene_op_clone(op,h.allocator))
}
/// Marks a producer finished after its already queued requests are processed.
agent_finish :: proc(h:^Agent_Harness) {
    sync.mutex_lock(&h.mutex); defer sync.mutex_unlock(&h.mutex)
    h.finished_requested=true
}
/// Returns ownership of the next result to the background agent.
agent_take_result :: proc(h:^Agent_Harness)->(Agent_Response,bool) {
    sync.mutex_lock(&h.mutex); defer sync.mutex_unlock(&h.mutex)
    if len(h.responses)==0 { return {},false }
    result:=h.responses[0]; ordered_remove(&h.responses,0)
    return result,true
}
/// Executes at most ten queued operations; pause preserves requests for a later tick.
agent_tick :: proc(h:^Agent_Harness,w:^ecs.World,reg:^Component_Registry)->int {
    if h.session.paused || h.session.finished { return 0 }
    processed:=0
    for processed<10 {
        sync.mutex_lock(&h.mutex)
        if len(h.requests)==0 {
            if h.finished_requested { h.session.finished=true }
            sync.mutex_unlock(&h.mutex); break
        }
        op:=h.requests[0]; ordered_remove(&h.requests,0)
        sync.mutex_unlock(&h.mutex)
        action:=agent_execute(&h.session,w,reg,op)
        scene_op_destroy(&op,h.allocator)
        response:=tool_result_clone(action.result,h.allocator)
        sync.mutex_lock(&h.mutex); append(&h.responses,Agent_Response{action.id,response}); sync.mutex_unlock(&h.mutex)
        processed+=1
    }
    return processed
}
