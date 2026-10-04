//! Admission uses an explicit monotonic clock and records only accepted calls.
package agent

import "core:sync"
import "core:time"

/// Outcomes of one atomic admission attempt.
Rate_Decision :: enum { Allowed, Wait, Exceeded, Invalid_Clock }
/// Owns a rolling minute of timestamps; keep its address stable while shared.
Rate_Limiter :: struct {
    mutex:sync.Mutex,
    interval:time.Duration,
    maximum:int,
    timestamps:[dynamic]time.Duration,
    last_clock:time.Duration,
    has_clock:bool,
}
/// Initializes a limiter with capacity for its full rolling window.
rate_limiter_init :: proc(l:^Rate_Limiter,interval:time.Duration,maximum:int,allocator:=context.allocator) {
    l.interval=max(interval,0); l.maximum=max(maximum,1)
    l.timestamps=make([dynamic]time.Duration,0,l.maximum,allocator)
}
/// Destroy after joining all calling threads.
rate_limiter_destroy :: proc(l:^Rate_Limiter) { delete(l.timestamps); l^={} }
/// Delayed callers must retry admission; waiting does not reserve a future call.
rate_admit :: proc(l:^Rate_Limiter,now:time.Duration)->(Rate_Decision,time.Duration) {
    sync.mutex_lock(&l.mutex); defer sync.mutex_unlock(&l.mutex)
    if now<0 || (l.has_clock && now<l.last_clock) { return .Invalid_Clock,0 }
    l.last_clock=now; l.has_clock=true
    expired:=0
    for stamp in l.timestamps { if now-stamp>=time.Minute { expired+=1 } else { break } }
    if expired>0 {
        copy(l.timestamps[:],l.timestamps[expired:])
        resize(&l.timestamps,len(l.timestamps)-expired)
    }
    if len(l.timestamps)>=l.maximum { return .Exceeded,time.Minute-(now-l.timestamps[0]) }
    if len(l.timestamps)>0 {
        elapsed:=now-l.timestamps[len(l.timestamps)-1]
        if elapsed<l.interval { return .Wait,l.interval-elapsed }
    }
    append(&l.timestamps,now)
    return .Allowed,0
}
