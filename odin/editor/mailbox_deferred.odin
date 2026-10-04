//! Deferred application replies retain mailbox credit until a committed owner result exists.
package editor

import "core:sync"

/// Called only by the scene owner. Consumes result and undo on success, preserving caller correlation.
/// Rejected or abandoned tickets leave both owners untouched for caller cleanup.
agent_complete :: proc(h:^Agent_Harness,ticket:u64,result:^Tool_Result,undo:^Undo_Group)->bool {
    sync.mutex_lock(&h.mutex); defer sync.mutex_unlock(&h.mutex)
    for &request,index in h.deferred {
        if request.ticket!=ticket { continue }
        action:=agent_record_action(&h.session,request.operation,result,undo)
        response:=Agent_Response{id=action.id,ticket=ticket,call_id=request.call_id,
            result=tool_result_clone(action.result,h.allocator),allocator=h.allocator}
        request.call_id=""
        agent_request_destroy(&request,h.allocator); ordered_remove(&h.deferred,index)
        append(&h.responses,response)
        return true
    }
    return false
}

/// Reports whether an accepted deferred request still belongs to a transport.
agent_is_deferred :: proc(h:^Agent_Harness,ticket:u64)->bool {
    sync.mutex_lock(&h.mutex); defer sync.mutex_unlock(&h.mutex)
    for request in h.deferred { if request.ticket==ticket { return true } }
    return false
}
