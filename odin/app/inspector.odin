//! Inspector material gestures preview validated values and commit one shared undo action.
package app

import ecs "../ecs"
import editor "../editor"
import agent "../agent"
import "core:mem"
import "core:encoding/json"

/// A stationary owner-thread gesture retains exact authored first/last linear values.
Material_Gesture :: struct { edits:[]Material_Edit, allocator:mem.Allocator, active,changed:bool }
/// Begins a selection gesture only after validating the complete editable target set.
material_gesture_begin :: proc(app:^Authoring,gesture:^Material_Gesture,entities:[]ecs.Entity_Id)->editor.Scene_Error {
    if gesture.active { return .Invalid_Operation }
    if app.mode!=.Editing { return .Editing_Required }
    if len(entities)<1 || len(entities)>256 { return .Invalid_Operation }
    edits:=make([]Material_Edit,len(entities),app.world.allocator)
    success:=false; defer { if !success { delete(edits,app.world.allocator) } }
    for entity,i in entities {
        for previous in entities[:i] { if entity==previous { return .Invalid_Operation } }
        surface,error:=target_surface(&app.world,entity); if error!=.None { return error }
        if !agent.material_values_valid(material_values(surface)) { return .Invalid_Operation }
        edits[i]={entity,surface,surface}
    }
    gesture^={edits=edits,allocator=app.world.allocator,active=true}; success=true
    return .None
}
@(private="package")
material_gesture_preflight :: proc(app:^Authoring,gesture:^Material_Gesture)->editor.Scene_Error {
    if !gesture.active { return .Invalid_Operation }
    if app.mode!=.Editing { return .Editing_Required }
    for edit in gesture.edits {
        current,error:=target_surface(&app.world,edit.entity); if error!=.None { return error }
        if current!=edit.after { return .Invalid_Operation }
    }
    return .None
}
/// Applies one validated preview without creating an undo entry for every pointer movement.
material_gesture_preview :: proc(app:^Authoring,gesture:^Material_Gesture,fields:bit_set[agent.Material_Field],values:agent.Material_Values)->editor.Scene_Error {
    error:=material_gesture_preflight(app,gesture); if error!=.None { return error }
    entities:=make([]ecs.Entity_Id,len(gesture.edits),gesture.allocator); defer delete(entities,gesture.allocator)
    for edit,i in gesture.edits { entities[i]=edit.entity }
    result,temporary:=material_execute(app,agent.Material_Set{entities=entities,fields=fields,values=values})
    defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&temporary)
    if result.error!=.None { return result.error }
    gesture.changed=false
    for &edit in gesture.edits {
        edit.after,_=target_surface(&app.world,edit.entity)
        if edit.after!=edit.before { gesture.changed=true }
    }
    return .None
}
/// Cancels the whole live gesture atomically, preserving unrelated component edits.
material_gesture_cancel :: proc(app:^Authoring,gesture:^Material_Gesture)->editor.Scene_Error {
    error:=material_gesture_preflight(app,gesture); if error!=.None { return error }
    command:=Material_Command{edits=gesture.edits}
    remaps:=make([dynamic]editor.Entity_Remap,gesture.allocator); defer delete(remaps)
    if restore_error:=material_command_apply(&command,&app.world,&app.registry,false,&remaps); restore_error!=.None { return restore_error }
    material_gesture_destroy(gesture)
    return .None
}
/// Commits first-before/last-after as one action in the same history used by agent material tools.
material_gesture_finish :: proc(app:^Authoring,gesture:^Material_Gesture)->editor.Scene_Error {
    error:=material_gesture_preflight(app,gesture); if error!=.None { return error }
    if !gesture.changed { material_gesture_destroy(gesture); return .None }
    allocator:=gesture.allocator
    context.allocator=allocator
    command:=new(Material_Command,allocator)
    command.edits=make([]Material_Edit,len(gesture.edits),allocator); copy(command.edits,gesture.edits)
    entities:=make([]ecs.Entity_Id,len(gesture.edits),allocator); defer delete(entities,allocator)
    for edit,i in gesture.edits { entities[i]=edit.entity }
    group:=editor.undo_group_create(command,{material_command_apply,material_command_destroy,material_command_remap},entities,allocator)
    result:=error_result(&app.world,.None); append(&result.entities,..entities)
    data,marshal_error:=json.marshal(struct { source:string, count:int }{"inspector",len(entities)},allocator=allocator)
    if marshal_error!=nil { editor.tool_result_destroy(&result); editor.undo_group_destroy(&group); return .Decode_Failed }
    result.data=data
    editor.agent_record_action(&app.agent.session,{kind=.Application,tool_name="material",value=data},&result,&group)
    material_gesture_destroy(gesture)
    return .None
}
/// Releases preview storage during scene teardown; normal UI paths finish or cancel first.
material_gesture_destroy :: proc(gesture:^Material_Gesture) { delete(gesture.edits,gesture.allocator); gesture^={} }
