//! Per-system FIFO structural commands applied after prepared borrows end.
package ecs

import "core:mem"
import "core:reflect"

@(private="package")
Commands_Marker :: distinct struct {}
/// Queues structural changes in a per-system FIFO buffer.
Commands :: struct { queue:^Command_Queue, kind:Commands_Marker }
@(private="package")
Command_Kind :: enum { Spawn, Destroy, Insert, Remove }
@(private="package")
Command_Value :: struct { T:typeid, data:rawptr, ops:Value_Ops }
@(private="package")
Deferred_Command :: struct { kind:Command_Kind, target:Entity_Id, values:[dynamic]Command_Value }
/// Owns pending structural values and their allocation policy.
Command_Queue :: struct { commands:[dynamic]Deferred_Command, allocator:mem.Allocator }
/// Initializes a stationary, caller-owned structural command queue.
commands_init :: proc(q:^Command_Queue,allocator:=context.allocator) {
    q.allocator=allocator; q.commands=make([dynamic]Deferred_Command,allocator)
}
@(private="package")
command_value :: proc(T:typeid,data:rawptr,allocator:mem.Allocator,ops:=Value_Ops{}) -> Command_Value {
    context.allocator=allocator
    info:=type_info_of(T)
    p:=allocate(info.size,info.align,allocator)
    if ops.clone!=nil { ops.clone(p,data) } else { mem.copy(p,data,info.size) }
    return Command_Value{T,p,ops}
}
/// Queues a bundle; no live ID is reserved before application.
command_spawn :: proc(c:Commands,bundle:$B,ops:[8]Value_Ops={}) {
    cmd:=Deferred_Command{kind=.Spawn,values=make([dynamic]Command_Value,c.queue.allocator)}
    owned_bundle:=bundle
    fields:=bundle_fields(B)
    assert(len(fields)<=8)
    for f,i in fields {
        append(&cmd.values,command_value(f.type.id,address(rawptr(&owned_bundle),int(f.offset)),c.queue.allocator,ops[i]))
    }
    append(&c.queue.commands,cmd)
}
/// Queues destruction of a generational handle; stale targets have no effect.
command_destroy :: proc(c:Commands,id:Entity_Id) {
    append(&c.queue.commands,Deferred_Command{kind=.Destroy,target=id})
}
/// Queues an owned value, with matching clone/destruction hooks when needed.
command_insert :: proc(c:Commands,id:Entity_Id,value:$T,ops:=Value_Ops{}) {
    cmd:=Deferred_Command{kind=.Insert,target=id,values=make([dynamic]Command_Value,c.queue.allocator)}
    owned_value:=value
    append(&cmd.values,command_value(T,rawptr(&owned_value),c.queue.allocator,ops))
    append(&c.queue.commands,cmd)
}
/// Queues removal of a runtime component type from a generational entity.
command_remove :: proc(c:Commands,id:Entity_Id,$T:typeid) {
    cmd:=Deferred_Command{kind=.Remove,target=id,values=make([dynamic]Command_Value,c.queue.allocator)}
    append(&cmd.values,Command_Value{T=T})
    append(&c.queue.commands,cmd)
}
@(private="package")
commands_drain :: proc(q:^Command_Queue,w:^World,apply:bool) {
    context.allocator=q.allocator
    for cmd in q.commands {
        id:=cmd.target
        transferred:=false
        if apply {
            switch cmd.kind {
            case .Spawn: id=create_entity(w)
            case .Destroy: destroy_entity(w,id)
            case .Remove: remove_component_type(w,id,cmd.values[0].T)
            case .Insert:
            }
            if cmd.kind==.Spawn || cmd.kind==.Insert {
                for v in cmd.values {
                    if entity_exists(w,id) {
                        store:=w.stores[v.T]
                        if store==nil { w.stores[v.T]=store_new(v.T,w.allocator,v.ops) }
                        else { assert(store.ops.destroy==v.ops.destroy && store.ops.clone==v.ops.clone,"command ownership hooks must match storage") }
                    }
                    transferred=insert_component_value(w,id,v.T,v.data)
                }
            }
        }
        for v in cmd.values {
            if !transferred && v.ops.destroy!=nil { v.ops.destroy(v.data) }
            mem.free(v.data,q.allocator)
        }
        delete(cmd.values)
    }
    clear(&q.commands)
}
/// Applies this queue in FIFO order outside a prepared query scope.
commands_apply :: proc(q:^Command_Queue,w:^World) { commands_drain(q,w,true) }
/// Discards pending owned values and releases command storage.
commands_destroy :: proc(q:^Command_Queue) { commands_drain(q,nil,false); delete(q.commands) }

@(private="package")
bundle_fields :: proc(T:typeid)->#soa[]reflect.Struct_Field { return reflect.struct_fields_zipped(T) }
