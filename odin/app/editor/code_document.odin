//! Owned code tabs publish confined source files only after real Luau syntax preflight.
package editor_app

import app ".."
import ecs "../../ecs"
import editor "../../editor"
import resources "../../resources"
import "core:mem"
import "core:strings"
import "core:unicode/utf8"

Code_Dialog :: enum {None,Unsaved}
Code_Response :: enum {Save,Discard,Cancel}
Code_Pending_Kind :: enum {Select,Close,Leave}
Code_Pending :: struct {kind:Code_Pending_Kind,index:int}
/// Each tab retains source identity and its last published baseline independently from selection.
Code_Tab :: struct {root:app.Mesh_Path_Root,path,identity,text,saved:string}
Code_Documents :: struct {owner:^app.Authoring,tabs:[dynamic]Code_Tab,active:int,has_active,leave_requested,leave_ready:bool,pending:Code_Pending,dialog:Code_Dialog,last_error:editor.Scene_Error,message:string,allocator:mem.Allocator}
Code_Save_Result :: struct {published,reloaded:bool,reload_count:int,error:editor.Scene_Error}
/// Initializes text ownership independently from the frontend widget or scene document.
code_documents_init :: proc(documents:^Code_Documents,owner:^app.Authoring) { documents^={owner=owner,allocator=owner.world.allocator,tabs=make([dynamic]Code_Tab,owner.world.allocator)} }
@(private="file")
code_tab_destroy :: proc(tab:^Code_Tab,allocator:mem.Allocator) { delete(tab.path,allocator); delete(tab.identity,allocator); delete(tab.text,allocator); delete(tab.saved,allocator); tab^={} }
/// Releases tabs before the borrowed scene owner is destroyed.
code_documents_destroy :: proc(documents:^Code_Documents) { for &tab in documents.tabs { code_tab_destroy(&tab,documents.allocator) }; delete(documents.tabs); delete(documents.message,documents.allocator); documents^={} }
/// Dirty state compares exact owned source bytes against the last successful publication.
code_document_dirty :: proc(tab:Code_Tab)->bool { return tab.text!=tab.saved }
@(private="file")
code_error :: proc(documents:^Code_Documents,error:editor.Scene_Error)->editor.Scene_Error { documents.last_error=error; return error }
@(private="file")
code_message_clear :: proc(documents:^Code_Documents) { delete(documents.message,documents.allocator); documents.message="" }
/// Opens real UTF-8 Luau source; an explicit File selection grants only that source's exact child capability.
code_document_open :: proc(documents:^Code_Documents,root:app.Mesh_Path_Root,path:string)->editor.Scene_Error {
    if documents.owner==nil || documents.dialog!=.None || len(documents.tabs)>=64 { return code_error(documents,.Invalid_Operation) }
    owner:=documents.owner; allocator:=documents.allocator
    normalized,valid:=app.script_source_name(path,root,allocator); if !valid { return code_error(documents,.Invalid_Field_Value) }; defer delete(normalized,allocator)
    identity,has_identity:=app.asset_absolute_path(owner,root,normalized); if !has_identity { return code_error(documents,.Invalid_Field_Value) }; defer delete(identity,allocator)
    for tab,index in documents.tabs { if tab.identity==identity { return code_document_select(documents,index) } }
    snapshot:=app.Scene_Snapshot{allocator=allocator}; token,token_error:=app.script_sources_prepare(owner,&snapshot); if token_error!=.None { return code_error(documents,token_error) }
    accepted:=false; defer app.script_sources_finish(owner,&token,accepted)
    if root==.File { if error:=app.script_sources_admit(owner,&token,normalized); error!=.None { return code_error(documents,error) } }
    bytes,read_error:=app.script_source_read(owner,{path=normalized,root=root}); if read_error!=.None { return code_error(documents,read_error) }; defer delete(bytes,allocator)
    tab:=Code_Tab{root=root,path=strings.clone(normalized,allocator),identity=strings.clone(identity,allocator),text=strings.clone(string(bytes),allocator),saved=strings.clone(string(bytes),allocator)}
    append(&documents.tabs,tab); accepted=true
    return code_document_select(documents,len(documents.tabs)-1)
}
/// Replaces the active buffer while retaining its published baseline and pending action.
code_document_edit :: proc(documents:^Code_Documents,text:string)->editor.Scene_Error {
    if !documents.has_active || documents.dialog!=.None || !utf8.valid_string(text) || len(text)>1024*1024 { return code_error(documents,.Invalid_Field_Value) }
    for character in text { if character==0 { return code_error(documents,.Invalid_Field_Value) } }
    documents.leave_ready=false; replacement:=strings.clone(text,documents.allocator); tab:=&documents.tabs[documents.active]; delete(tab.text,documents.allocator); tab.text=replacement; return code_error(documents,.None)
}
@(private="file")
code_save_index :: proc(documents:^Code_Documents,index:int)->Code_Save_Result {
    result:Code_Save_Result
    if index<0 || index>=len(documents.tabs) { result.error=code_error(documents,.Invalid_Operation); return result }
    owner:=documents.owner; allocator:=documents.allocator; tab:=&documents.tabs[index]; code_message_clear(documents)
    message,check_error:=app.script_source_check(owner,tab.text,tab.identity); documents.message=message
    if check_error!=.None { result.error=code_error(documents,check_error); return result }
    baseline:=strings.clone(tab.text,allocator); transferred:=false; defer { if !transferred { delete(baseline,allocator) } }
    scope,scope_error:=app.asset_path_scope(owner,tab.root,tab.path); if scope_error!=.None { result.error=code_error(documents,.Invalid_Operation); return result }; defer app.asset_path_scope_destroy(&scope)
    published,write_error:=resources.write_atomic(&scope.root,scope.path,transmute([]byte)tab.text); result.published=published
    if !published { result.error=code_error(documents,.Invalid_Operation); return result }
    delete(tab.saved,allocator); tab.saved=baseline; transferred=true
    if write_error!=.None { result.error=.Invalid_Operation }
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    for entity in ids {
        source,present:=ecs.get_component(&owner.world,entity,app.Script_Component); if !present { continue }
        absolute,valid:=app.asset_absolute_path(owner,source.root,source.path); defer delete(absolute,owner.world.allocator)
        if !valid || absolute!=tab.identity { continue }
        reload_error:=app.script_reload(owner,entity); if reload_error!=.None { result.error=reload_error } else { result.reload_count+=1 }
    }
    result.reloaded=result.reload_count>0 && result.error==.None; code_error(documents,result.error); return result
}
/// Syntax failures preserve file bytes and every live VM; postpublication reload errors report published=true.
code_document_save :: proc(documents:^Code_Documents)->Code_Save_Result {
    if !documents.has_active || documents.dialog!=.None { return {error=code_error(documents,.Invalid_Operation)} }
    return code_save_index(documents,documents.active)
}
/// Requests a different tab; dirty active source requires one explicit Save/Discard/Cancel decision.
code_document_select :: proc(documents:^Code_Documents,index:int)->editor.Scene_Error {
    if documents.dialog!=.None || index<0 || index>=len(documents.tabs) { return code_error(documents,.Invalid_Operation) }
    if documents.has_active && documents.active!=index && code_document_dirty(documents.tabs[documents.active]) { documents.pending={.Select,index}; documents.dialog=.Unsaved; return code_error(documents,.None) }
    documents.active=index; documents.has_active=true; return code_error(documents,.None)
}
@(private="file")
code_close_index :: proc(documents:^Code_Documents,index:int) {
    code_tab_destroy(&documents.tabs[index],documents.allocator)
    for i:=index;i<len(documents.tabs)-1;i+=1 { documents.tabs[i]=documents.tabs[i+1] }
    pop(&documents.tabs)
    if len(documents.tabs)==0 { documents.active=0; documents.has_active=false }
    else if documents.active>index { documents.active-=1 }
    else if documents.active>=len(documents.tabs) { documents.active=len(documents.tabs)-1 }
}
/// Closes only a clean tab; dirty source stays owned while its decision is pending.
code_document_close :: proc(documents:^Code_Documents,index:int)->editor.Scene_Error {
    if documents.dialog!=.None || index<0 || index>=len(documents.tabs) { return code_error(documents,.Invalid_Operation) }
    if code_document_dirty(documents.tabs[index]) { documents.pending={.Close,index}; documents.dialog=.Unsaved; return code_error(documents,.None) }
    code_close_index(documents,index); return code_error(documents,.None)
}
/// Failed Save retains the pending action; Discard restores exact published bytes and Cancel preserves edits.
code_document_respond :: proc(documents:^Code_Documents,response:Code_Response)->editor.Scene_Error {
    if documents.dialog!=.Unsaved { return code_error(documents,.Invalid_Operation) }
    if response==.Cancel { documents.dialog=.None; documents.leave_requested=false; documents.leave_ready=false; return code_error(documents,.None) }
    pending:=documents.pending; index:=documents.active; if pending.kind in (bit_set[Code_Pending_Kind]{.Close,.Leave}) { index=pending.index }
    if response==.Save { saved:=code_save_index(documents,index); if saved.error!=.None { return saved.error } }
    else {
        tab:=&documents.tabs[index]; replacement:=strings.clone(tab.saved,documents.allocator); delete(tab.text,documents.allocator); tab.text=replacement
    }
    documents.dialog=.None
    if pending.kind==.Leave { _,error:=code_documents_request_leave(documents); return error }
    if pending.kind==.Close { code_close_index(documents,pending.index) } else { documents.active=pending.index; documents.has_active=true }
    return code_error(documents,.None)
}

/// Gates application shutdown across every dirty tab; no draft is silently discarded.
code_documents_request_leave :: proc(documents:^Code_Documents)->(ready:bool,error:editor.Scene_Error) {
    if documents.dialog!=.None {
        if documents.pending.kind==.Leave { return false,.None }
        return false,code_error(documents,.Invalid_Operation)
    }
    documents.leave_requested=true; documents.leave_ready=false
    for tab,index in documents.tabs {
        if code_document_dirty(tab) { documents.pending={.Leave,index}; documents.dialog=.Unsaved; documents.active=index; documents.has_active=true; return false,code_error(documents,.None) }
    }
    documents.leave_ready=true; return true,code_error(documents,.None)
}
