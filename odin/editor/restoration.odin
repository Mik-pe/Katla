//! Component restoration admits a complete reversible proposal before retiring identities.
package editor

import ecs "../ecs"
import "core:mem"

/// Borrows an immutable target snapshot; absent targets are retired only after admission.
Restoration_Row :: struct { entity:ecs.Entity_Id, exists:bool, components:[]Component_Snapshot }
/// An application owner may prepare resources against the reversible prospective world.
Restoration_Participant :: struct {
    state:rawptr,
    prepare:proc(rawptr,^ecs.World,^Component_Registry,[]ecs.Entity_Id,[]ecs.Entity_Id)->(rawptr,Scene_Error),
    finish:proc(rawptr,rawptr,bool),
}

/// Captures registered components with their exact deep ownership hooks.
entity_components_capture :: proc(w:^ecs.World,reg:^Component_Registry,id:ecs.Entity_Id)->[dynamic]Component_Snapshot {
    return snapshot_entity(w,reg,id)
}
/// Releases captured component values; borrowed registry names and entries remain registered.
entity_components_destroy :: proc(values:[dynamic]Component_Snapshot,allocator:mem.Allocator) {
    component_snapshots_destroy(values,allocator)
}

@(private="package")
restoration_install :: proc(w:^ecs.World,reg:^Component_Registry,id:ecs.Entity_Id,values:[]Component_Snapshot) {
    for _,entry in reg.entries { ecs.remove_component_type(w,id,entry.T) }
    for snapshot in values {
        value:=editor_clone_value(snapshot.entry,snapshot.value,w.allocator)
        inserted:=ecs.insert_component_value(w,id,snapshot.entry.T,value)
        assert(inserted,"exclusive restoration retains its live target")
        mem.free(value,w.allocator)
    }
}

/// Applies an admitted batch, preserving existing identities and every component on rejection.
restoration_apply :: proc(w:^ecs.World,reg:^Component_Registry,rows:[]Restoration_Row,remaps:^[dynamic]Entity_Remap)->Scene_Error {
    context.allocator=w.allocator
    if len(rows)>100_000 { return .Invalid_Operation }
    participant,installed:=ecs.get_resource(w,Restoration_Participant)
    if installed && (participant.prepare==nil || participant.finish==nil) { return .Invalid_Operation }
    before:=make([][dynamic]Component_Snapshot,len(rows),w.allocator)
    proposed:=make([][dynamic]Component_Snapshot,len(rows),w.allocator)
    targets:=make([]ecs.Entity_Id,len(rows),w.allocator)
    created:=make([]bool,len(rows),w.allocator)
    replacements:=make([dynamic]Entity_Remap,w.allocator)
    changed:=make([dynamic]ecs.Entity_Id,w.allocator)
    removed:=make([dynamic]ecs.Entity_Id,w.allocator)
    defer {
        for values in before { component_snapshots_destroy(values,w.allocator) }
        for values in proposed { component_snapshots_destroy(values,w.allocator) }
        delete(before,w.allocator); delete(proposed,w.allocator); delete(targets,w.allocator); delete(created,w.allocator)
        delete(replacements); delete(changed); delete(removed)
    }
    for row,i in rows {
        for earlier in rows[:i] { if earlier.entity==row.entity { return .Invalid_Operation } }
        if !row.exists && !ecs.entity_exists(w,row.entity) { return .Entity_Not_Found }
        targets[i]=row.entity
        if row.exists {
            proposed[i]=make([dynamic]Component_Snapshot,w.allocator)
            for snapshot,j in row.components {
                entry:=reg.entries[snapshot.name]
                if entry==nil { return .Component_Not_Found }
                if entry!=snapshot.entry || snapshot.value==nil { return .Invalid_Operation }
                for earlier in row.components[:j] { if earlier.entry==entry { return .Invalid_Operation } }
                append(&proposed[i],Component_Snapshot{snapshot.name,entry,editor_clone_value(entry,snapshot.value,w.allocator)})
            }
            if ecs.entity_exists(w,row.entity) { before[i]=snapshot_entity(w,reg,row.entity) }
        }
    }
    committed:=false; reversible:=false
    defer {
        if !committed {
            if reversible {
                for replacement in replacements { editor_remap_world_references(w,reg,{replacement.after,replacement.before}) }
                for row,i in rows { if row.exists && !created[i] { restoration_install(w,reg,targets[i],before[i][:]) } }
            }
            for is_created,i in created { if is_created { ecs.destroy_entity(w,targets[i]) } }
        }
    }
    mapping:=make(map[ecs.Entity_Id]ecs.Entity_Id,w.allocator); defer delete(mapping)
    for row,i in rows {
        if !row.exists { append(&removed,row.entity); continue }
        if !ecs.entity_exists(w,row.entity) {
            targets[i]=ecs.create_entity(w); created[i]=true
            mapping[row.entity]=targets[i]; append(&replacements,Entity_Remap{row.entity,targets[i]})
        }
        append(&changed,targets[i])
    }
    for values in proposed { for value in values { if !component_map_references(value.entry,value.value,{mapping,false}) { return .Invalid_Operation } } }
    reversible=true
    for row,i in rows { if row.exists { restoration_install(w,reg,targets[i],proposed[i][:]) } }
    for replacement in replacements { editor_remap_world_references(w,reg,replacement) }
    token:rawptr
    if installed {
        error:Scene_Error
        token,error=participant.prepare(participant.state,w,reg,changed[:],removed[:])
        if error!=.None { return error }
    }
    accepted:=false
    defer { if installed { participant.finish(participant.state,token,accepted) } }
    for entity in removed { ecs.destroy_entity(w,entity) }
    append(remaps,..replacements[:])
    committed=true; accepted=true
    return .None
}
