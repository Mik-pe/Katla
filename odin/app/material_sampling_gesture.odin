//! Continuous role sampling previews share atomic application admission and one final history action.
package app

import agent "../agent"
import ecs "../ecs"
import editor "../editor"
import "core:mem"

/// Retains initial entity snapshots while each admitted preview updates its accepted endpoint.
Material_Sampling_Gesture :: struct { scene:Scene_Gesture,role:agent.Material_Texture_Role }
material_sampling_gesture_begin :: proc(owner:^Authoring,gesture:^Material_Sampling_Gesture,entities:[]ecs.Entity_Id,role:agent.Material_Texture_Role)->editor.Scene_Error {
    if gesture.scene.active || int(role)<0 || int(role)>=5 { return .Invalid_Operation }
    if error:=material_actor_targets(owner,entities); error!=.None { return error }
    if error:=scene_gesture_begin(owner,&gesture.scene,entities); error!=.None { return error }; gesture.role=role; return .None
}
/// Uses the exact same typed patch and native preflight as the material tool.
material_sampling_gesture_preview :: proc(owner:^Authoring,gesture:^Material_Sampling_Gesture,patch:agent.Material_Sampling_Patch)->editor.Scene_Error {
    if error:=scene_gesture_preflight(owner,&gesture.scene); error!=.None { return error }
    ids:=make([]ecs.Entity_Id,len(gesture.scene.command.rows),owner.world.allocator); defer delete(ids,owner.world.allocator)
    for row,index in gesture.scene.command.rows { ids[index]=row.entity }
    result,group:=material_sampling_execute_internal(owner,{ids,gesture.role,patch},false); defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&group)
    if result.error!=.None { return result.error }
    entry:=owner.registry.entries["SurfaceMaterial"]
    for &row in gesture.scene.command.rows {
        current:=ecs.component_address(&owner.world,row.entity,entry.T)
        for &snapshot in row.after { if snapshot.entry==entry {
            if entry.ops.destroy!=nil { entry.ops.destroy(snapshot.value) }; mem.free(snapshot.value,owner.world.allocator)
            snapshot.value=editor.editor_clone_value(entry,current,owner.world.allocator); break
        } }
        protected:=false; for component in gesture.scene.edited { if component.entity==row.entity && component.entry==entry { protected=true; break } }
        if !protected { append(&gesture.scene.edited,Scene_Gesture_Component{row.entity,entry}) }
    }
    return .None
}
/// Restores the initial role state through the same shared restoration participant.
material_sampling_gesture_cancel :: proc(owner:^Authoring,gesture:^Material_Sampling_Gesture)->editor.Scene_Error { error:=scene_gesture_cancel(owner,&gesture.scene); if error==.None { gesture^={} }; return error }
/// Records one command for all accepted previews, clearing the gesture only on success.
material_sampling_gesture_finish :: proc(owner:^Authoring,gesture:^Material_Sampling_Gesture)->editor.Scene_Error { error:=scene_gesture_finish(owner,&gesture.scene); if error==.None { gesture^={} }; return error }
material_sampling_gesture_destroy :: proc(gesture:^Material_Sampling_Gesture) { scene_gesture_destroy(&gesture.scene); gesture^={} }
