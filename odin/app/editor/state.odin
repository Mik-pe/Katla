//! Persistent editor state borrows one stationary authored scene and shares its command history.
package editor_app

import app ".."
import ecs "../../ecs"
import editor "../../editor"
import "core:mem"
import "core:strings"

Panel :: enum u64 { Hierarchy=1,Viewport,Inspector,Assets,Co_Creator,Preferences,Particles,Console,Mixer,Timeline,Code }
Selection_Mode :: enum { Replace,Toggle,Range }
Selected_Entity :: struct { entity:ecs.Entity_Id,key:u64,has_key:bool }
/// Presence is explicit because zero is a valid generational entity ID.
Selection :: struct { entries:[dynamic]Selected_Entity,primary,anchor:ecs.Entity_Id,has_primary,has_anchor:bool }
State :: struct {
    owner:^app.Authoring,
    selection:Selection,
    expanded:map[ecs.Entity_Id]bool,
    search:string,
    rows:[dynamic]Hierarchy_Row,
    allocator:mem.Allocator,
    last_error:editor.Scene_Error,
    revision:u64,
}
/// Initializes the CPU editor independently of a window, graphics or font provider.
state_init :: proc(state:^State,owner:^app.Authoring,allocator:=context.allocator) {
    state^={owner=owner,allocator=allocator}
    state.selection.entries=make([dynamic]Selected_Entity,allocator)
    state.expanded=make(map[ecs.Entity_Id]bool,allocator)
    state.rows=make([dynamic]Hierarchy_Row,allocator)
}
/// Releases editor-owned views before the borrowed scene owner is destroyed.
state_destroy :: proc(state:^State) {
    hierarchy_clear(state)
    delete(state.rows); delete(state.selection.entries); delete(state.expanded)
    delete(state.search,state.allocator); state^={}
}
/// Search text owns its allocation and remains valid across input-frame cleanup.
search_set :: proc(state:^State,text:string) {
    next:=strings.clone(text,state.allocator); delete(state.search,state.allocator); state.search=next
}
/// Protects editor infrastructure and rejects stale generations before local selection or actions.
selectable :: proc(state:^State,entity:ecs.Entity_Id)->bool {
    if state.owner==nil || !ecs.entity_exists(&state.owner.world,entity) { return false }
    _,hidden:=ecs.get_component(&state.owner.world,entity,app.Editor_Hidden)
    return !hidden
}
/// Uses the canonical executor and shared undo session for every authored editor command.
execute :: proc(state:^State,op:editor.Scene_Op,selected:[]ecs.Entity_Id=nil)->editor.Scene_Error {
    owner:=state.owner
    if owner==nil { return .Invalid_Operation }
    result,undo:=app.scene_action_execute(owner,op,selected)
    action:=editor.agent_record_action(&owner.agent.session,op,&result,&undo)
    state.last_error=action.result.error
    if action.result.error==.None { state.revision+=1; selection_refresh(state) }
    return action.result.error
}
/// Undo and redo operate on the same history used by tools and grouped gestures.
history_apply :: proc(state:^State,redo:bool)->editor.Scene_Error {
    error:editor.Scene_Error
    if redo { error=app.authoring_redo_last(state.owner) } else { error=app.authoring_undo_last(state.owner) }
    state.last_error=error
    if error==.None { state.revision+=1; selection_refresh(state) }
    return error
}

/// Admits every imported drag item before publishing any entity and records one shared undo command.
execute_batch :: proc(state:^State,operations:[]editor.Scene_Op)->editor.Scene_Error {
    if state.owner==nil || len(operations)==0 { return .Invalid_Operation }
    result,undo:=app.scene_action_execute_batch(state.owner,operations)
    action:=editor.agent_record_action(&state.owner.agent.session,operations[0],&result,&undo)
    state.last_error=action.result.error
    if state.last_error==.None { state.revision+=1; if len(action.result.entities)>0 { selection_set(state,action.result.entities[0]) }; selection_refresh(state) }
    return state.last_error
}
