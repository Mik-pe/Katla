//! System parameters derive every component/resource access claim from their type.
package ecs

import "core:mem"

@(private="package")
Res_Marker :: distinct struct {}
@(private="package")
Res_Mut_Marker :: distinct struct {}
@(private="package")
Optional_Res_Marker :: distinct struct {}
@(private="package")
Optional_Res_Mut_Marker :: distinct struct {}
@(private="package")
Local_Marker :: distinct struct {}
@(private="package")
Event_Reader_Marker :: distinct struct {}
@(private="package")
Event_Writer_Marker :: distinct struct {}
/// Requires a shared resource value to be present before a tick.
Res :: struct($T:typeid) { value:^T, kind:Res_Marker }
/// Requires exclusive access to a present resource value.
Res_Mut :: struct($T:typeid) { value:^T, kind:Res_Mut_Marker }
/// Claims shared access while allowing resource absence.
Optional_Res :: struct($T:typeid) { value:^T, kind:Optional_Res_Marker }
/// Claims mutable access while allowing resource absence.
Optional_Res_Mut :: struct($T:typeid) { value:^T, kind:Optional_Res_Mut_Marker }
/// Provides persistent, isolated state for one registered system.
Local :: struct($T:typeid) { value:^T, kind:Local_Marker }
/// Reads retained events using an independent per-system cursor.
Event_Reader :: struct($E:typeid) { log:^Event_Log, cursor:^Event_Cursor, event_type:^E, kind:Event_Reader_Marker }
/// Publishes events through an exclusive event-log claim.
Event_Writer :: struct($E:typeid) { log:^Event_Log, event_type:^E, kind:Event_Writer_Marker }
/// Reads a resource value using Odin shallow-copy semantics.
resource_read :: proc(r:Res($T)) -> T { assert(r.value!=nil); return r.value^ }
/// Returns a shallow resource copy when the optional parameter is present.
optional_resource_read :: proc(r:Optional_Res($T)) -> (value:T,ok:bool) {
    if r.value!=nil { return r.value^,true }; return
}
/// Borrows a mutable resource only for the current system invocation.
resource_write :: proc(r:Res_Mut($T)) -> ^T { return r.value }
/// Returns isolated persistent state for this registered system.
local :: proc(r:Local($T)) -> ^T { return r.value }
/// Returns borrowed unread events and advances only this reader cursor.
reader_read :: proc(r:Event_Reader($E)) -> []E {
    events:=Events(E){r.log}
    return events_read(&events,r.cursor)
}
/// Transfers event ownership to the prepared event log.
writer_send :: proc(r:Event_Writer($E),value:E) { owned_value:=value; event_send_raw(r.log,rawptr(&owned_value)) }

@(private="package")
Claim_Namespace :: enum { Component, Resource, Event }
@(private="package")
Claim :: struct { T:typeid, namespace:Claim_Namespace, write:bool }
@(private="package")
Param_Entry :: struct { kind:typeid, T:typeid, offset:int, persistent:rawptr, name:string, ops:Value_Ops }
@(private="package")
claim_conflicts :: proc(a,b:Claim) -> bool {
    return a.T==b.T && a.namespace==b.namespace && (a.write || b.write)
}
@(private="package")
claim_add :: proc(s:^System_Entry,c:Claim) -> bool {
    for old in s.claims { if claim_conflicts(old,c) { return false } }
    append(&s.claims,c)
    return true
}
@(private="package")
params_init :: proc(w:^World,s:^System_Entry,P:typeid) -> bool {
    info:=struct_info(P)
    for i in 0..<int(info.field_count) {
        T:=info.types[i].id
        wrapper:=struct_info(T)
        kind:=wrapper_kind(T)
        entry:=Param_Entry{kind=kind,offset=int(info.offsets[i]),name=info.names[i]}
        if kind==Query_Marker {
            Row:=pointer_target(wrapper.types[1]); Filter:=pointer_target(wrapper.types[2])
            q:=query_new(Row,Filter,w.allocator)
            entry.persistent=q
            append(&s.params,entry)
            for f in q.fields {
                if !claim_add(s,Claim{f.T,.Component,f.write}) { return false }
            }
        } else if kind==Commands_Marker {
            queue:=new(Command_Queue,w.allocator)
            commands_init(queue,w.allocator)
            entry.persistent=queue
            append(&s.params,entry)
        } else if kind==Local_Marker {
            target:=pointer_target(wrapper.types[0]); ti:=type_info_of(target)
            entry.T=target
            entry.persistent=allocate(ti.size,ti.align,w.allocator)
            mem.zero(entry.persistent,ti.size)
            append(&s.params,entry)
        } else if kind==Event_Reader_Marker || kind==Event_Writer_Marker {
            index:=1
            if kind==Event_Reader_Marker { index=2; entry.persistent=new(Event_Cursor,w.allocator) }
            target:=pointer_target(wrapper.types[index]); entry.T=target
            ensure_event_log(w,target)
            append(&s.params,entry)
            if !claim_add(s,Claim{target,.Event,kind==Event_Writer_Marker}) { return false }
        } else {
            if kind!=Res_Marker && kind!=Res_Mut_Marker && kind!=Optional_Res_Marker && kind!=Optional_Res_Mut_Marker { return false }
            target:=pointer_target(wrapper.types[0]); entry.T=target
            append(&s.params,entry)
            if !claim_add(s,Claim{target,.Resource,kind==Res_Mut_Marker || kind==Optional_Res_Mut_Marker}) { return false }
        }
    }
    return true
}
@(private="package")
params_prepare :: proc(w:^World,s:^System_Entry) -> bool {
    for entry in s.params {
        dst:=address(s.prepared,entry.offset)
        if entry.kind==Query_Marker {
            q:=cast(^Query_Data)entry.persistent
            query_prepare(w,q)
            (^rawptr)(dst)^=q
        } else if entry.kind==Commands_Marker || entry.kind==Local_Marker {
            (^rawptr)(dst)^=entry.persistent
        } else if entry.kind==Event_Reader_Marker || entry.kind==Event_Writer_Marker {
            log:=w.event_logs[entry.T]
            if log==nil { return false }
            (^rawptr)(dst)^=log
            if entry.kind==Event_Reader_Marker { (^rawptr)(address(dst,size_of(rawptr)))^=entry.persistent }
        } else {
            r:=w.resources[entry.T]
            required:=entry.kind==Res_Marker || entry.kind==Res_Mut_Marker
            if r==nil && required { return false }
            (^rawptr)(dst)^=nil
            if r!=nil { (^rawptr)(dst)^=r.data }
        }
    }
    return true
}
@(private="package")
params_commands :: proc(s:^System_Entry,w:^World,apply:bool) {
    for entry in s.params {
        if entry.kind==Commands_Marker { commands_drain(cast(^Command_Queue)entry.persistent,w,apply) }
    }
}
@(private="package")
params_destroy :: proc(s:^System_Entry,allocator:mem.Allocator) {
    for entry in s.params {
        if entry.kind==Query_Marker { query_delete(cast(^Query_Data)entry.persistent) }
        else if entry.kind==Commands_Marker { q:=cast(^Command_Queue)entry.persistent; commands_destroy(q); free(q,allocator) }
        else if entry.persistent!=nil {
            if entry.ops.destroy!=nil { entry.ops.destroy(entry.persistent) }
            mem.free(entry.persistent,allocator)
        }
    }
    delete(s.params); mem.free(s.prepared,allocator)
}

/// Initializes persistent Local state before execution, with optional ownership hooks.
initialize_local :: proc(w:^World,handle:System_Handle,name:string,value:$T,ops:=Value_Ops{})->bool {
    assert(!w.execution_active && !w.frozen)
    context.allocator=w.allocator
    entry:=cast(^System_Entry)handle
    found:=false
    for system in w.systems { if system==entry { found=true; break } }
    if !found { return false }
    for &param in entry.params {
        if param.kind==Local_Marker && param.name==name && param.T==T {
            if param.ops.destroy!=nil { param.ops.destroy(param.persistent) }
            owned:=value
            mem.copy(param.persistent,rawptr(&owned),size_of(T))
            param.ops=ops
            return true
        }
    }
    return false
}
