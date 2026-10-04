//! Live subtree capture rejects opaque references and stages native resource retirement before removal.
package app

import asset "../agent/assets"
import editor "../editor"
import ecs "../ecs"
import "core:encoding/json"
import "core:strings"
import "core:fmt"

/// Enumerates a live rooted subtree once, retaining full generational identities and rejecting protected children.
scene_subtree :: proc(app:^Authoring,root:ecs.Entity_Id)->([]ecs.Entity_Id,editor.Scene_Error) {
    allocator:=app.world.allocator; context.allocator=allocator
    if !ecs.entity_exists(&app.world,root) { return nil,.Entity_Not_Found }
    if _,hidden:=ecs.get_component(&app.world,root,Editor_Hidden); hidden { return nil,.Protected_Entity }
    ids:=ecs.entity_ids(&app.world); defer delete(ids)
    selected:=make(map[ecs.Entity_Id]bool,allocator); defer delete(selected); selected[root]=true
    children:=make(map[ecs.Entity_Id][dynamic]ecs.Entity_Id,allocator)
    defer { for _,group in children { delete(group) }; delete(children) }
    for id in ids { if parent,has_parent:=ecs.get_component(&app.world,id,Scene_Parent); has_parent { group:=children[parent.entity]; if group.allocator.procedure==nil { group=make([dynamic]ecs.Entity_Id,allocator) }; append(&group,id); children[parent.entity]=group } }
    queue:=make([dynamic]ecs.Entity_Id,allocator); defer delete(queue); append(&queue,root)
    for index:=0;index<len(queue);index+=1 {
        group,has_children:=children[queue[index]]; if !has_children { continue }
        for child in group {
            if selected[child] { return nil,.Invalid_Operation }
            if _,hidden:=ecs.get_component(&app.world,child,Editor_Hidden); hidden { return nil,.Protected_Entity }
            selected[child]=true; append(&queue,child)
            if len(queue)>100_000 { return nil,.Invalid_Operation }
        }
    }
    result:=make([]ecs.Entity_Id,len(queue),allocator); copy(result,queue[:]); return result,.None
}
/// Saves an edited subtree with identity root placement; outside references fail without changing the world or file.
asset_capture_prefab :: proc(app:^Authoring,request:asset.Prefab_Request)->(editor.Tool_Result,editor.Undo_Group) {
    result:=error_result(&app.world,.None); context.allocator=app.world.allocator
    if app.mode!=.Editing { result.error=.Editing_Required; return result,{} }
    if !asset_document_path_valid(request.path) || !strings.has_suffix(request.path,".katprefab") { result.error=.Invalid_Operation; return result,{} }
    ids,err:=scene_subtree(app,request.root_entity); if err!=.None { result.error=err; return result,{} }; defer delete(ids)
    for id in ids { if _,unknown:=ecs.get_component(&app.world,id,Scene_Unknown); unknown { result.error=.Component_Not_Found; return result,{} } }
    snapshot,capture_error:=scene_snapshot_capture(app,subset=ids,detach_root=true,root=request.root_entity,commit_identity=false)
    if capture_error!=.None { result.error=capture_error; return result,{} }; defer scene_snapshot_destroy(&snapshot)
    name:="Prefab"; if label,present:=ecs.get_component(&app.world,request.root_entity,Scene_Name); present { name=label.name }
    scene,encode_error:=scene_document_encode(app,&snapshot,name,request.path); if encode_error!=.None { result.error=encode_error; return result,{} }; defer json.destroy_value(scene)
    root_key:ecs.Entity_Id; found_root:=false
    for row in snapshot.entities { if row.has_source && row.source_entity==request.root_entity { root_key=row.key; found_root=true; break } }
    if !found_root { result.error=.Invalid_Operation; return result,{} }
    document:=make(json.Object,app.world.allocator); defer json.destroy_value(document)
    scene_json_put(&document,"version",json.Integer(1)); scene_json_put(&document,"root",scene_document_key_value(u64(root_key)))
    scene_copy,copy_ok:=scene_value_clone(scene); if !copy_ok { result.error=.Decode_Failed; return result,{} }; scene_json_put(&document,"scene",scene_copy)
    return asset_authoring_prefab(app,{action=.Write,path=request.path,document=document})
}
/// Prepares remaining native resources, then removes one subtree using the canonical owned group history.
asset_remove_prefab :: proc(app:^Authoring,root:ecs.Entity_Id)->(editor.Tool_Result,editor.Undo_Group) {
    allocator:=app.world.allocator; context.allocator=allocator; result:=error_result(&app.world,.None)
    if app.mode!=.Editing { result.error=.Editing_Required; return result,{} }
    ids,err:=scene_subtree(app,root); if err!=.None { result.error=err; return result,{} }; defer delete(ids,allocator)
    text:=fmt.aprintf("%d",u64(root)); defer delete(text,allocator)
    result.data,_=json.marshal(struct {root_entity:string,removed:int}{text,len(ids)},allocator=allocator)
    if result.data==nil { result.error=.Decode_Failed; return result,{} }
    applied,group:=scene_action_execute(app,{kind=.Destroy,entity=root}); defer editor.tool_result_destroy(&applied)
    if applied.error!=.None { result.error=applied.error; return result,group }
    append(&result.entities,..applied.entities[:]); return result,group
}
