//! World owns identity, storage registries and caller-thread lifecycle.
package ecs

import "core:mem"
import "core:thread"

/// Owns entity identity, components, resources, events and systems.
World :: struct {
    allocator: mem.Allocator,
    slots: [dynamic]Entity_Slot,
    free_slots: [dynamic]int,
    live_count: int,
    stores: map[typeid]^Store,
    resources: map[typeid]^Resource_Entry,
    event_logs: map[typeid]^Event_Log,
    systems: [dynamic]^System_Entry,
    pool: ^thread.Pool,
    frozen, execution_active, clear_systems_requested: bool,
    structural_epoch: u64,
    parallel_work_threshold: int,
    entity_events: [dynamic]Entity_Event,
    component_events: [dynamic]Component_Event,
}

/// Initializes a stationary World. Its allocator must be thread-safe for parallel use.
world_init :: proc(w: ^World, worker_count := 0, allocator := context.allocator) {
    w.allocator=allocator
    w.slots=make([dynamic]Entity_Slot,allocator)
    w.free_slots=make([dynamic]int,allocator)
    w.stores=make(map[typeid]^Store,allocator)
    w.resources=make(map[typeid]^Resource_Entry,allocator)
    w.event_logs=make(map[typeid]^Event_Log,allocator)
    w.systems=make([dynamic]^System_Entry,allocator)
    w.entity_events=make([dynamic]Entity_Event,allocator)
    w.component_events=make([dynamic]Component_Event,allocator)
    w.parallel_work_threshold=32768
    if worker_count>0 {
        w.pool=new(thread.Pool,allocator)
        thread.pool_init(w.pool,allocator,worker_count)
        thread.pool_start(w.pool)
    }
}
/// Joins workers, shuts down systems and releases all owned allocations.
world_destroy :: proc(w: ^World) {
    assert(!w.execution_active && !w.frozen)
    clear_systems(w)
    if w.pool!=nil {
        thread.pool_join(w.pool); thread.pool_destroy(w.pool); free(w.pool,w.allocator)
    }
    for _,s in w.stores { store_delete(s) }
    for _,r in w.resources { resource_delete(r,w.allocator) }
    for _,log in w.event_logs { event_log_delete(log,w.allocator) }
    delete(w.slots); delete(w.free_slots); delete(w.stores); delete(w.resources)
    delete(w.event_logs); delete(w.systems); delete(w.entity_events); delete(w.component_events)
    w^={}
}
@(private="package")
advance_epoch :: proc(w: ^World) {
    assert(w.structural_epoch<max(u64), "structural epoch exhausted")
    w.structural_epoch+=1
}
/// Verifies free slots, generations, sparse/dense mappings and component ownership.
validate :: proc(w: ^World) -> bool {
    assert(!w.frozen)
    live:=0
    free_seen:=make(map[int]bool,w.allocator)
    defer delete(free_seen)
    for i in w.free_slots {
        if i<0 || i>=len(w.slots) || free_seen[i] || w.slots[i].occupied || w.slots[i].retired { return false }
        free_seen[i]=true
    }
    for slot,i in w.slots {
        if slot.occupied { live+=1 } else if !slot.retired && !free_seen[i] { return false }
    }
    if live!=w.live_count { return false }
    for _,s in w.stores {
        for id,row in s.entities {
            actual,ok:=store_index(s,id)
            if !entity_exists(w,id) || !ok || actual!=row { return false }
        }
    }
    return true
}
/// Destroys live entities that have no component columns.
cleanup_empty_entities :: proc(w: ^World) {
    ids:=entity_ids(w)
    defer delete(ids)
    for id in ids {
        present:=false
        for _,s in w.stores { if store_ptr(s,id)!=nil { present=true; break } }
        if !present { destroy_entity(w,id) }
    }
}

/// Verifies every supplied handle denotes a currently live entity.
validate_entities :: proc(w:^World,ids:[]Entity_Id)->bool {
    for id in ids { if !entity_exists(w,id) { return false } }
    return true
}
