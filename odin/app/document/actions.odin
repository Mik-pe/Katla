//! Deferred document commands share genuine file transactions and explicit destructive-action decisions.
package document

import app ".."
import ecs "../../ecs"
import editor "../../editor"
import resources "../../resources"
import "core:strings"

@(private="package")
clear_pending :: proc(document:^State) { delete(document.pending.path,document.allocator); document.pending={}; document.has_pending=false }
@(private="package")
execute :: proc(document:^State,action:Action)->editor.Scene_Error {
    owner:=document.owner; if owner.mode!=.Editing { return fail(document,.Editing_Required) }
    switch action.kind {
    case .Quit: document.quit_requested=true
    case .Open:
        result,history:=app.scene_file_execute(owner,{action=.Load,path=action.path,has_path=true}); defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&history)
        if result.error!=.None { return fail(document,result.error) }
    case .New:
        snapshot:=app.Scene_Snapshot{next_entity_id=1,allocator=document.allocator}
        if error:=app.scene_snapshot_restore(owner,&snapshot,file_publication=true); error!=.None { return fail(document,error) }
        ecs.remove_resource(&owner.world,app.Scene_File_State)
        session:=&owner.agent.session; next_id,paused,finished:=session.next_id,session.paused,session.finished
        editor.agent_session_destroy(session); editor.agent_session_init(session,document.allocator); session.next_id=next_id; session.paused=paused; session.finished=finished
    }
    document.dialog=.None; document.last_error=.None; return .None
}
/// Requests New, Open or Quit, retaining the current document until any unsaved decision completes.
request :: proc(document:^State,action:Action)->editor.Scene_Error {
    if document.owner==nil { return .Invalid_Operation }; if document.owner.mode!=.Editing { return fail(document,.Editing_Required) }
    if action.kind==.Open && (!resources.valid_relative_path(action.path) || !strings.has_suffix(action.path,".katla")) { return fail(document,.Invalid_Operation) }
    if dirty(document) { clear_pending(document); document.pending={action.kind,strings.clone(action.path,document.allocator)}; document.has_pending=true; document.dialog=.Unsaved; return .None }
    return execute(document,action)
}
/// Opens a path-entry dialog using the actual origin when available.
choose_path :: proc(document:^State,saving:bool)->editor.Scene_Error {
    if document.owner.mode!=.Editing { return fail(document,.Editing_Required) }
    path:="scene.katla"; if state,present:=ecs.get_resource(&document.owner.world,app.Scene_File_State); present { path=state.path }
    set_path(document,path); document.dialog=.Save_As if saving else .Open; document.last_error=.None; return .None
}
/// Saves to an explicit destination or current origin; an untitled scene opens Save As.
save :: proc(document:^State,path:string="")->editor.Scene_Error {
    owner:=document.owner; if owner.mode!=.Editing { return fail(document,.Editing_Required) }
    if path=="" && !ecs.contains_resource(&owner.world,app.Scene_File_State) { return choose_path(document,true) }
    result,history:=app.scene_file_execute(owner,{action=.Save,path=path,has_path=path!=""}); defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&history)
    if result.error!=.None { return fail(document,result.error) }; document.dialog=.None; document.last_error=.None
    if document.has_pending { action:=document.pending; document.pending={}; document.has_pending=false; defer delete(action.path,document.allocator); return execute(document,action) }
    return .None
}
/// Submits a confined scene path; replacing an existing destination requires Overwrite.
submit_path :: proc(document:^State,text:string)->editor.Scene_Error {
    path:=strings.trim_space(text); if len(path)==0 { return .None }
    if !resources.valid_relative_path(path) || !strings.has_suffix(path,".katla") { return fail(document,.Invalid_Operation) }
    mode:=document.dialog
    if mode==.Open { document.dialog=.None; return request(document,{kind=.Open,path=path}) }
    if mode!=.Save_As { return .Invalid_Operation }
    roots:=ecs.get_resource_mut(&document.owner.world,app.Asset_Roots); if roots==nil { return fail(document,.Invalid_Operation) }
    bytes,error:=resources.read_bytes(&roots.project,path,0); delete(bytes,document.allocator)
    if error==.None || error==.Limit { set_path(document,path); document.dialog=.Overwrite; return .None }
    return save(document,path)
}
/// Completes one modal decision through the same publication routines as menus and shortcuts.
respond :: proc(document:^State,response:Response)->editor.Scene_Error {
    switch response {
    case .Cancel: document.dialog=.None; document.last_error=.None; clear_pending(document); return .None
    case .Save: if document.dialog!=.Unsaved { return .Invalid_Operation }; return save(document)
    case .Discard:
        if document.dialog!=.Unsaved || !document.has_pending { return .Invalid_Operation }
        action:=document.pending; document.pending={}; document.has_pending=false; defer delete(action.path,document.allocator); document.dialog=.None; return execute(document,action)
    case .Overwrite: if document.dialog!=.Overwrite { return .Invalid_Operation }; return save(document,document.path)
    }
    return .Invalid_Operation
}
