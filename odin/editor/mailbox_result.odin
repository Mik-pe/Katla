//! Ticket-selective replies let independent transports share one scene-owner mailbox.
package editor

import "core:sync"

/// Transfers only this ticket's response and releases its reserved slot once.
/// A missing, executing, cancelled or already consumed ticket leaves other replies untouched.
agent_take_result_for :: proc(h:^Agent_Harness,ticket:u64)->(Agent_Response,bool) {
    if ticket==0 { return {},false }
    sync.mutex_lock(&h.mutex); defer sync.mutex_unlock(&h.mutex)
    for response,i in h.responses {
        if response.ticket==ticket {
            ordered_remove(&h.responses,i); h.outstanding-=1
            return response,true
        }
    }
    return {},false
}
