//! One joined worker owns a conversation turn and a bounded streaming progress queue.
package llm

import "core:thread"
import "core:sync"
import "core:mem"
import "core:strings"

/// A stationary async request owner; destroy joins its worker before releasing borrowed state.
Job :: struct {
    worker:^thread.Thread,
    conversation:^Conversation,
    cancellation:Cancel,
    mutex:sync.Mutex,
    prompt:string,
    chunks:[dynamic]string,
    capacity,bytes:int,
    done,overflow,taken:bool,
    response:Response,
    error:Error,
    allocator:mem.Allocator,
}
@(private="package")
job_text :: proc(state:rawptr,text:string) {
    job:=cast(^Job)state
    sync.mutex_lock(&job.mutex); defer sync.mutex_unlock(&job.mutex)
    if len(job.chunks)>=job.capacity || job.bytes+len(text)>MAX_TEXT_BYTES {
        job.overflow=true; cancel(&job.cancellation); return
    }
    append(&job.chunks,strings.clone(text,job.allocator)); job.bytes+=len(text)
}
@(private="package")
job_worker :: proc(th:^thread.Thread) {
    job:=cast(^Job)th.data
    response,err:=conversation_turn(job.conversation,job.prompt,&job.cancellation,{state=job,text=job_text})
    sync.mutex_lock(&job.mutex); defer sync.mutex_unlock(&job.mutex)
    job.response=response; job.error=.Limit if job.overflow else err; job.done=true
}
/// Starts one real provider producer; progress backpressure fails explicitly rather than dropping text.
job_start :: proc(job:^Job,conversation:^Conversation,prompt:string,capacity:=64,allocator:=context.allocator)->Error {
    if job.worker!=nil || conversation==nil || len(prompt)>MAX_TEXT_BYTES || capacity<1 || capacity>65536 { return .Config }
    job.allocator=allocator; job.conversation=conversation; job.capacity=capacity
    job.prompt=strings.clone(prompt,allocator); job.chunks=make([dynamic]string,0,capacity,allocator)
    job.worker=thread.create(job_worker); job.worker.data=job; thread.start(job.worker)
    return .None
}
/// Transfers one progress string; destroy it with the job's captured allocator.
job_poll_text :: proc(job:^Job)->(string,bool) {
    sync.mutex_lock(&job.mutex); defer sync.mutex_unlock(&job.mutex)
    if len(job.chunks)==0 { return "",false }
    value:=job.chunks[0]; copy(job.chunks[:],job.chunks[1:]); resize(&job.chunks,len(job.chunks)-1)
    job.bytes-=len(value); return value,true
}
/// Transfers the terminal response at most once; it remains independently owned after job destruction.
job_poll :: proc(job:^Job)->(Response,Error,bool) {
    sync.mutex_lock(&job.mutex); defer sync.mutex_unlock(&job.mutex)
    if !job.done || job.taken { return {},.None,false }
    job.taken=true; response:=job.response; job.response={}; return response,job.error,true
}
/// Cancels network or queued work; accepted scene operations remain in shared undo history.
job_cancel :: proc(job:^Job) { cancel(&job.cancellation) }
/// Cancels and joins the producer, then frees any unread progress and terminal response.
job_destroy :: proc(job:^Job) {
    if job.worker!=nil { job_cancel(job); thread.join(job.worker); thread.destroy(job.worker) }
    for chunk in job.chunks { delete(chunk,job.allocator) }
    delete(job.chunks); delete(job.prompt,job.allocator); response_destroy(&job.response); job^={}
}
