//! Native HTTP/TLS streaming has bounded bodies, explicit deadlines and atomic cancellation.
package llm

import agent ".."
import curl "vendor:curl"
import "core:c/libc"
import "core:mem"
import "core:strings"
import "core:fmt"
import "core:sync"
import "core:time"
import "base:runtime"

/// A shared cancellation flag; cancellation does not undo accepted scene operations.
Cancel :: struct { flag:u32 }
/// May be called on another thread while a request is in progress.
cancel :: proc(c:^Cancel) { sync.atomic_store(&c.flag,1) }
/// Reads cancellation without borrowing any scene state.
is_cancelled :: proc(c:^Cancel)->bool { return c!=nil && sync.atomic_load(&c.flag)!=0 }
/// Owns curl initialization and shared provider admission; join all users before destruction.
Runtime :: struct { limiter:agent.Rate_Limiter, admission_mutex:sync.Mutex, start:time.Tick, allocator:mem.Allocator, initialized:bool }
/// Initializes provider transport before starting worker threads.
runtime_init :: proc(r:^Runtime,c:Config,allocator:=context.allocator)->Error {
    if err:=config_validate(c); err!=.None { return err }
    if curl.global_init(curl.GLOBAL_DEFAULT)!=.E_OK { return .Network }
    r.initialized=true; r.allocator=allocator; r.start=time.tick_now()
    agent.rate_limiter_init(&r.limiter,time.Duration(c.interval_ms)*time.Millisecond,int(c.max_calls),allocator)
    return .None
}
/// Releases native/global and limiter ownership after all caller threads have joined.
runtime_destroy :: proc(r:^Runtime) {
    if r.initialized { agent.rate_limiter_destroy(&r.limiter); curl.global_cleanup() }; r^={}
}
@(private="package")
Transfer :: struct { stream:^Stream, handle:^curl.CURL, cancellation:^Cancel, allocator:mem.Allocator, bytes:int, content_type:bool, error:Error }
@(private="package")
write_callback :: proc "c" (data:[^]u8,size,count:libc.size_t,state:rawptr)->libc.size_t {
    t:=cast(^Transfer)state; context=runtime.default_context(); context.allocator=t.allocator
    if size!=0 && count>libc.size_t(MAX_RESPONSE_BYTES)/size { t.error=.Limit; return 0 }
    n:=int(size*count)
    if t.bytes+n>MAX_RESPONSE_BYTES { t.error=.Limit; return 0 }; t.bytes+=n
    if is_cancelled(t.cancellation) { t.error=.Cancelled; return 0 }
    status:libc.long
    if curl.easy_getinfo(t.handle,.RESPONSE_CODE,&status)!=.E_OK { t.error=.Network; return 0 }
    if status!=200 { return libc.size_t(n) }
    if !t.content_type { t.error=.Protocol; return 0 }
    if err:=stream_feed(t.stream,data[:n]); err!=.None { t.error=err; return 0 }
    return libc.size_t(n)
}
@(private="package")
header_callback :: proc "c" (data:[^]u8,size,count:libc.size_t,state:rawptr)->libc.size_t {
    t:=cast(^Transfer)state; context=runtime.default_context(); context.allocator=t.allocator
    if size!=0 && count>libc.size_t(65536)/size { t.error=.Limit; return 0 }
    n:=int(size*count); line:=string(data[:n])
    if strings.has_prefix(line,"HTTP/") { t.content_type=false }
    if colon:=strings.index_byte(line,':'); colon>=0 && strings.equal_fold(line[:colon],"content-type") {
        value:=strings.trim_space(line[strings.index_byte(line,':')+1:])
        t.content_type=strings.has_prefix(value,"text/event-stream") && (len(value)==17 || value[17]==';')
    }
    return libc.size_t(n)
}
@(private="package")
progress_callback :: proc "c" (state:rawptr,total_down,current_down,total_up,current_up:curl.off_t)->libc.int {
    t:=cast(^Transfer)state; context=runtime.default_context()
    return 1 if is_cancelled(t.cancellation) else 0
}
/// Makes one actual streaming POST; provider errors are typed and response bodies are never diagnostic output.
complete :: proc(r:^Runtime,c:Config,body:string,cancellation:^Cancel=nil,observer:=Text_Observer{})->(Response,Error) {
    if !r.initialized || config_validate(c)!=.None { return {},.Config }
    if r.limiter.interval!=time.Duration(c.interval_ms)*time.Millisecond || r.limiter.maximum!=int(c.max_calls) { return {},.Config }
    if c.provider==.Disabled { return {},.Disabled }
    if len(body)>MAX_RESPONSE_BYTES { return {},.Limit }
    if is_cancelled(cancellation) { return {},.Cancelled }
    context.allocator=r.allocator
    key,err:=config_resolve_key(c); if err!=.None { return {},err }; defer delete(key,c.allocator)
    deadline:=time.tick_now()
    for {
        if is_cancelled(cancellation) { return {},.Cancelled }
        if time.tick_since(deadline)>=time.Duration(c.timeout_ms)*time.Millisecond { return {},.Timeout }
        sync.mutex_lock(&r.admission_mutex)
        decision,wait:=agent.rate_admit(&r.limiter,time.tick_since(r.start))
        sync.mutex_unlock(&r.admission_mutex)
        switch decision {
        case .Allowed: break
        case .Exceeded: return {},.Rate_Limited
        case .Invalid_Clock: return {},.Config
        case .Wait: time.sleep(min(wait,10*time.Millisecond)); continue
        }
        break
    }
    endpoint:=c.base_url; if c.provider==.OpenAI { endpoint="https://api.openai.com/v1" }
    suffix:="/responses" if c.api==.Responses else "/chat/completions"
    url:=fmt.aprintf("%s%s",strings.trim_right(endpoint,"/"),suffix,allocator=r.allocator); defer delete(url,r.allocator)
    url_c:=strings.clone_to_cstring(url,r.allocator); defer delete(url_c,r.allocator)
    auth:=fmt.aprintf("Authorization: Bearer %s",key,allocator=r.allocator); defer delete(auth,r.allocator)
    auth_c:=strings.clone_to_cstring(auth,r.allocator); defer delete(auth_c,r.allocator)
    handle:=curl.easy_init(); if handle==nil { return {},.Network }; defer curl.easy_cleanup(handle)
    headers:^curl.slist
    for value in ([3]cstring{auth_c,"Content-Type: application/json","Accept: text/event-stream"}) {
        next:=curl.slist_append(headers,value); if next==nil { if headers!=nil { curl.slist_free_all(headers) }; return {},.Network }; headers=next
    }
    defer curl.slist_free_all(headers)
    stream:Stream; stream_init(&stream,c.api,observer,r.allocator); defer stream_destroy(&stream)
    transfer:=Transfer{stream=&stream,handle=handle,cancellation=cancellation,allocator=r.allocator}
    remaining:=max(time.Duration(c.timeout_ms)*time.Millisecond-time.tick_since(deadline),time.Millisecond)
    options:=[19]curl.code{
        curl.easy_setopt(handle,.URL,url_c),
        curl.easy_setopt(handle,.HTTPHEADER,headers),
        curl.easy_setopt(handle,.POST,libc.long(1)),
        curl.easy_setopt(handle,.POSTFIELDS,raw_data(body)),
        curl.easy_setopt(handle,.POSTFIELDSIZE,libc.long(len(body))),
        curl.easy_setopt(handle,.WRITEFUNCTION,write_callback),
        curl.easy_setopt(handle,.WRITEDATA,&transfer),
        curl.easy_setopt(handle,.HEADERFUNCTION,header_callback),
        curl.easy_setopt(handle,.HEADERDATA,&transfer),
        curl.easy_setopt(handle,.XFERINFOFUNCTION,progress_callback),
        curl.easy_setopt(handle,.XFERINFODATA,&transfer),
        curl.easy_setopt(handle,.NOPROGRESS,libc.long(0)),
        curl.easy_setopt(handle,.NOSIGNAL,libc.long(1)),
        curl.easy_setopt(handle,.TIMEOUT_MS,libc.long(remaining/time.Millisecond)),
        curl.easy_setopt(handle,.CONNECTTIMEOUT_MS,libc.long(min(remaining/time.Millisecond,10000))),
        curl.easy_setopt(handle,.FOLLOWLOCATION,libc.long(0)),
        curl.easy_setopt(handle,.SSL_VERIFYPEER,libc.long(1)),
        curl.easy_setopt(handle,.SSL_VERIFYHOST,libc.long(2)),
        curl.easy_setopt(handle,.PROTOCOLS_STR,cstring("http,https")),
    }
    for code in options { if code!=.E_OK { return {},.Network } }
    result:=curl.easy_perform(handle)
    if is_cancelled(cancellation) { return {},.Cancelled }
    if transfer.error!=.None { return {},transfer.error }
    if result==.E_OPERATION_TIMEDOUT { return {},.Timeout }
    if result!=.E_OK { return {},.Network }
    status:libc.long; if curl.easy_getinfo(handle,.RESPONSE_CODE,&status)!=.E_OK { return {},.Network }
    if status==429 { return {},.Rate_Limited }; if status!=200 { return {},.HTTP }
    return stream_finish(&stream)
}
