//! Generational entity identity and lifecycle events.
package ecs

/// Identifies an entity by its 32-bit slot and 32-bit generation.
Entity_Id :: distinct u64
/// Stores occupancy, generation and permanent retirement for one slot.
Entity_Slot :: struct { generation: u32, occupied, retired: bool }
Entity_Event_Kind :: enum { Spawned, Destroyed }
Component_Event_Kind :: enum { Added, Removed }
/// Records creation or destruction of one generational entity.
Entity_Event :: struct { kind: Entity_Event_Kind, entity: Entity_Id }
/// Records insertion or removal of one component type.
Component_Event :: struct { kind: Component_Event_Kind, entity: Entity_Id, component: typeid }

@(private="package")
entity_id :: #force_inline proc(index, generation: u32) -> Entity_Id {
    return Entity_Id(u64(generation)<<32 | u64(index))
}
entity_index :: #force_inline proc(id: Entity_Id) -> int { return int(u64(id)&0xffffffff) }
entity_generation :: #force_inline proc(id: Entity_Id) -> u32 { return u32(u64(id)>>32) }

/// Tests whether the complete handle still denotes a live entity.
entity_exists :: #force_inline proc(w: ^World, id: Entity_Id) -> bool {
    i := entity_index(id)
    return i < len(w.slots) && w.slots[i].occupied && w.slots[i].generation == entity_generation(id)
}

/// Allocates identity only on the caller thread, outside a frozen query batch.
create_entity :: proc(w: ^World) -> Entity_Id {
    assert(!w.frozen, "structural mutation during a query batch")
    i: int
    if len(w.free_slots)>0 {
        i = pop(&w.free_slots)
        w.slots[i].occupied = true
    } else {
        assert(u64(len(w.slots)) <= 0xffffffff, "entity index space exhausted")
        i = len(w.slots)
        append(&w.slots, Entity_Slot{occupied=true})
    }
    id := entity_id(u32(i), w.slots[i].generation)
    w.live_count += 1
    advance_epoch(w)
    append(&w.entity_events, Entity_Event{.Spawned, id})
    return id
}

@(private="package")
retire_entity :: proc(w: ^World, id: Entity_Id) {
    i := entity_index(id)
    slot := &w.slots[i]
    slot.occupied = false
    if slot.generation == 0xffffffff { slot.retired = true } else {
        slot.generation += 1
        append(&w.free_slots, i)
    }
    w.live_count -= 1
}

/// Removes all components and invalidates identity before emitting Destroyed.
destroy_entity :: proc(w: ^World, id: Entity_Id) -> bool {
    assert(!w.frozen)
    if !entity_exists(w, id) { return false }
    retire_entity(w, id)
    for key, store in w.stores {
        if store_remove(store, id) { append(&w.component_events, Component_Event{.Removed,id,key}) }
    }
    advance_epoch(w)
    append(&w.entity_events, Entity_Event{.Destroyed,id})
    return true
}

/// Invalidates live IDs without resetting generations or emitting lifecycle events.
clear_entities :: proc(w: ^World) {
    assert(!w.frozen)
    clear(&w.free_slots)
    for i := len(w.slots)-1; i>=0; i-=1 {
        slot := &w.slots[i]
        if slot.occupied {
            retire_entity(w, entity_id(u32(i),slot.generation))
        }
    }
    clear(&w.free_slots)
    for i := len(w.slots)-1; i>=0; i-=1 {
        if !w.slots[i].retired { append(&w.free_slots,i) }
    }
    for _, store in w.stores { store_clear(store) }
    advance_epoch(w)
}

/// Returns an owned snapshot in allocator slot order; the caller deletes it.
entity_ids :: proc(w: ^World) -> [dynamic]Entity_Id {
    result := make([dynamic]Entity_Id,0,w.live_count,w.allocator)
    for slot,i in w.slots {
        if slot.occupied { append(&result,entity_id(u32(i),slot.generation)) }
    }
    return result
}
