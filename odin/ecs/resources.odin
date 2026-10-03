//! Owned global resources and typed event logs with independent reader cursors.
package ecs

import "core:mem"
import "base:intrinsics"

@(private="package")
Resource_Entry :: struct { data: rawptr, value_type: typeid, ops: Value_Ops }
@(private="package")
resource_delete :: proc(r: ^Resource_Entry, allocator: mem.Allocator) {
    context.allocator=allocator
    if r.ops.destroy!=nil { r.ops.destroy(r.data) }
    mem.free(r.data,allocator); free(r,allocator)
}
/// Transfers resource ownership and destroys any replaced value.
insert_resource :: proc(w: ^World, value: $T, ops := Value_Ops{}) {
    assert(!w.frozen)
    if old:=w.resources[T]; old!=nil { resource_delete(old,w.allocator) }
    r:=new(Resource_Entry,w.allocator)
    r^=Resource_Entry{allocate(size_of(T),align_of(T),w.allocator),T,ops}
    owned_value:=value
    mem.copy(r.data,rawptr(&owned_value),size_of(T))
    w.resources[T]=r
}
/// Returns a shallow copy of a global resource when present.
get_resource :: proc(w: ^World, $T:typeid) -> (value:T, ok:bool) {
    assert(!w.frozen)
    if r:=w.resources[T]; r!=nil { return (cast(^T)r.data)^,true }
    return
}
/// Borrows a global resource until removal or replacement.
get_resource_mut :: proc(w: ^World, $T:typeid) -> ^T {
    assert(!w.frozen)
    if r:=w.resources[T]; r!=nil { return cast(^T)r.data }
    return nil
}
/// Destroys a global resource and reports whether it was present.
remove_resource :: proc(w: ^World, $T:typeid) -> bool {
    assert(!w.frozen)
    r:=w.resources[T]
    if r==nil { return false }
    resource_delete(r,w.allocator); delete_key(&w.resources,T)
    return true
}

@(private="package")
next_log_identity: u64 = 1
@(private="package")
Event_Log :: struct {
    store: ^Store,
    identity, first_sequence: u64,
}
/// Tracks event-log identity and the next unread sequence.
Event_Cursor :: struct { identity, next_sequence: u64 }
/// Owns a typed event log outside World when initialized explicitly.
Events :: struct($E:typeid) { log: ^Event_Log }
/// Creates a caller-owned log. World event resources use ensure_event_log instead.
events_init :: proc(events: ^Events($E), allocator := context.allocator, ops := Value_Ops{}) {
    events.log=event_log_new(E,allocator,ops)
}
/// Releases all retained event values and the caller-owned log.
events_destroy :: proc(events: ^Events($E)) {
    if events.log!=nil { event_log_delete(events.log,events.log.store.allocator); events.log=nil }
}
@(private="package")
event_log_new :: proc(T:typeid, allocator:mem.Allocator, ops:=Value_Ops{}) -> ^Event_Log {
    identity:=intrinsics.atomic_add(&next_log_identity,1)
    assert(identity<max(u64))
    log:=new(Event_Log,allocator)
    log^=Event_Log{store_new(T,allocator,ops),identity,0}
    return log
}
@(private="package")
event_log_delete :: proc(log:^Event_Log, allocator:mem.Allocator) {
    store_delete(log.store); free(log,allocator)
}
@(private="package")
ensure_event_log :: proc(w:^World,T:typeid) -> ^Event_Log {
    log:=w.event_logs[T]
    if log==nil { log=event_log_new(T,w.allocator); w.event_logs[T]=log }
    return log
}
/// Replacing a log resets existing readers by assigning a new identity.
replace_events :: proc(w:^World,$E:typeid) {
    assert(!w.frozen)
    ops:=Value_Ops{}
    if old:=w.event_logs[E]; old!=nil { ops=old.store.ops; event_log_delete(old,w.allocator) }
    w.event_logs[E]=event_log_new(E,w.allocator,ops)
}
/// Clears a World event log while preserving independent reader positions.
clear_events :: proc(w:^World,$E:typeid) {
    assert(!w.frozen)
    if log:=w.event_logs[E]; log!=nil { event_log_clear(log) }
}
@(private="package")
event_log_clear :: proc(log:^Event_Log) {
    assert(log.first_sequence<=max(u64)-u64(len(log.store.entities)))
    log.first_sequence+=u64(len(log.store.entities))
    store_clear(log.store)
}
@(private="package")
event_send_raw :: proc(log:^Event_Log, value:rawptr) {
    assert(u64(len(log.store.entities))<0xffffffff)
    store_insert(log.store,Entity_Id(len(log.store.entities)),value)
}
/// Transfers an event to a caller-owned log.
event_send :: proc(events:^Events($E),value:E) { owned_value:=value; event_send_raw(events.log,rawptr(&owned_value)) }
/// Releases retained events without resetting sequence numbers.
events_clear :: proc(events:^Events($E)) { event_log_clear(events.log) }
/// Returns a borrowed slice. Read it before the next send, clear or replacement.
events_read :: proc(events:^Events($E),cursor:^Event_Cursor) -> []E {
    log:=events.log
    if cursor.identity!=log.identity { cursor^=Event_Cursor{log.identity,log.first_sequence} }
    count:=u64(len(log.store.entities))
    start:=min(count,cursor.next_sequence-max(min(cursor.next_sequence,log.first_sequence),u64(0)))
    assert(log.first_sequence<=max(u64)-count)
    cursor.next_sequence=log.first_sequence+count
    return (cast([^]E)log.store.data)[int(start):int(count)]
}

/// Tests whether a global resource type is currently registered.
contains_resource :: proc(w:^World,$T:typeid)->bool { assert(!w.frozen); return w.resources[T]!=nil }

/// Transfers resource ownership to the caller instead of destroying its value.
take_resource :: proc(w:^World,$T:typeid)->(value:T,ok:bool) {
    assert(!w.frozen)
    r:=w.resources[T]; if r==nil { return }
    value=(cast(^T)r.data)^
    mem.free(r.data,w.allocator); free(r,w.allocator); delete_key(&w.resources,T)
    return value,true
}

/// Calls initialize once if absent, then returns a caller-thread mutable borrow.
get_resource_or_insert :: proc(w:^World,$T:typeid,initialize:proc()->T,ops:=Value_Ops{})->^T {
    assert(!w.frozen)
    context.allocator=w.allocator
    if w.resources[T]==nil { insert_resource(w,initialize(),ops) }
    return get_resource_mut(w,T)
}

/// Registers event ownership hooks before event reader/writer system registration.
register_events :: proc(w:^World,$E:typeid,ops:=Value_Ops{}) {
    assert(!w.frozen && !w.execution_active && w.event_logs[E]==nil)
    w.event_logs[E]=event_log_new(E,w.allocator,ops)
}
