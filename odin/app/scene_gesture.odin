//! Captured scene gestures retain their first revision and publish one shared history action.
package app

import ecs "../ecs"
import editor "../editor"
import "core:mem"

@(private="package")
Scene_Gesture_Component :: struct { entity:ecs.Entity_Id,entry:^editor.Editor_Entry }
/// Owns first-before and last-after revisions while guarding only edited components.
Scene_Gesture :: struct { command:^Scene_Action_Command,owner:^Authoring,active:bool,edited:[dynamic]Scene_Gesture_Component }
/// Captures exact component owners before the first preview; duplicate and protected selections fail.
scene_gesture_begin :: proc(owner:^Authoring,gesture:^Scene_Gesture,selected:[]ecs.Entity_Id)->editor.Scene_Error {
    if gesture.active || len(selected)==0 || len(selected)>256 { return .Invalid_Operation }
    if owner.mode!=.Editing { return .Editing_Required }
    command:=scene_action_command_new(owner)
    success:=false; defer { if !success { scene_action_command_destroy(command,owner.world.allocator) } }
    for id,i in selected {
        for earlier in selected[:i] { if earlier==id { return .Invalid_Operation } }
        if error:=scene_action_target(owner,id); error!=.None { return error }
        before:=editor.entity_components_capture(&owner.world,&owner.registry,id)
        after:=editor.entity_components_capture(&owner.world,&owner.registry,id)
        append(&command.rows,Scene_Action_Row{id,true,true,before,after})
    }
    gesture^={command=command,owner=owner,active=true,edited=make([dynamic]Scene_Gesture_Component,owner.world.allocator)}; success=true
    return .None
}
@(private="package")
scene_gesture_components_equal :: proc(owner:^Authoring,a,b:[]editor.Component_Snapshot)->bool {
    if len(a)!=len(b) { return false }
    for value in a { if !scene_action_component_equal(value.entry,value.value,scene_action_component_find(b,value.entry),owner.world.allocator) { return false } }
    return true
}
@(private="package")
scene_gesture_preflight :: proc(owner:^Authoring,gesture:^Scene_Gesture)->editor.Scene_Error {
    if !gesture.active || gesture.owner!=owner { return .Invalid_Operation }
    if owner.mode!=.Editing { return .Editing_Required }
    for row in gesture.command.rows {
        if error:=scene_action_target(owner,row.entity); error!=.None { return error }
        for component in gesture.edited { if component.entity!=row.entity { continue }
            current:=ecs.component_address(&owner.world,row.entity,component.entry.T)
            expected:=scene_action_component_find(row.after[:],component.entry)
            if !scene_action_component_equal(component.entry,current,expected,owner.world.allocator) { return .Invalid_Operation }
        }
    }
    return .None
}
/// Applies one atomic field preview without adding history entries for pointer movement.
scene_gesture_preview :: proc(owner:^Authoring,gesture:^Scene_Gesture,op:editor.Scene_Op)->editor.Scene_Error {
    if error:=scene_gesture_preflight(owner,gesture); error!=.None { return error }
    if op.kind!=.Set_Field { return .Invalid_Operation }
    operations:=make([]editor.Scene_Op,len(gesture.command.rows),owner.world.allocator); defer delete(operations,owner.world.allocator)
    for row,i in gesture.command.rows { operations[i]=op; operations[i].entity=row.entity }
    return scene_gesture_preview_values(owner,gesture,operations)
}
/// Applies distinct per-entity placements in one admission, preserving multi-selection offsets.
scene_gesture_preview_values :: proc(owner:^Authoring,gesture:^Scene_Gesture,operations:[]editor.Scene_Op)->editor.Scene_Error {
    if error:=scene_gesture_preflight(owner,gesture); error!=.None { return error }
    if len(operations)!=len(gesture.command.rows) { return .Invalid_Operation }
    command:=scene_action_command_new(owner); defer scene_action_command_destroy(command,owner.world.allocator)
    for row,i in gesture.command.rows {
        op:=operations[i]
        if op.kind!=.Set_Field || op.entity!=row.entity { return .Invalid_Operation }
        entry:=owner.registry.entries[op.component]; if entry==nil { return .Component_Not_Found }
        current:=ecs.component_address(&owner.world,row.entity,entry.T)
        expected:=scene_action_component_find(row.after[:],entry)
        if current==nil || expected==nil { return .Component_Not_Found }
        if !scene_action_component_equal(entry,current,expected,owner.world.allocator) { return .Invalid_Operation }
        before:=editor.entity_components_capture(&owner.world,&owner.registry,row.entity)
        proposal:=scene_action_proposal_clone(owner,before[:])
        error:=scene_action_mutate_proposal(owner,proposal,op)
        if error!=.None { ecs.destroy_entity(&owner.world,proposal); editor.entity_components_destroy(before,owner.world.allocator); return error }
        after:=editor.entity_components_capture(&owner.world,&owner.registry,proposal); ecs.destroy_entity(&owner.world,proposal)
        append(&command.rows,Scene_Action_Row{row.entity,true,true,before,after})
    }
    remaps:=make([dynamic]editor.Entity_Remap,owner.world.allocator); defer delete(remaps)
    if error:=scene_action_command_apply(command,&owner.world,&owner.registry,true,&remaps); error!=.None { return error }
    for &row,i in gesture.command.rows {
        entry:=owner.registry.entries[operations[i].component]
        current:=ecs.component_address(&owner.world,row.entity,entry.T)
        for &value in row.after { if value.entry==entry {
            if entry.ops.destroy!=nil { entry.ops.destroy(value.value) }; mem.free(value.value,owner.world.allocator)
            value.value=editor.editor_clone_value(entry,current,owner.world.allocator); break
        } }
        protected:=false; for component in gesture.edited { if component.entity==row.entity && component.entry==entry { protected=true; break } }
        if !protected { append(&gesture.edited,Scene_Gesture_Component{row.entity,entry}) }
    }
    return .None
}
/// Restores the first revision through the same native admission boundary; rejection retains the gesture.
scene_gesture_cancel :: proc(owner:^Authoring,gesture:^Scene_Gesture)->editor.Scene_Error {
    if error:=scene_gesture_preflight(owner,gesture); error!=.None { return error }
    remaps:=make([dynamic]editor.Entity_Remap,owner.world.allocator); defer delete(remaps)
    error:=scene_action_command_apply(gesture.command,&owner.world,&owner.registry,false,&remaps)
    if error!=.None { return error }
    scene_gesture_destroy(gesture); return .None
}
/// Records first-before and last-after once without replaying an already accepted preview.
scene_gesture_finish :: proc(owner:^Authoring,gesture:^Scene_Gesture)->editor.Scene_Error {
    if error:=scene_gesture_preflight(owner,gesture); error!=.None { return error }
    changed:=false
    for row in gesture.command.rows { if !scene_gesture_components_equal(owner,row.before[:],row.after[:]) { changed=true; break } }
    if !changed { scene_gesture_destroy(gesture); return .None }
    group:=scene_action_command_group(gesture.command)
    result:=error_result(&owner.world,.None); for row in gesture.command.rows { append(&result.entities,row.entity) }
    editor.agent_record_action(&owner.agent.session,{kind=.Application,tool_name="scene_gesture"},&result,&group)
    delete(gesture.edited); gesture^={}; return .None
}
/// Releases the retained revisions; ordinary input paths finish or cancel before teardown.
scene_gesture_destroy :: proc(gesture:^Scene_Gesture) {
    if gesture.command!=nil { scene_action_command_destroy(gesture.command,gesture.owner.world.allocator) }
    delete(gesture.edited); gesture^={}
}
