//! Asset selection and confirmed filesystem actions resolve every real path through the owning root.
package asset_browser
import app ".."
import ecs "../../ecs"
import resources "../../resources"
import "core:strings"
import "core:mem"

Drag_Batch :: struct { items:[]Drag,allocator:mem.Allocator }
Delete_Result :: struct { removed:int,error:resources.Error }
@(private="package")
paths_clear :: proc(paths:^[dynamic]string,allocator:mem.Allocator) { for path in paths^ { delete(path,allocator) }; clear(paths) }
@(private="package")
entry_index :: proc(state:^State,path:string)->(int,bool) { for entry,i in state.entries { if entry.path==path { return i,true } }; return 0,false }
@(private="package")
selection_refresh :: proc(state:^State) {
    for i:=len(state.selected_paths)-1;i>=0;i-=1 { if _,exists:=entry_index(state,state.selected_paths[i]); !exists { delete(state.selected_paths[i],state.allocator); ordered_remove(&state.selected_paths,i) } }
    if _,exists:=entry_index(state,state.anchor); !exists { delete(state.anchor,state.allocator); state.anchor="" }
    if state.selected=="" && len(state.selected_paths)>0 { state.selected=strings.clone(state.selected_paths[len(state.selected_paths)-1],state.allocator) }
}
/// Range follows displayed ordering; toggle retains independent paths and a stable anchor.
select_mode :: proc(state:^State,path:string,mode:Selection_Mode)->bool {
    index,exists:=entry_index(state,path); if !exists { return false }
    switch mode {
    case .Replace: paths_clear(&state.selected_paths,state.allocator); append(&state.selected_paths,strings.clone(path,state.allocator))
    case .Toggle:
        found:=-1; for item,i in state.selected_paths { if item==path { found=i; break } }
        if found>=0 { delete(state.selected_paths[found],state.allocator); ordered_remove(&state.selected_paths,found) }
        else { append(&state.selected_paths,strings.clone(path,state.allocator)) }
    case .Range:
        anchor,has_anchor:=entry_index(state,state.anchor); if !has_anchor { anchor=index }
        paths_clear(&state.selected_paths,state.allocator)
        for i in min(anchor,index)..=max(anchor,index) { append(&state.selected_paths,strings.clone(state.entries[i].path,state.allocator)) }
    }
    delete(state.selected,state.allocator); state.selected=""
    for item in state.selected_paths { if item==path { state.selected=strings.clone(path,state.allocator); break } }
    if state.selected=="" && len(state.selected_paths)>0 { state.selected=strings.clone(state.selected_paths[len(state.selected_paths)-1],state.allocator) }
    if mode!=.Range { delete(state.anchor,state.allocator); state.anchor=strings.clone(path,state.allocator) }
    return true
}
/// Owns drag paths until the receiving viewport admits the canonical spawn batch.
drag_batch :: proc(state:^State)->Drag_Batch {
    context.allocator=state.allocator
    items:=make([dynamic]Drag,state.allocator); defer delete(items)
    for selected in state.selected_paths { index,exists:=entry_index(state,selected); if exists && state.entries[index].kind!=.Folder { append(&items,Drag{strings.clone(selected,state.allocator),state.root,state.entries[index].kind}) } }
    owned:=make([]Drag,len(items),state.allocator); copy(owned,items[:]); return {owned,state.allocator}
}
drag_batch_destroy :: proc(batch:^Drag_Batch) { for item in batch.items { delete(item.path,batch.allocator) }; delete(batch.items,batch.allocator); batch^={} }
/// Returns the actual project-relative path consumed by canonical Scene_Op Spawn_Model/prefab tools.
project_path :: proc(state:^State,path:string)->(string,bool) {
    if !resources.valid_relative_path(path) { return "",false }
    roots:=ecs.get_resource_mut(&state.owner.world,app.Asset_Roots); if roots==nil { return "",false }
    if state.root==.Project { return strings.clone(path,state.allocator),true }
    prefix:=strings.concatenate({roots.project.path,"/"},state.allocator); defer delete(prefix,state.allocator)
    if !strings.has_prefix(roots.resource.path,prefix) { return "",false }
    return strings.concatenate({roots.resource.path[len(prefix):],"/",path},state.allocator),true
}
@(private="package")
root_for :: proc(state:^State)->^resources.Root { roots:=ecs.get_resource_mut(&state.owner.world,app.Asset_Roots); if roots==nil { return nil }; if state.root==.Resource { return &roots.resource }; if state.root==.Project { return &roots.project }; return nil }
/// Creates a named folder in the displayed directory and refreshes only after actual success.
create_folder :: proc(state:^State,name:string)->resources.Error {
    if !resources.valid_relative_path(name) || strings.contains(name,"/") { return .Invalid_Path }
    root:=root_for(state); if root==nil { return .Invalid_Path }
    path:=strings.clone(name,state.allocator); if state.directory!="" { delete(path,state.allocator); path=strings.concatenate({state.directory,"/",name},state.allocator) }; defer delete(path,state.allocator)
    if error:=resources.create_directory(root,path); error!=.None { return error }; return refresh(state)
}
/// Captures owned deletion targets so later selection changes cannot alter an open confirmation dialog.
delete_request :: proc(state:^State)->bool { paths_clear(&state.pending_delete,state.allocator); for path in state.selected_paths { if _,exists:=entry_index(state,path); exists { append(&state.pending_delete,strings.clone(path,state.allocator)) } }; return len(state.pending_delete)>0 }
/// Cancels deletion without touching any file or active selection.
delete_cancel :: proc(state:^State) { paths_clear(&state.pending_delete,state.allocator) }
/// Executes exactly the confirmed targets; partial filesystem failures report the count already removed.
delete_confirm :: proc(state:^State)->Delete_Result {
    root:=root_for(state); if root==nil { return {error=.Invalid_Path} }
    result:Delete_Result
    for path in state.pending_delete { if error:=resources.remove_path(root,path,recursive=true); error!=.None { result.error=error; break }; result.removed+=1 }
    delete_cancel(state); refresh_error:=refresh(state); if result.error==.None { result.error=refresh_error }; return result
}
