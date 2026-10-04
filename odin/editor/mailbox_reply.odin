//! Observation replies transfer owned payloads without retaining images in authored scene history.
package editor

import "core:sync"

/// Completes one deferred read/view ticket on the scene owner, consuming only its reply.
/// Already abandoned tickets leave the caller's result untouched. World mutation history uses agent_complete.
agent_complete_reply :: proc(h:^Agent_Harness,ticket:u64,result:^Tool_Result)->bool {
    if h==nil || result==nil { return false }
    sync.mutex_lock(&h.mutex); defer sync.mutex_unlock(&h.mutex)
    if h.session.next_id==max(u64) { return false }
    for &request,index in h.deferred {
        if request.ticket!=ticket { continue }
        response:=Agent_Response{id=h.session.next_id,ticket=ticket,call_id=request.call_id,result=result^,allocator=h.allocator}
        h.session.next_id+=1; request.call_id=""; result^={}
        agent_request_destroy(&request,h.allocator); ordered_remove(&h.deferred,index); append(&h.responses,response)
        return true
    }
    return false
}
