//! Additive scene transactions retain prepared values and bind every restored entity before references.
package editor

import ecs "../ecs"
import "core:mem"

@(private="package")
Created_Row :: struct { entity:ecs.Entity_Id,components:[dynamic]Component_Snapshot }
@(private="package")
Created_Group :: struct { rows:[]Created_Row,allocator:mem.Allocator }
@(private="package")
created_group_destroy :: proc(state:rawptr,allocator:mem.Allocator) {
    command:=cast(^Created_Group)state
    for row in command.rows { component_snapshots_destroy(row.components,allocator) }
    delete(command.rows,allocator); free(command,allocator)
}
@(private="package")
created_group_remap :: proc(state:rawptr,remap:Entity_Remap) {
    command:=cast(^Created_Group)state; context.allocator=command.allocator
    mapping:=make(map[ecs.Entity_Id]ecs.Entity_Id,command.allocator); defer delete(mapping); mapping[remap.before]=remap.after
    for &row in command.rows {
        if row.entity==remap.before { row.entity=remap.after }
        for component in row.components { assert(component_map_references(component.entry,component.value,{mapping,false})) }
    }
}
@(private="package")
created_group_apply :: proc(state:rawptr,w:^ecs.World,reg:^Component_Registry,redo:bool,remaps:^[dynamic]Entity_Remap)->Scene_Error {
    command:=cast(^Created_Group)state; context.allocator=w.allocator
    rows:=make([]Restoration_Row,len(command.rows),w.allocator); defer delete(rows,w.allocator)
    for row,i in command.rows {
        if ecs.entity_exists(w,row.entity)==redo { return .Invalid_Operation }
        rows[i]={row.entity,redo,row.components[:]}
    }
    offset:=len(remaps)
    error:=restoration_apply(w,reg,rows,remaps)
    if error!=.None { return error }
    for replacement in remaps^[offset:] { created_group_remap(command,replacement) }
    return .None
}

/// Captures one created subtree as an owned command; redo preserves prepared revisions and remaps cross-entity references.
created_entities_group :: proc(w:^ecs.World,reg:^Component_Registry,entities:[]ecs.Entity_Id)->Undo_Group {
    context.allocator=w.allocator
    assert(len(entities)>0)
    command:=new(Created_Group,w.allocator); command^={rows=make([]Created_Row,len(entities),w.allocator),allocator=w.allocator}
    for entity,i in entities {
        assert(ecs.entity_exists(w,entity))
        for earlier in entities[:i] { assert(earlier!=entity) }
        command.rows[i]={entity,snapshot_entity(w,reg,entity)}
    }
    return undo_group_create(command,{created_group_apply,created_group_destroy,created_group_remap},entities,w.allocator)
}

@(private="package")
removed_group_apply :: proc(state:rawptr,w:^ecs.World,reg:^Component_Registry,redo:bool,remaps:^[dynamic]Entity_Remap)->Scene_Error { return created_group_apply(state,w,reg,!redo,remaps) }
/// Captures a subtree before removal; the caller applies redo once after enclosing preparation succeeds.
removed_entities_group :: proc(w:^ecs.World,reg:^Component_Registry,entities:[]ecs.Entity_Id)->Undo_Group {
    group:=created_entities_group(w,reg,entities); group.ops.apply=removed_group_apply; return group
}
