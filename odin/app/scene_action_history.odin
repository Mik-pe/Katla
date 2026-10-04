//! Existing entity history merges changed component revisions with live state before atomic admission.
package app

import ecs "../ecs"
import editor "../editor"
import "core:mem"
import "core:bytes"


@(private="package")
Scene_Action_Row :: struct {
    entity:ecs.Entity_Id,
    before_exists,after_exists:bool,
    before,after:[dynamic]editor.Component_Snapshot,
}
@(private="package")
Scene_Action_Command :: struct { rows:[dynamic]Scene_Action_Row,allocator:mem.Allocator }
@(private="package")
scene_action_command_destroy :: proc(state:rawptr,allocator:mem.Allocator) {
    command:=cast(^Scene_Action_Command)state
    for row in command.rows { editor.entity_components_destroy(row.before,allocator); editor.entity_components_destroy(row.after,allocator) }
    delete(command.rows); mem.free(command,allocator)
}
@(private="package")
scene_action_command_remap :: proc(state:rawptr,replacement:editor.Entity_Remap) {
    command:=cast(^Scene_Action_Command)state
    mapping:=make(map[ecs.Entity_Id]ecs.Entity_Id,command.allocator); defer delete(mapping)
    mapping[replacement.before]=replacement.after
    for &row in command.rows {
        if row.entity==replacement.before { row.entity=replacement.after }
        for values in ([2][]editor.Component_Snapshot{row.before[:],row.after[:]}) {
            for value in values { assert(editor.component_map_references(value.entry,value.value,{mapping,false})) }
        }
    }
}
@(private="package")
scene_action_component_find :: proc(values:[]editor.Component_Snapshot,entry:^editor.Editor_Entry)->rawptr {
    for value in values { if value.entry==entry { return value.value } }; return nil
}
@(private="package")
scene_action_component_equal :: proc(entry:^editor.Editor_Entry,a,b:rawptr,allocator:mem.Allocator)->bool {
    if a==nil || b==nil { return a==b }
    left,left_valid:=editor.editor_encode_value(entry,a,allocator); defer delete(left,allocator)
    right,right_valid:=editor.editor_encode_value(entry,b,allocator); defer delete(right,allocator)
    if !left_valid || !right_valid { return false }
    if bytes.equal(left,right) { return true }
    return scene_action_json_equal(left,right,allocator)
}
@(private="package")
scene_action_command_apply :: proc(state:rawptr,w:^ecs.World,reg:^editor.Component_Registry,redo:bool,remaps:^[dynamic]editor.Entity_Remap)->editor.Scene_Error {
    command:=cast(^Scene_Action_Command)state
    rows:=make([dynamic]editor.Restoration_Row,w.allocator); defer delete(rows)
    proposals:=make([dynamic][dynamic]editor.Component_Snapshot,w.allocator)
    defer { for values in proposals { editor.entity_components_destroy(values,w.allocator) }; delete(proposals) }
    for row in command.rows {
        if !row.before_exists || !row.after_exists {
            exists:=row.before_exists; components:=row.before[:]
            if redo { exists=row.after_exists; components=row.after[:] }
            append(&rows,editor.Restoration_Row{row.entity,exists,components}); continue
        }
        if !ecs.entity_exists(w,row.entity) { return .Entity_Not_Found }
        current:=editor.entity_components_capture(w,reg,row.entity)
        transferred:=false; defer { if !transferred { editor.entity_components_destroy(current,w.allocator) } }
        entries:=make([dynamic]^editor.Editor_Entry,w.allocator); defer delete(entries)
        for value in row.before { append(&entries,value.entry) }
        for value in row.after { if scene_action_component_find(row.before[:],value.entry)==nil { append(&entries,value.entry) } }
        changed:=false
        for entry in entries {
            before:=scene_action_component_find(row.before[:],entry); after:=scene_action_component_find(row.after[:],entry)
            if scene_action_component_equal(entry,before,after,w.allocator) { continue }
            if reg.entries[entry.name]!=entry { return .Component_Not_Found }
            expected,target:=before,after; if !redo { expected,target=after,before }
            live:=scene_action_component_find(current[:],entry)
            if !scene_action_component_equal(entry,live,expected,w.allocator) { return .Invalid_Operation }
            replacement:rawptr
            if target!=nil { replacement=particle_history_clone(entry,target,live,w.allocator) }
            for value,i in current { if value.entry==entry {
                if entry.ops.destroy!=nil { entry.ops.destroy(value.value) }; mem.free(value.value,w.allocator)
                ordered_remove(&current,i); break
            } }
            if replacement!=nil { append(&current,editor.Component_Snapshot{entry.name,entry,replacement}) }
            changed=true
        }
        if changed { append(&rows,editor.Restoration_Row{row.entity,true,current[:]}); append(&proposals,current); transferred=true }
    }
    offset:=len(remaps)
    error:=editor.restoration_apply(w,reg,rows[:],remaps)
    if error!=.None { return error }
    for replacement in remaps^[offset:] { scene_action_command_remap(command,replacement) }
    return .None
}
@(private="package")
scene_action_command_new :: proc(owner:^Authoring)->^Scene_Action_Command {
    command:=new(Scene_Action_Command,owner.world.allocator)
    command^={rows=make([dynamic]Scene_Action_Row,owner.world.allocator),allocator=owner.world.allocator}
    return command
}
@(private="package")
scene_action_command_group :: proc(command:^Scene_Action_Command)->editor.Undo_Group {
    for row in command.rows { for values in ([2][]editor.Component_Snapshot{row.before[:],row.after[:]}) { for value in values { particle_history_clear(value.entry,value.value) } } }
    ids:=make([]ecs.Entity_Id,len(command.rows),command.allocator); defer delete(ids,command.allocator)
    for row,i in command.rows { ids[i]=row.entity }
    return editor.undo_group_create(command,{scene_action_command_apply,scene_action_command_destroy,scene_action_command_remap},ids,command.allocator)
}
@(private="package")
scene_action_proposal_clone :: proc(owner:^Authoring,values:[]editor.Component_Snapshot)->ecs.Entity_Id {
    entity:=ecs.create_entity(&owner.world)
    for snapshot in values {
        value:=editor.editor_clone_value(snapshot.entry,snapshot.value,owner.world.allocator)
        inserted:=ecs.insert_component_value(&owner.world,entity,snapshot.entry.T,value)
        assert(inserted,"new exclusive proposal retains its entity")
        mem.free(value,owner.world.allocator)
    }
    return entity
}
