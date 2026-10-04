//! One explicit existing-conversation connection; the bridge never owns a scene or host daemon.
package host

import "core:thread"
import "core:sync"
import "core:mem"
import "core:strings"

MAX_TEXT_BYTES :: 1<<20
MAX_IMAGE_BYTES :: 24<<20
MAX_QUEUE_BYTES :: 32<<20
MAX_EVENTS :: 256
MAX_EVENT_BYTES :: 4<<20

Config :: struct { socket,thread_id:string }
Error :: enum { None, Invalid_Config, Unsupported, Closed, Full, Limit, Transport, Protocol, Timeout }
Event_Kind :: enum { Connected, Accepted, Text, Finished, Attention, Error, Disconnected }
/// String fields are transferred by bridge_poll and released by event_destroy.
Event :: struct { kind:Event_Kind, name,turn_id,item_id,text,status:string, allocator:mem.Allocator }
@(private="package")
Command :: struct { text,metadata,png,turn_id:string, interrupt:bool }
/// Keep this owner stationary until bridge_destroy has joined the transport worker.
Bridge :: struct {
    config:Config,
    worker:^thread.Thread,
    mutex:sync.Mutex,
    commands:[dynamic]Command,
    events:[dynamic]Event,
    command_bytes,event_bytes:int,
    connected,stop:bool,
    accepted_turn:string,
    terminal:Event,
    has_terminal:bool,
    allocator:mem.Allocator,
}

/// Connects through the supplied private app-server socket to one already loaded thread.
bridge_connect :: proc(b:^Bridge,c:Config,allocator:=context.allocator)->Error {
    if b.worker!=nil { return .Invalid_Config }
    if err:=config_validate(c); err!=.None { return err }
    b.allocator=allocator; b.config={strings.clone(c.socket,allocator),strings.clone(c.thread_id,allocator)}
    b.commands=make([dynamic]Command,allocator); b.events=make([dynamic]Event,allocator)
    b.worker=thread.create(bridge_worker); b.worker.data=b; thread.start(b.worker)
    return .None
}
/// Clones a bounded question and committed viewport payload; callers retain all input ownership.
bridge_submit :: proc(b:^Bridge,text,metadata_json,png_base64:string)->Error {
    if !bridge_connected(b) { return .Closed }
    if len(text)==0 || len(text)>MAX_TEXT_BYTES || len(metadata_json)>MAX_TEXT_BYTES || len(png_base64)==0 || len(png_base64)>MAX_IMAGE_BYTES { return .Limit }
    if !question_valid(text,metadata_json,png_base64,b.allocator) { return .Protocol }
    sync.mutex_lock(&b.mutex); defer sync.mutex_unlock(&b.mutex)
    if !b.connected || b.stop { return .Closed }
    size:=len(text)+len(metadata_json)+len(png_base64)
    if len(b.commands)>=4 || b.command_bytes+size>MAX_QUEUE_BYTES { return .Full }
    append(&b.commands,Command{text=strings.clone(text,b.allocator),metadata=strings.clone(metadata_json,b.allocator),png=strings.clone(png_base64,b.allocator)})
    b.command_bytes+=size
    return .None
}
/// Explicit cancellation interrupts only the named accepted turn in this selected thread.
/// Disconnecting or destroying the bridge never interrupts external work.
bridge_cancel :: proc(b:^Bridge,turn_id:string)->Error {
    if len(turn_id)==0 || len(turn_id)>4096 { return .Invalid_Config }
    sync.mutex_lock(&b.mutex); defer sync.mutex_unlock(&b.mutex)
    if !b.connected || b.stop { return .Closed }
    if turn_id!=b.accepted_turn { return .Invalid_Config }
    if len(b.commands)>=4 { return .Full }
    append(&b.commands,Command{turn_id=strings.clone(turn_id,b.allocator),interrupt=true}); b.command_bytes+=len(turn_id)
    return .None
}
bridge_connected :: proc(b:^Bridge)->bool {
    sync.mutex_lock(&b.mutex); defer sync.mutex_unlock(&b.mutex)
    return b.connected && !b.stop
}
/// Transfers one event; turn/item correlation is preserved even when notifications precede acceptance.
bridge_poll :: proc(b:^Bridge)->(Event,bool) {
    sync.mutex_lock(&b.mutex); defer sync.mutex_unlock(&b.mutex)
    if len(b.events)>0 {
        e:=b.events[0]; ordered_remove(&b.events,0); b.event_bytes-=event_size(e); return e,true
    }
    if b.has_terminal { e:=b.terminal; b.terminal={}; b.has_terminal=false; return e,true }
    return {},false
}
event_destroy :: proc(e:^Event) {
    for text in ([5]string{e.name,e.turn_id,e.item_id,e.text,e.status}) { delete(text,e.allocator) }
    e^={}
}
/// Cancels unsent questions, joins the worker and closes only its owned stream.
bridge_destroy :: proc(b:^Bridge) {
    sync.mutex_lock(&b.mutex); b.stop=true; b.connected=false; sync.mutex_unlock(&b.mutex)
    if b.worker!=nil { thread.join(b.worker); thread.destroy(b.worker) }
    for &command in b.commands { command_destroy(&command,b.allocator) }
    for &event in b.events { event_destroy(&event) }
    event_destroy(&b.terminal); delete(b.commands); delete(b.events)
    delete(b.accepted_turn,b.allocator); delete(b.config.socket,b.allocator); delete(b.config.thread_id,b.allocator); b^={}
}
@(private="package")
command_destroy :: proc(c:^Command,a:mem.Allocator) {
    for text in ([4]string{c.text,c.metadata,c.png,c.turn_id}) { delete(text,a) }; c^={}
}
@(private="package")
event_size :: proc(e:Event)->int { return len(e.name)+len(e.turn_id)+len(e.item_id)+len(e.text)+len(e.status) }
@(private="package")
emit :: proc(b:^Bridge,e:Event)->bool {
    sync.mutex_lock(&b.mutex); defer sync.mutex_unlock(&b.mutex)
    if b.stop { return false }
    if len(b.events)>=MAX_EVENTS || b.event_bytes+event_size(e)>MAX_EVENT_BYTES {
        terminal_locked(b,"Host progress queue exceeded its bound; reconnect explicitly.")
        return false
    }
    copy:=e; copy.allocator=b.allocator
    copy.name=strings.clone(e.name,b.allocator); copy.turn_id=strings.clone(e.turn_id,b.allocator)
    copy.item_id=strings.clone(e.item_id,b.allocator); copy.text=strings.clone(e.text,b.allocator); copy.status=strings.clone(e.status,b.allocator)
    append(&b.events,copy); b.event_bytes+=event_size(copy)
    if e.kind==.Connected { b.connected=true }
    if e.kind==.Accepted { delete(b.accepted_turn,b.allocator); b.accepted_turn=strings.clone(e.turn_id,b.allocator) }
    return true
}
@(private="package")
terminal_locked :: proc(b:^Bridge,text:string) {
    b.stop=true; b.connected=false
    if !b.has_terminal { b.terminal={kind=.Disconnected,text=strings.clone(text,b.allocator),allocator=b.allocator}; b.has_terminal=true }
}
@(private="package")
terminal :: proc(b:^Bridge,text:string) {
    sync.mutex_lock(&b.mutex); defer sync.mutex_unlock(&b.mutex)
    terminal_locked(b,text)
}
@(private="package")
stopped :: proc(b:^Bridge)->bool {
    sync.mutex_lock(&b.mutex); defer sync.mutex_unlock(&b.mutex); return b.stop
}
@(private="package")
pop_command :: proc(b:^Bridge)->(Command,bool) {
    sync.mutex_lock(&b.mutex); defer sync.mutex_unlock(&b.mutex)
    if len(b.commands)==0 || b.stop { return {},false }
    c:=b.commands[0]; ordered_remove(&b.commands,0); b.command_bytes-=len(c.text)+len(c.metadata)+len(c.png)+len(c.turn_id); return c,true
}
