//! Per-type paged sparse columns; all value allocations use the World's allocator.
package ecs

import "core:mem"
import "core:reflect"

PAGE_SIZE :: 1024
Sparse_Page :: [PAGE_SIZE]int
/// Owned component/resource values may register destruction and deep-copy hooks.
Value_Ops :: struct {
    destroy: proc(rawptr),
    clone: proc(dst, src: rawptr),
}
@(private="package")
Store :: struct {
    value_type: typeid,
    size, alignment, capacity: int,
    data: rawptr,
    entities: [dynamic]Entity_Id,
    pages: [dynamic]^Sparse_Page,
    changed: map[Entity_Id]bool,
    all_changed: bool,
    ops: Value_Ops,
    allocator: mem.Allocator,
}

@(private="package")
allocate :: proc(size, alignment: int, allocator: mem.Allocator) -> rawptr {
    p, err := mem.alloc(max(size,1),alignment,allocator)
    assert(err == nil, "ECS allocation failed")
    return p
}
@(private="package")
address :: #force_inline proc(data: rawptr, offset: int) -> rawptr {
    return rawptr(uintptr(data)+uintptr(offset))
}
@(private="package")
store_new :: proc(T: typeid, allocator: mem.Allocator, ops := Value_Ops{}) -> ^Store {
    s := new(Store, allocator)
    info := type_info_of(T)
    s^ = Store{value_type=T,size=max(info.size,1),alignment=info.align,allocator=allocator,ops=ops,
               entities=make([dynamic]Entity_Id,allocator), pages=make([dynamic]^Sparse_Page,allocator),
               changed=make(map[Entity_Id]bool,allocator)}
    return s
}
@(private="package")
store_index :: #force_inline proc(s: ^Store, id: Entity_Id) -> (int,bool) {
    if s==nil { return 0,false }
    i := entity_index(id)
    page := i/PAGE_SIZE
    if page>=len(s.pages) || s.pages[page]==nil { return 0,false }
    row := s.pages[page][i%PAGE_SIZE]-1
    return row, row>=0 && row<len(s.entities) && s.entities[row]==id
}
@(private="package")
store_ptr :: #force_inline proc(s: ^Store, id: Entity_Id) -> rawptr {
    i,ok := store_index(s,id)
    if !ok { return nil }
    return address(s.data,i*s.size)
}
@(private="package")
store_insert :: proc(s: ^Store, id: Entity_Id, value: rawptr) {
    context.allocator=s.allocator
    row,exists := store_index(s,id)
    if exists {
        if s.ops.destroy!=nil { s.ops.destroy(address(s.data,row*s.size)) }
    } else {
        row = len(s.entities)
        if row == s.capacity {
            capacity := max(8,s.capacity*2)
            data := allocate(capacity*s.size,s.alignment,s.allocator)
            mem.copy(data,s.data,row*s.size)
            mem.free(s.data,s.allocator)
            s.data, s.capacity = data,capacity
        }
        append(&s.entities,id)
        i := entity_index(id)
        page := i/PAGE_SIZE
        for len(s.pages)<=page { append(&s.pages,nil) }
        if s.pages[page]==nil { s.pages[page]=new(Sparse_Page,s.allocator) }
        s.pages[page][i%PAGE_SIZE] = row+1
    }
    mem.copy(address(s.data,row*s.size),value,type_info_of(s.value_type).size)
    s.changed[id]=true
}
@(private="package")
store_remove :: proc(s: ^Store, id: Entity_Id) -> bool {
    context.allocator=s.allocator
    row,ok := store_index(s,id)
    if !ok { return false }
    if s.ops.destroy!=nil { s.ops.destroy(address(s.data,row*s.size)) }
    last := len(s.entities)-1
    if row!=last {
        moved := s.entities[last]
        s.entities[row]=moved
        mem.copy(address(s.data,row*s.size),address(s.data,last*s.size),s.size)
        i := entity_index(moved)
        s.pages[i/PAGE_SIZE][i%PAGE_SIZE]=row+1
    }
    resize(&s.entities,last)
    i := entity_index(id)
    s.pages[i/PAGE_SIZE][i%PAGE_SIZE]=0
    delete_key(&s.changed,id)
    return true
}
@(private="package")
store_clear :: proc(s: ^Store) {
    context.allocator=s.allocator
    if s.ops.destroy!=nil {
        for _,i in s.entities { s.ops.destroy(address(s.data,i*s.size)) }
    }
    clear(&s.entities)
    for page in s.pages { if page!=nil { page^={} } }
    clear(&s.changed)
    s.all_changed=false
}
@(private="package")
store_delete :: proc(s: ^Store) {
    store_clear(s)
    for p in s.pages { free(p,s.allocator) }
    delete(s.pages); delete(s.entities); delete(s.changed)
    mem.free(s.data,s.allocator)
    free(s,s.allocator)
}

/// Registers optional ownership hooks before a type's first insertion.
register_component :: proc(w: ^World, $T: typeid, ops := Value_Ops{}) {
    assert(!w.frozen)
    assert(w.stores[T]==nil, "component already registered")
    w.stores[T]=store_new(T,w.allocator,ops)
}
/// Transfers value ownership to a live entity; stale targets are rejected.
add_component :: proc(w: ^World, id: Entity_Id, value: $T) -> bool {
    owned_value:=value
    return insert_component_value(w,id,T,rawptr(&owned_value))
}
/// Inserts an owned, correctly aligned value of T for editor/runtime reflection.
insert_component_value :: proc(w: ^World, id: Entity_Id, T: typeid, value: rawptr) -> bool {
    assert(!w.frozen)
    if !entity_exists(w,id) { return false }
    s := w.stores[T]
    if s==nil { s=store_new(T,w.allocator); w.stores[T]=s }
    store_insert(s,id,value)
    advance_epoch(w)
    append(&w.component_events,Component_Event{.Added,id,T})
    return true
}
/// Returns a value copy, so read access cannot mutate a column through the API.
get_component :: proc(w: ^World, id: Entity_Id, $T: typeid) -> (value: T, ok: bool) {
    assert(!w.frozen)
    if !entity_exists(w,id) { return }
    p := cast(^T)store_ptr(w.stores[T],id)
    if p!=nil { return p^,true }
    return
}
/// Borrows until the next structural operation and marks this entity changed.
get_component_mut :: proc(w: ^World, id: Entity_Id, $T: typeid) -> ^T {
    assert(!w.frozen)
    if !entity_exists(w,id) { return nil }
    s:=w.stores[T]
    p:=cast(^T)store_ptr(s,id)
    if p!=nil { s.changed[id]=true }
    return p
}
/// Removes a live component through lifecycle and change-tracking bookkeeping.
remove_component :: proc(w: ^World, id: Entity_Id, $T: typeid) -> bool {
    return remove_component_type(w,id,T)
}
/// Removes a runtime-selected type through the same lifecycle path.
remove_component_type :: proc(w: ^World, id: Entity_Id, T: typeid) -> bool {
    assert(!w.frozen)
    if !entity_exists(w,id) || w.stores[T]==nil || !store_remove(w.stores[T],id) { return false }
    advance_epoch(w)
    append(&w.component_events,Component_Event{.Removed,id,T})
    return true
}
/// Spawns a struct bundle with one component per field, up to eight fields.
spawn :: proc(w: ^World, bundle: $B) -> Entity_Id {
    owned_bundle:=bundle
    fields := reflect.struct_fields_zipped(B)
    assert(len(fields)<=8)
    id:=create_entity(w)
    for f in fields {
        insert_component_value(w,id,f.type.id,address(rawptr(&owned_bundle),int(f.offset)))
    }
    return id
}
/// Clears per-type change flags on the caller thread.
clear_changed :: proc(w: ^World) {
    assert(!w.frozen)
    for _,s in w.stores { clear(&s.changed); s.all_changed=false }
}
/// Tests insertion or requested mutable access since the last clearing boundary.
component_changed :: proc(w: ^World, id: Entity_Id, $T:typeid) -> bool {
    s:=w.stores[T]
    return s!=nil && store_ptr(s,id)!=nil && (s.all_changed || s.changed[id])
}

/// Returns an ephemeral raw component borrow for the reflection package.
component_address :: proc(w:^World,id:Entity_Id,T:typeid)->rawptr {
    assert(!w.frozen)
    if !entity_exists(w,id) { return nil }
    return store_ptr(w.stores[T],id)
}
/// Reports registered ownership hooks without exposing a storage implementation.
component_ops :: proc(w:^World,T:typeid)->(Value_Ops,bool) {
    s:=w.stores[T]; if s==nil { return {},false }; return s.ops,true
}
