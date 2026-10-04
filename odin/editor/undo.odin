//! One owned undo command can contain a validated application batch or a scene edit.
package editor

import ecs "../ecs"
import "core:mem"

/// Records replacement identity after restoring a destroyed entity.
Entity_Remap :: struct { before,after:ecs.Entity_Id }
/// Mandatory callbacks preflight restoration, release owned state and remap target IDs.
Undo_Ops :: struct {
    apply:proc(rawptr,^ecs.World,^Component_Registry,bool,^[dynamic]Entity_Remap)->Scene_Error,
    destroy:proc(rawptr,mem.Allocator),
    remap:proc(rawptr,Entity_Remap),
}
/// Owns one command, affected identities and the last restoration's identity changes.
Undo_Group :: struct { state:rawptr, ops:Undo_Ops, entities:[]ecs.Entity_Id, remaps:[dynamic]Entity_Remap, allocator:mem.Allocator }
/// Transfers command state and clones its affected identity list.
undo_group_create :: proc(state:rawptr,ops:Undo_Ops,entities:[]ecs.Entity_Id,allocator:=context.allocator)->Undo_Group {
    assert(state!=nil && ops.apply!=nil && ops.destroy!=nil && ops.remap!=nil)
    owned:=make([]ecs.Entity_Id,len(entities),allocator); copy(owned,entities)
    return {state,ops,owned,make([dynamic]Entity_Remap,allocator),allocator}
}
/// Releases command state exactly once without changing the world.
undo_group_destroy :: proc(group:^Undo_Group) {
    if group.state!=nil { group.ops.destroy(group.state,group.allocator) }
    delete(group.entities,group.allocator); delete(group.remaps); group^={}
}
/// Remaps command targets after another history action restores a fresh generation.
undo_group_remap :: proc(group:^Undo_Group,remap:Entity_Remap) {
    if group.state==nil { return }
    group.ops.remap(group.state,remap)
    for &entity in group.entities { if entity==remap.before { entity=remap.after } }
}
@(private="package")
restore_group :: proc(w:^ecs.World,reg:^Component_Registry,group:^Undo_Group,redo:bool)->Scene_Error {
    if group.state==nil { return .None }
    context.allocator=group.allocator
    clear(&group.remaps)
    err:=group.ops.apply(group.state,w,reg,redo,&group.remaps)
    if err==.None { for remap in group.remaps { for &entity in group.entities { if entity==remap.before { entity=remap.after } } } }
    return err
}
/// Restores the command's previous state, preserving the group on failure.
undo_group :: proc(w:^ecs.World,reg:^Component_Registry,group:^Undo_Group)->Scene_Error { return restore_group(w,reg,group,false) }
/// Restores the command's subsequent state with its current generational targets.
redo_group :: proc(w:^ecs.World,reg:^Component_Registry,group:^Undo_Group)->Scene_Error { return restore_group(w,reg,group,true) }

@(private="package")
Entity_Command :: struct {
    entity:ecs.Entity_Id,
    registry:^Component_Registry,
    before_exists,after_exists:bool,
    before,after:[dynamic]Component_Snapshot,
    allocator:mem.Allocator,
}
@(private="package")
entity_snapshots_destroy :: proc(command:^Entity_Command) {
    component_snapshots_destroy(command.before,command.allocator)
    component_snapshots_destroy(command.after,command.allocator)
}
@(private="package")
entity_command_destroy :: proc(state:rawptr,allocator:mem.Allocator) {
    command:=cast(^Entity_Command)state; entity_snapshots_destroy(command); free(command,allocator)
}
@(private="package")
entity_command_remap :: proc(state:rawptr,remap:Entity_Remap) {
    command:=cast(^Entity_Command)state
    if command.entity==remap.before { command.entity=remap.after }
    context.allocator=command.allocator
    mapping:=make(map[ecs.Entity_Id]ecs.Entity_Id,command.allocator); defer delete(mapping)
    mapping[remap.before]=remap.after
    snapshot_lists:=[2][]Component_Snapshot{command.before[:],command.after[:]}
    for snapshots in snapshot_lists {
        for &snapshot in snapshots {
            if !snapshot.entry.has_references { continue }
            mapped:=component_map_references(snapshot.entry,snapshot.value,{mapping,false})
            assert(mapped,"partial reference maps must preserve unmapped IDs")
        }
    }
}
@(private="package")
entity_command_apply :: proc(state:rawptr,w:^ecs.World,reg:^Component_Registry,redo:bool,remaps:^[dynamic]Entity_Remap)->Scene_Error {
    command:=cast(^Entity_Command)state
    exists:=command.before_exists; snapshots:=command.before
    if redo { exists=command.after_exists; snapshots=command.after }
    if !exists { ecs.destroy_entity(w,command.entity); return .None }
    context.allocator=w.allocator
    decoded:=make([]struct { entry:^Editor_Entry, value:rawptr, transferred:bool },len(snapshots),w.allocator)
    defer {
        for item in decoded {
            if item.value!=nil {
                if !item.transferred && item.entry.ops.destroy!=nil { item.entry.ops.destroy(item.value) }
                mem.free(item.value,w.allocator)
            }
        }
        delete(decoded,w.allocator)
    }
    for snapshot,i in snapshots {
        entry:=reg.entries[snapshot.name]; if entry==nil { return .Component_Not_Found }; if entry!=snapshot.entry { return .Invalid_Operation }
        value:=editor_clone_value(entry,snapshot.value,w.allocator)
        decoded[i].entry=entry; decoded[i].value=value
    }
    replacement:Entity_Remap
    replaced:=false
    if !ecs.entity_exists(w,command.entity) {
        old:=command.entity; command.entity=ecs.create_entity(w)
        replacement={old,command.entity}; replaced=true
        append(remaps,replacement)
        mapping:=make(map[ecs.Entity_Id]ecs.Entity_Id,w.allocator); defer delete(mapping)
        mapping[old]=command.entity
        for item in decoded {
            ok:=component_map_references(item.entry,item.value,{mapping,false})
            assert(ok,"partial reference maps must preserve unmapped IDs")
        }
    }
    for _,entry in reg.entries { ecs.remove_component_type(w,command.entity,entry.T) }
    for &item in decoded {
        if !ecs.insert_component_value(w,command.entity,item.entry.T,item.value) { return .Entity_Not_Found }
        item.transferred=true
    }
    if replaced { entity_command_remap(command,replacement); editor_remap_world_references(w,reg,replacement) }
    return .None
}

/// Owns the state captured before one application-authorized entity edit.
Entity_Edit :: struct { state:rawptr,allocator:mem.Allocator }
/// Captures all registered components before a mutation; creation records an explicitly absent predecessor.
entity_edit_begin :: proc(w:^ecs.World,reg:^Component_Registry,id:ecs.Entity_Id,existed:bool)->(Entity_Edit,Scene_Error) {
    if existed && !ecs.entity_exists(w,id) { return {},.Entity_Not_Found }
    command:=new(Entity_Command,w.allocator)
    command^={entity=id,registry=reg,before_exists=existed,allocator=w.allocator}
    if existed { command.before=snapshot_entity(w,reg,id) }
    return {command,w.allocator},.None
}
/// Cancels recording without changing the world; use on a failed application mutation.
entity_edit_destroy :: proc(edit:^Entity_Edit) {
    if edit.state!=nil { entity_command_destroy(edit.state,edit.allocator) }; edit^={}
}
/// Captures post-edit state and transfers both sides into the canonical reference-aware command.
entity_edit_finish :: proc(edit:^Entity_Edit,w:^ecs.World,id:ecs.Entity_Id)->Undo_Group {
    assert(edit.state!=nil)
    command:=cast(^Entity_Command)edit.state; command.entity=id
    command.after_exists=ecs.entity_exists(w,id)
    if command.after_exists { command.after=snapshot_entity(w,command.registry,id) }
    group:=undo_group_create(command,{entity_command_apply,entity_command_destroy,entity_command_remap},{id},edit.allocator)
    edit^={}; return group
}
