//! Browser state inventories real confined assets and preserves selection by durable relative paths.
package asset_browser
import app ".."
import ecs "../../ecs"
import resources "../../resources"
import "core:mem"
import "core:strings"

Selection_Mode :: enum { Replace,Toggle,Range }
Kind :: enum { Model,Prefab,Material,Shader,Script,Image,Font,Audio,Folder,Unknown }
Thumbnail_State :: enum { Pending,Loading,Ready,Failed }
Entry :: struct { name,path:string,kind:Kind,size:i64,thumbnail:Thumbnail_State,thumbnail_texture:u64,thumbnail_width,thumbnail_height:u32,thumbnail_revision:u64,thumbnail_error:bool }
Drag :: struct { path:string,root:app.Mesh_Path_Root,kind:Kind }
Location :: struct { root:app.Mesh_Path_Root,directory:string }
MAX_NAVIGATION_HISTORY :: 128
MAX_DIRECTORY_BYTES :: 128*1024
State :: struct { thumbnail_roots:[2]Thumbnail_Root,thumbnail_inventory_identity,thumbnail_revision:u64,thumbnail_elapsed:f64,back_history,forward_history:[dynamic]Location,owner:^app.Authoring,root:app.Mesh_Path_Root,directory,search,selected,anchor:string,selected_paths,pending_delete:[dynamic]string,entries:[dynamic]Entry,allocator:mem.Allocator,revision:u64 }
/// Classifies exactly the browser formats supported by the Rust editor.
classify :: proc(path:string,directory:=false)->Kind {
    if directory { return .Folder }
    dot:=strings.last_index_byte(path,'.'); if dot<0 { return .Unknown }; extension:=path[dot+1:]
    switch extension {
    case "glb","gltf","stl": return .Model
    case "katmesh","katprefab": return .Prefab
    case "toml": return .Material
    case "wgsl": return .Shader
    case "luau","lua": return .Script
    case "png","jpg","jpeg": return .Image
    case "ttf","otf": return .Font
    case "wav","ogg","mp3","flac": return .Audio
    }
    return .Unknown
}
/// Initializes browser-owned views; filesystem handles remain with the application.
init :: proc(state:^State,owner:^app.Authoring,root:app.Mesh_Path_Root=.Resource) { state^={thumbnail_revision=1,back_history=make([dynamic]Location,owner.world.allocator),forward_history=make([dynamic]Location,owner.world.allocator),owner=owner,root=root,allocator=owner.world.allocator,entries=make([dynamic]Entry,owner.world.allocator),selected_paths=make([dynamic]string,owner.world.allocator),pending_delete=make([dynamic]string,owner.world.allocator)} }
@(private="package")
entries_destroy :: proc(entries:^[dynamic]Entry,allocator:mem.Allocator) { for entry in entries^ { delete(entry.name,allocator); delete(entry.path,allocator) }; delete(entries^) }
/// Releases views before the borrowed application roots are destroyed.
destroy :: proc(state:^State) { history_clear(&state.back_history,state.allocator); history_clear(&state.forward_history,state.allocator); delete(state.back_history); delete(state.forward_history); entries_destroy(&state.entries,state.allocator); delete(state.directory,state.allocator); delete(state.search,state.allocator); delete(state.selected,state.allocator); delete(state.anchor,state.allocator); paths_clear(&state.selected_paths,state.allocator); paths_clear(&state.pending_delete,state.allocator); delete(state.selected_paths); delete(state.pending_delete); state^={} }
/// Scans into a complete replacement and retains selection and thumbnail state across background refreshes.
refresh :: proc(state:^State)->resources.Error {
    roots:=ecs.get_resource_mut(&state.owner.world,app.Asset_Roots); if roots==nil || state.root>.Project { return .Invalid_Path }
    root:=&roots.resource; if state.root==.Project { root=&roots.project }
    inventory,error:=resources.list_directory(root,state.directory); if error!=.None { return error }; defer resources.directory_result_destroy(&inventory)
    identity,identity_error:=thumbnail_root_identity(state,root); if identity_error!=.None { return identity_error }
    context.allocator=state.allocator
    next:=make([dynamic]Entry,state.allocator); accepted:=false; defer { if !accepted { entries_destroy(&next,state.allocator) } }
    needle:=strings.to_lower(state.search,state.allocator); defer delete(needle,state.allocator)
    selected_exists:=state.selected==""
    for item in inventory.entries {
        if strings.has_prefix(item.name,".") { continue }
        lower:=strings.to_lower(item.name,state.allocator); defer delete(lower,state.allocator)
        if needle!="" && !strings.contains(lower,needle) { continue }
        path:=strings.clone(item.name,state.allocator)
        if state.directory!="" { delete(path,state.allocator); path=strings.concatenate({state.directory,"/",item.name},state.allocator) }
        entry:=Entry{name=strings.clone(item.name,state.allocator),path=path,kind=classify(item.name,item.directory),size=item.size}
        if identity==state.thumbnail_inventory_identity { for previous in state.entries { if previous.path==path {
            entry.thumbnail=previous.thumbnail; entry.thumbnail_texture=previous.thumbnail_texture; entry.thumbnail_width=previous.thumbnail_width; entry.thumbnail_height=previous.thumbnail_height; entry.thumbnail_revision=previous.thumbnail_revision; entry.thumbnail_error=previous.thumbnail_error; break
        } } }
        if path==state.selected { selected_exists=true }
        append(&next,entry)
    }
    entries_destroy(&state.entries,state.allocator); state.entries=next; accepted=true; state.revision+=1; state.thumbnail_inventory_identity=identity; thumbnail_freshen(state)
    if !selected_exists { delete(state.selected,state.allocator); state.selected="" }
    selection_refresh(state)
    return .None
}
@(private="package")
history_clear :: proc(history:^[dynamic]Location,allocator:mem.Allocator) { for entry in history^ { delete(entry.directory,allocator) }; clear(history) }
@(private="package")
history_push :: proc(history:^[dynamic]Location,location:Location,allocator:mem.Allocator) {
    if len(history^)==MAX_NAVIGATION_HISTORY { delete(history^[0].directory,allocator); ordered_remove(history,0) }
    append(history,Location{location.root,strings.clone(location.directory,allocator)})
}
@(private="package")
location_admit :: proc(state:^State,location:Location)->resources.Error {
    if location.root>.Project || len(location.directory)>MAX_DIRECTORY_BYTES || !resources.valid_relative_path(location.directory,allow_empty=true) { return .Invalid_Path }
    next:=strings.clone(location.directory,state.allocator); previous:=Location{state.root,state.directory}; state.root=location.root; state.directory=next
    error:=refresh(state)
    if error!=.None { state.root=previous.root; state.directory=previous.directory; delete(next,state.allocator); return error }
    delete(previous.directory,state.allocator); return .None
}
/// Publishes a new location and its history only after genuine directory enumeration succeeds.
navigate_root :: proc(state:^State,root:app.Mesh_Path_Root,path:string)->resources.Error {
    if root==state.root && path==state.directory { return refresh(state) }
    previous:=Location{state.root,strings.clone(state.directory,state.allocator)}; defer delete(previous.directory,state.allocator)
    if error:=location_admit(state,{root,path}); error!=.None { return error }
    history_push(&state.back_history,previous,state.allocator); history_clear(&state.forward_history,state.allocator); return .None
}
/// Ordinary folder navigation retains the current confined resource root.
navigate :: proc(state:^State,path:string)->resources.Error { return navigate_root(state,state.root,path) }
can_back :: proc(state:^State)->bool { return len(state.back_history)>0 }
can_forward :: proc(state:^State)->bool { return len(state.forward_history)>0 }
@(private="package")
history_navigate :: proc(state:^State,backward:bool)->resources.Error {
    source:=&state.back_history; destination:=&state.forward_history; if !backward { source=&state.forward_history; destination=&state.back_history }
    if len(source^)==0 { return .Invalid_Path }
    previous:=Location{state.root,strings.clone(state.directory,state.allocator)}; defer delete(previous.directory,state.allocator)
    target:=source^[len(source^)-1]
    if error:=location_admit(state,target); error!=.None { return error }
    history_push(destination,previous,state.allocator); delete(target.directory,state.allocator); resize(source,len(source^)-1); return .None
}
/// Deleted or inaccessible historical directories retain both stacks and the accepted inventory.
back :: proc(state:^State)->resources.Error { return history_navigate(state,true) }
forward :: proc(state:^State)->resources.Error { return history_navigate(state,false) }
/// Confines parent navigation to the selected browser root and records the accepted move.
parent :: proc(state:^State)->resources.Error { separator:=strings.last_index_byte(state.directory,'/'); return navigate(state,state.directory[:separator] if separator>=0 else "") }
/// Breadcrumb zero denotes the root; following indices denote genuine path components.
breadcrumb :: proc(state:^State,index:int)->resources.Error {
    if index<0 { return .Invalid_Path }; if index==0 { return navigate(state,"") }
    remaining:=state.directory; found:=0; length:=0
    for segment in strings.split_iterator(&remaining,"/") { if segment=="" { return .Invalid_Path }; found+=1; length+=len(segment); if found==index { return navigate(state,state.directory[:length]) }; length+=1 }
    return .Invalid_Path
}
/// Search owns text before asynchronous UI buffers are released.
search_set :: proc(state:^State,text:string)->resources.Error { next:=strings.clone(text,state.allocator); delete(state.search,state.allocator); state.search=next; return refresh(state) }
/// Selects an existing inventory entry by relative path.
select :: proc(state:^State,path:string)->bool { return select_mode(state,path,.Replace) }
/// Borrows the selected real path for synchronous canonical spawn, preview or drag routing.
drag :: proc(state:^State)->(Drag,bool) { for entry in state.entries { if entry.path==state.selected && entry.kind!=.Folder { return {entry.path,state.root,entry.kind},true } }; return {},false }
