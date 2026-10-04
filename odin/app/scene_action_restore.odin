//! Shared history prepares genuine application resources before publishing component restoration.
package app

import ecs "../ecs"
import editor "../editor"
import "core:mem"

@(private="package")
scene_action_restore_prepare :: proc(state:rawptr,w:^ecs.World,reg:^editor.Component_Registry,changed,removed:[]ecs.Entity_Id)->(rawptr,editor.Scene_Error) {
    owner:=cast(^Authoring)state
    assert(w==&owner.world && reg==&owner.registry)
    if owner.mode!=.Editing { return nil,.Editing_Required }
    for targets in ([2][]ecs.Entity_Id{changed,removed}) { for id in targets { if _,hidden:=ecs.get_component(w,id,Editor_Hidden); hidden { return nil,.Protected_Entity } } }
    ids:=ecs.entity_ids(w); defer delete(ids)
    visible:=make([dynamic]ecs.Entity_Id,w.allocator); defer delete(visible)
    admitted:=make(map[ecs.Entity_Id]ecs.Entity_Id,w.allocator); defer delete(admitted)
    for id in ids {
        excluded:=false; for target in removed { if id==target { excluded=true; break } }
        if !excluded { admitted[id]=id }
    }
    for id in ids {
        excluded:=false; for target in removed { if id==target { excluded=true; break } }
        if _,hidden:=ecs.get_component(w,id,Editor_Hidden); hidden { excluded=true }
        if excluded { continue }
        if _,has_parent:=ecs.get_component(w,id,Scene_Parent); has_parent {
            current:=id
            path:=make(map[ecs.Entity_Id]bool,w.allocator); defer delete(path)
            for {
                if path[current] { return nil,.Invalid_Operation }; path[current]=true
                for target in removed { if current==target { return nil,.Invalid_Operation } }
                if !ecs.entity_exists(w,current) { return nil,.Entity_Not_Found }
                parent,exists:=ecs.get_component(w,current,Scene_Parent); if !exists { break }; current=parent.entity
            }
        }
        append(&visible,id)
    }
    for id in visible { for _,entry in reg.entries {
        if !entry.has_references { continue }
        if value:=ecs.component_address(w,id,entry.T); value!=nil {
            clone:=editor.editor_clone_value(entry,value,w.allocator)
            valid:=editor.component_map_references(entry,clone,{admitted,true})
            if entry.ops.destroy!=nil { entry.ops.destroy(clone) }; mem.free(clone,w.allocator)
            if !valid { return nil,.Entity_Not_Found }
        }
    } }
    if error:=scene_gameplay_validate_entities(owner,visible[:]); error!=.None { return nil,error }
    if error:=light_scene_validate(owner,visible[:]); error!=.None { return nil,error }
    if error:=audio_scene_validate(owner,visible[:]); error!=.None { return nil,error }
    if error:=perspective_scene_validate(owner,visible[:]); error!=.None { return nil,error }
    if error:=billboard_scene_validate(owner,visible[:]); error!=.None { return nil,error }
    mode:=Scene_Preparation_Mode.Insert; entities:=changed
    if len(removed)>0 { mode=.Remove; entities=removed }
    preparation,error:=scene_prepare_begin(owner,entities,mode)
    if error!=.None { return nil,error }
    if !preparation.active { return nil,.None }
    token:=new(Scene_Preparation,w.allocator); token^=preparation
    return token,.None
}
@(private="package")
scene_action_restore_finish :: proc(state,token:rawptr,commit:bool) {
    if token==nil { return }
    owner:=cast(^Authoring)state; preparation:=cast(^Scene_Preparation)token
    scene_prepare_finish(preparation,commit); mem.free(preparation,owner.world.allocator)
}
@(private="package")
scene_action_restore_install :: proc(owner:^Authoring) {
    ecs.insert_resource(&owner.world,editor.Restoration_Participant{owner,scene_action_restore_prepare,scene_action_restore_finish})
}
