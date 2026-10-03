//! Descriptor-derived queries cache membership, never cached component addresses.
package ecs

import "core:reflect"
import "base:runtime"
import "core:mem"
import "core:thread"

@(private="package")
Read_Marker :: distinct struct {}
@(private="package")
Write_Marker :: distinct struct {}
@(private="package")
With_Marker :: distinct struct {}
@(private="package")
Without_Marker :: distinct struct {}
@(private="package")
Query_Marker :: distinct struct {}
/// Selects all entities matching the requested query components.
No_Filter :: struct {}
/// Describes shared component access; consume values with read.
Read :: struct($T:typeid) { value:^T, kind:Read_Marker }
/// Describes mutable component access; borrow values with write.
Write :: struct($T:typeid) { value:^T, kind:Write_Marker }
/// Requires a disjoint component type without returning its data.
With :: struct($T:typeid) { value:^T, kind:With_Marker }
/// Excludes entities containing a disjoint component type.
Without :: struct($T:typeid) { value:^T, kind:Without_Marker }
/// Reads a copy; owning containers inside T follow Odin's ordinary shallow-copy rules.
read :: #force_inline proc(r:Read($T)) -> T { assert(r.value!=nil); return r.value^ }
/// Borrows a mutable value only for the current prepared query scope.
write :: #force_inline proc(r:Write($T)) -> ^T { return r.value }
/// Carries descriptor types and one batch-scoped prepared query.
Query :: struct($Row:typeid,$Filter:typeid) {
    data:^Query_Data,
    row_type:^Row,
    filter_type:^Filter,
    kind:Query_Marker,
}
@(private="package")
Query_Field :: struct { T:typeid, offset:int, write:bool }
@(private="package")
Query_Filter :: struct { T:typeid, without:bool }
@(private="package")
Query_Data :: struct {
    fields:[dynamic]Query_Field,
    filters:[dynamic]Query_Filter,
    stores:[8]^Store,
    rows:[dynamic]Entity_Id,
    membership:map[Entity_Id]bool,
    epoch:u64,
    cached, changed_only:bool,
    allocator:mem.Allocator,
}
@(private="package")
struct_info :: proc(T:typeid) -> runtime.Type_Info_Struct {
    info,ok:=reflect.type_info_base(type_info_of(T)).variant.(runtime.Type_Info_Struct)
    assert(ok,"descriptor must be a struct")
    return info
}
@(private="package")
pointer_target :: proc(info:^runtime.Type_Info) -> typeid {
    pointer,ok:=reflect.type_info_base(info).variant.(runtime.Type_Info_Pointer)
    assert(ok && pointer.elem!=nil,"descriptor field must be a typed pointer")
    return pointer.elem.id
}
@(private="package")
wrapper_kind :: proc(T:typeid) -> typeid {
    info:=struct_info(T)
    assert(info.field_count>=2)
    return info.types[info.field_count-1].id
}
@(private="package")
query_new :: proc(Row,Filter:typeid, allocator:mem.Allocator) -> ^Query_Data {
    q:=new(Query_Data,allocator)
    q.allocator=allocator
    q.fields=make([dynamic]Query_Field,allocator)
    q.filters=make([dynamic]Query_Filter,allocator)
    q.rows=make([dynamic]Entity_Id,allocator)
    q.membership=make(map[Entity_Id]bool,allocator)
    row:=struct_info(Row)
    assert(row.field_count>0 && row.field_count<=8,"query arity must be 1..8")
    for i in 0..<int(row.field_count) {
        wrapper:=struct_info(row.types[i].id)
        kind:=wrapper_kind(row.types[i].id)
        assert(kind==Read_Marker || kind==Write_Marker,"query fields must use Read or Write")
        T:=pointer_target(wrapper.types[0])
        for f in q.fields { assert(f.T!=T,"duplicate query component") }
        append(&q.fields,Query_Field{T,int(row.offsets[i])+int(wrapper.offsets[0]),kind==Write_Marker})
    }
    filter:=struct_info(Filter)
    for i in 0..<int(filter.field_count) {
        wrapper:=struct_info(filter.types[i].id)
        kind:=wrapper_kind(filter.types[i].id)
        assert(kind==With_Marker || kind==Without_Marker)
        T:=pointer_target(wrapper.types[0])
        for f in q.fields { assert(f.T!=T,"filter overlaps query component") }
        for f in q.filters { assert(f.T!=T,"duplicate filter component") }
        append(&q.filters,Query_Filter{T,kind==Without_Marker})
    }
    return q
}
@(private="package")
query_delete :: proc(q:^Query_Data) {
    delete(q.fields); delete(q.filters); delete(q.rows); delete(q.membership); free(q,q.allocator)
}
@(private="package")
query_prepare :: proc(w:^World,q:^Query_Data,direct:=false) {
    candidate:^Store
    missing:=false
    for f,i in q.fields {
        s:=w.stores[f.T]
        q.stores[i]=s
        if s==nil { missing=true; continue }
        if candidate==nil || len(s.entities)<len(candidate.entities) { candidate=s }
    }
    for f in q.filters {
        s:=w.stores[f.T]
        if !f.without {
            if s==nil { missing=true; continue }
            if candidate==nil || len(s.entities)<len(candidate.entities) { candidate=s }
        }
    }
    if !q.cached || q.epoch!=w.structural_epoch || q.changed_only {
        clear(&q.rows); clear(&q.membership)
        if !missing && candidate!=nil {
            for id in candidate.entities {
                matches:=true
                changed:=false
                for _,i in q.fields {
                    s:=q.stores[i]
                    if store_ptr(s,id)==nil { matches=false; break }
                    changed=changed || s.all_changed || s.changed[id]
                }
                if !matches { continue }
                for f in q.filters {
                    present:=store_ptr(w.stores[f.T],id)!=nil
                    if present==f.without { matches=false; break }
                }
                if matches && (!q.changed_only || changed) { append(&q.rows,id); q.membership[id]=true }
            }
        }
        q.epoch=w.structural_epoch; q.cached=true
    }
    for f,i in q.fields {
        s:=q.stores[i]
        if !f.write || s==nil { continue }
        if direct || len(q.rows)==len(s.entities) { s.all_changed=true } else {
            for id in q.rows { s.changed[id]=true }
        }
    }
}
/// Freezes structural mutation until query_end; do not retain rows beyond that boundary.
query_begin :: proc(w:^World,$Row:typeid,$Filter:typeid, changed_only:=false, direct:=false) -> Query(Row,Filter) {
    assert(!w.frozen)
    q:=query_new(Row,Filter,w.allocator)
    q.changed_only=changed_only
    query_prepare(w,q,direct)
    w.frozen=true
    return Query(Row,Filter){data=q}
}
query_end :: proc(w:^World,q:^Query($Row,$Filter)) {
    assert(w.frozen && !w.execution_active)
    w.frozen=false
    query_delete(q.data); q.data=nil
}
/// Borrows the unique matched IDs for the current prepared query.
query_entities :: proc(q:^Query($Row,$Filter)) -> []Entity_Id { return q.data.rows[:] }
/// Resolves row pointers from the current prepared columns.
query_row :: #force_inline proc(q:^Query($Row,$Filter),id:Entity_Id) -> (row:Row,ok:bool) {
    if !q.data.membership[id] { return }
    for f,i in q.data.fields {
        p:=store_ptr(q.data.stores[i],id)
        if p==nil { return }
        (^rawptr)(address(rawptr(&row),f.offset))^=p
    }
    return row,true
}
@(private="package")
Chunk_Job :: struct($Row:typeid,$Filter:typeid) {
    query:^Query(Row,Filter), start,end:int, callback:proc(Entity_Id,Row),
}
/// Partitions unique rows and joins all chunk workers before returning.
query_parallel_each :: proc(q:^Query($Row,$Filter),chunk_size:int, callback:proc(Entity_Id,Row),workers:=4) {
    assert(chunk_size>0 && workers>0)
    jobs:=make([dynamic]Chunk_Job(Row,Filter),q.data.allocator)
    defer delete(jobs)
    for start:=0; start<len(q.data.rows); start+=chunk_size {
        append(&jobs,Chunk_Job(Row,Filter){q,start,min(start+chunk_size,len(q.data.rows)),callback})
    }
    if len(jobs)==0 { return }
    pool:thread.Pool
    chunk_pool_start(&pool,q.data.allocator,workers)
    defer { thread.pool_join(&pool); thread.pool_destroy(&pool) }
    for &job in jobs { thread.pool_add_task(&pool,q.data.allocator,proc(task:thread.Task) {
        job:=cast(^Chunk_Job(Row,Filter))task.data
        for id in job.query.data.rows[job.start:job.end] {
            row,ok:=query_row(job.query,id); assert(ok); job.callback(id,row)
        }
    },&job) }
    thread.pool_finish(&pool)
    for _ in jobs { _,ok:=thread.pool_pop_done(&pool); assert(ok) }
}

@(private="package")
chunk_pool_start :: proc(pool:^thread.Pool,allocator:mem.Allocator,workers:int) {
    thread.pool_init(pool,allocator,workers)
    thread.pool_start(pool)
}
