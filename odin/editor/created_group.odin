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
    for row in command.rows { if ecs.entity_exists(w,row.entity)==redo { return .Invalid_Operation } }
    if !redo { for row in command.rows { ecs.destroy_entity(w,row.entity) }; return .None }
    prepared:=make([][dynamic]Component_Snapshot,len(command.rows),w.allocator)
    defer { for components in prepared { component_snapshots_destroy(components,w.allocator) }; delete(prepared,w.allocator) }
    for row,i in command.rows {
        prepared[i]=make([dynamic]Component_Snapshot,w.allocator)
        for snapshot in row.components {
            entry:=reg.entries[snapshot.name]; if entry==nil { return .Component_Not_Found }; if entry!=snapshot.entry { return .Invalid_Operation }
            append(&prepared[i],Component_Snapshot{snapshot.name,entry,editor_clone_value(entry,snapshot.value,w.allocator)})
        }
    }
    replacements:=make([]Entity_Remap,len(command.rows),w.allocator); defer delete(replacements,w.allocator)
    mapping:=make(map[ecs.Entity_Id]ecs.Entity_Id,w.allocator); defer delete(mapping)
    allocated:=0; success:=false; defer { if !success { for replacement in replacements[:allocated] { ecs.destroy_entity(w,replacement.after) } } }
    for row,i in command.rows { replacement:=Entity_Remap{row.entity,ecs.create_entity(w)}; replacements[i]=replacement; mapping[replacement.before]=replacement.after; allocated+=1 }
    for components,i in prepared {
        for &component in components {
            if !component_map_references(component.entry,component.value,{mapping,false}) { return .Invalid_Operation }
            if !ecs.insert_component_value(w,replacements[i].after,component.entry.T,component.value) { return .Entity_Not_Found }
            mem.free(component.value,w.allocator); component.value=nil
        }
    }
    for replacement in replacements { created_group_remap(command,replacement); editor_remap_world_references(w,reg,replacement); append(remaps,replacement) }
    success=true; return .None
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
