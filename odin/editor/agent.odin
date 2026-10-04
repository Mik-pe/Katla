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
/// Application tools execute only on the scene owner, returning the shared undo command.
Application_Executor :: struct { state:rawptr, execute:proc(rawptr,^ecs.World,^Component_Registry,Scene_Op)->(Tool_Result,Undo_Group) }
/// Owns an entity count and sorted available-component names.
Observation :: struct { entity_count:int, available_components:[dynamic]string }
/// Distinguishes rejected submission from accepted work without touching the scene.
Mailbox_Error :: enum { None, Full, Closed, Identifier_Exhausted }
/// Owns one accepted operation and its caller correlation string.
@(private="package")
Agent_Request :: struct { ticket:u64, call_id:string, operation:Scene_Op }
/// Transfers request correlation, action ID and an owned result to the caller.
Agent_Response :: struct { id,ticket:u64, call_id:string, result:Tool_Result, allocator:mem.Allocator }
/// Owns a synchronized mailbox and caller-thread scene session.
Agent_Harness :: struct {
    session:Agent_Session,
    requests:[dynamic]Agent_Request,
    responses:[dynamic]Agent_Response,
    mutex:sync.Mutex,
    finished_requested:bool,
    capacity,outstanding:int,
    next_ticket:u64,
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
    for remap in a.undo.remaps {
        for &prior in s.actions[:len(s.actions)-1] {
            undo_group_remap(&prior.undo,remap)
            if prior.operation.entity==remap.before { prior.operation.entity=remap.after }
            if prior.operation.has_parent && prior.operation.parent==remap.before { prior.operation.parent=remap.after }
        }
    }
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
agent_execute :: proc(s:^Agent_Session,w:^ecs.World,reg:^Component_Registry,op:Scene_Op,application:Application_Executor={})->^Agent_Action {
    assert(s.next_id<max(u64))
    result:Tool_Result; group:Undo_Group
    if application.execute!=nil { result,group=application.execute(application.state,w,reg,op) }
    else { result,group=scene_execute(w,reg,op) }
    append(&s.actions,Agent_Action{s.next_id,scene_op_clone(op,s.allocator),result,group}); s.next_id+=1
    return &s.actions[len(s.actions)-1]
}
/// Synchronous agents observe, decide and receive each action result on the caller thread.
agent_run_sync :: proc(s:^Agent_Session,w:^ecs.World,reg:^Component_Registry,state:^$S,
                       decide:proc(^S,Observation)->(Scene_Op,bool),on_result:proc(^S,^Agent_Action),application:Application_Executor={}) {
    for !s.paused && !s.finished {
        obs:=build_observation(w,reg)
        op,has_op:=decide(state,obs); observation_destroy(&obs)
        if !has_op { s.finished=true; break }
        action:=agent_execute(s,w,reg,op,application); on_result(state,action)
    }
}
@(private="package")
scene_op_clone :: proc(op:Scene_Op,allocator:mem.Allocator)->Scene_Op {
    cloned:=op
    cloned.component=strings.clone(op.component,allocator); cloned.field=strings.clone(op.field,allocator)
    cloned.name=strings.clone(op.name,allocator); cloned.path=strings.clone(op.path,allocator)
    cloned.tool_name=strings.clone(op.tool_name,allocator)
    cloned.value=make([]byte,len(op.value),allocator); copy(cloned.value,op.value)
    return cloned
}
@(private="package")
scene_op_destroy :: proc(op:^Scene_Op,allocator:mem.Allocator) {
    delete(op.component,allocator); delete(op.field,allocator); delete(op.name,allocator)
    delete(op.path,allocator); delete(op.tool_name,allocator); delete(op.value,allocator); op^={}
}
@(private="package")
tool_result_clone :: proc(result:Tool_Result,allocator:mem.Allocator)->Tool_Result {
    cloned:=Tool_Result{allocator=allocator,error=result.error,entities=make([dynamic]ecs.Entity_Id,allocator),data=make([]byte,len(result.data),allocator)}
    append(&cloned.entities,..result.entities[:]); copy(cloned.data,result.data)
    return cloned
}
/// Initializes a stationary mailbox; capacity includes queued, executing and unread replies.
/// Supply a thread-safe allocator and keep this owner stationary until callers have joined.
agent_harness_init :: proc(h:^Agent_Harness,allocator:=context.allocator,capacity:=256) {
    assert(capacity>0)
    h.allocator=allocator; h.capacity=capacity; h.next_ticket=1
    agent_session_init(&h.session,allocator)
    h.requests=make([dynamic]Agent_Request,allocator); h.responses=make([dynamic]Agent_Response,allocator)
}
@(private="package")
agent_request_destroy :: proc(request:^Agent_Request,allocator:mem.Allocator) {
    scene_op_destroy(&request.operation,allocator); delete(request.call_id,allocator); request^={}
}
/// Releases a transferred response using its captured allocator.
agent_response_destroy :: proc(response:^Agent_Response) {
    tool_result_destroy(&response.result); delete(response.call_id,response.allocator); response^={}
}
/// Join producers and consumers before destruction; queued requests and replies are owned here.
agent_harness_destroy :: proc(h:^Agent_Harness) {
    for &request in h.requests { agent_request_destroy(&request,h.allocator) }
    for &response in h.responses { agent_response_destroy(&response) }
    agent_session_destroy(&h.session); delete(h.requests); delete(h.responses); h^={}
}
/// Clones borrowed data on acceptance; rejection returns ticket zero and reserves no capacity.
agent_submit :: proc(h:^Agent_Harness,op:Scene_Op,call_id:="")->(u64,Mailbox_Error) {
    sync.mutex_lock(&h.mutex); defer sync.mutex_unlock(&h.mutex)
    if h.finished_requested { return 0,.Closed }
    if h.outstanding>=h.capacity { return 0,.Full }
    if h.next_ticket==max(u64) { return 0,.Identifier_Exhausted }
    ticket:=h.next_ticket
    append(&h.requests,Agent_Request{ticket,strings.clone(call_id,h.allocator),scene_op_clone(op,h.allocator)})
    h.next_ticket+=1; h.outstanding+=1
    return ticket,.None
}
/// Cancels queued work only; an executing or completed action is never rolled back here.
agent_cancel :: proc(h:^Agent_Harness,ticket:u64)->bool {
    sync.mutex_lock(&h.mutex); defer sync.mutex_unlock(&h.mutex)
    for &request,i in h.requests {
        if request.ticket==ticket {
            agent_request_destroy(&request,h.allocator); ordered_remove(&h.requests,i)
            h.outstanding-=1
            return true
        }
    }
    return false
}
/// Closes admission; the owner drains accepted requests unless explicitly cancelled.
agent_finish :: proc(h:^Agent_Harness) {
    sync.mutex_lock(&h.mutex); defer sync.mutex_unlock(&h.mutex)
    h.finished_requested=true
}
/// Transfers the next response and frees its reserved mailbox slot.
agent_take_result :: proc(h:^Agent_Harness)->(Agent_Response,bool) {
    sync.mutex_lock(&h.mutex); defer sync.mutex_unlock(&h.mutex)
    if len(h.responses)==0 { return {},false }
    result:=h.responses[0]; ordered_remove(&h.responses,0); h.outstanding-=1
    return result,true
}
/// Executes at most ten queued operations; pause preserves requests for a later owner tick.
agent_tick :: proc(h:^Agent_Harness,w:^ecs.World,reg:^Component_Registry,application:Application_Executor={})->int {
    if h.session.paused || h.session.finished { return 0 }
    processed:=0
    for processed<10 {
        sync.mutex_lock(&h.mutex)
        if len(h.requests)==0 {
            if h.finished_requested { h.session.finished=true }
            sync.mutex_unlock(&h.mutex); break
        }
        request:=h.requests[0]; ordered_remove(&h.requests,0)
        sync.mutex_unlock(&h.mutex)
        action:=agent_execute(&h.session,w,reg,request.operation,application)
        scene_op_destroy(&request.operation,h.allocator)
        response:=Agent_Response{id=action.id,ticket=request.ticket,call_id=request.call_id,
                                 result=tool_result_clone(action.result,h.allocator),allocator=h.allocator}
        sync.mutex_lock(&h.mutex); append(&h.responses,response); sync.mutex_unlock(&h.mutex)
        processed+=1
    }
    return processed
}
