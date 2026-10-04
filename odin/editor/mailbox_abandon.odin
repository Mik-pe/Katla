//! Individual reply ownership may end while the scene owner completes an accepted action.
package editor

import "core:sync"

/// Releases a caller's ticket without closing the shared mailbox or undoing an executed action.
/// Queued work is removed; executing work retains its credit until completion suppresses its reply.
agent_abandon :: proc(h:^Agent_Harness,ticket:u64)->bool {
    sync.mutex_lock(&h.mutex); defer sync.mutex_unlock(&h.mutex)
    if ticket==0 { return false }
    for &request,index in h.requests {
        if request.ticket!=ticket { continue }
        agent_request_destroy(&request,h.allocator); ordered_remove(&h.requests,index)
        h.outstanding-=1
        return true
    }
    for &response,index in h.responses {
        if response.ticket!=ticket { continue }
        agent_response_destroy(&response); ordered_remove(&h.responses,index)
        h.outstanding-=1
        return true
    }
    if h.executing_ticket==ticket && !h.executing_abandoned {
        h.executing_abandoned=true
        return true
    }
    return false
}
