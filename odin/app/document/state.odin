//! Editor document ownership keeps saved authored state independent of runtime preview progress.
package document

import app ".."
import ecs "../../ecs"
import editor "../../editor"
import ron "../../encoding/ron"
import "core:encoding/json"
import "core:mem"
import "core:slice"
import "core:strings"
import "core:fmt"

Action_Kind :: enum { New, Open, Quit }
Action :: struct {kind:Action_Kind,path:string}
Dialog :: enum { None, Open, Save_As, Unsaved, Overwrite, Error }
Response :: enum { Save, Discard, Cancel, Overwrite }
/// A stationary state is installed before editor actions and destroyed before its Authoring owner.
State :: struct { owner:^app.Authoring,dialog:Dialog,path:string,pending:Action,has_pending,quit_requested:bool,last_error:editor.Scene_Error,saved:[]byte,allocator:mem.Allocator }
@(private="package")
Token :: struct {state:^State,saved:[]byte,allocator:mem.Allocator}
@(private="package")
normalized :: proc(owner:^app.Authoring,snapshot:^app.Scene_Snapshot)->([]byte,editor.Scene_Error) {
    context.allocator=owner.world.allocator
    tree,error:=app.scene_document_encode(owner,snapshot,"Untitled",""); if error!=.None { return nil,error }; defer json.destroy_value(tree)
    object:=tree.(json.Object); entities:=object["entities"].(json.Array)
    slice.sort_by(entities[:],proc(a,b:json.Value)->bool { left,_:=app.scene_document_key(a.(json.Object)["id"]); right,_:=app.scene_document_key(b.(json.Object)["id"]); return left<right })
    for entity in entities {
        if animation,is_animation:=entity.(json.Object)["animation"].(json.Object); is_animation {
            for field in ([8]string{"duration","target_duration","time","target_time","loop_count","target_loop_count","blend_time","blend_weight"}) { if previous,present:=animation[field]; present { json.destroy_value(previous); animation[field]=json.Integer(0) } }
            for field in ([2]string{"completed","target_completed"}) { if previous,present:=animation[field]; present { json.destroy_value(previous); animation[field]=false } }
        }
    }
    bytes,write_error:=ron.write_json(entities); if write_error.kind!=.None { return nil,.Decode_Failed }; return bytes,.None
}
@(private="package")
observe_prepare :: proc(state:rawptr,owner:^app.Authoring,snapshot:^app.Scene_Snapshot)->(rawptr,editor.Scene_Error) {
    document:=cast(^State)state; if document.owner!=owner { return nil,.Invalid_Operation }
    bytes,error:=normalized(owner,snapshot); if error!=.None { return nil,error }
    token:=new(Token,document.allocator); token^={document,bytes,document.allocator}; return token,.None
}
@(private="package")
observe_finish :: proc(state,prepared:rawptr,committed:bool) {
    document:=cast(^State)state; token:=cast(^Token)prepared; assert(document==token.state)
    if committed { delete(document.saved,document.allocator); document.saved=token.saved; token.saved=nil }
    delete(token.saved,token.allocator); free(token,token.allocator)
}
/// Installs baseline observation on every canonical UI or agent scene publication.
init :: proc(document:^State,owner:^app.Authoring)->editor.Scene_Error {
    if document.owner!=nil || ecs.contains_resource(&owner.world,app.Scene_File_Observer) { return .Invalid_Operation }
    snapshot,error:=app.scene_snapshot_capture(owner,commit_identity=false); if error!=.None { return error }; defer app.scene_snapshot_destroy(&snapshot)
    bytes,encode_error:=normalized(owner,&snapshot); if encode_error!=.None { return encode_error }
    document^={owner=owner,saved=bytes,allocator=owner.world.allocator}
    ecs.insert_resource(&owner.world,app.Scene_File_Observer{document,observe_prepare,observe_finish}); return .None
}
/// Releases pending paths and baseline storage after detaching the application observer.
destroy :: proc(document:^State) {
    if document.owner!=nil { observer,installed:=ecs.get_resource(&document.owner.world,app.Scene_File_Observer); if installed && observer.state==document { ecs.remove_resource(&document.owner.world,app.Scene_File_Observer) } }
    delete(document.path,document.allocator); delete(document.pending.path,document.allocator); delete(document.saved,document.allocator); document^={}
}
/// Compares actual authored state; playback timing and failed capture cannot erase an unsaved edit.
dirty :: proc(document:^State)->bool {
    if document.owner==nil { return true }; owner:=document.owner
    captured:app.Scene_Snapshot; defer app.scene_snapshot_destroy(&captured); snapshot:^app.Scene_Snapshot
    if runtime:=ecs.get_resource_mut(&owner.world,app.Simulation_Runtime); owner.mode!=.Editing && runtime!=nil && runtime.captured { snapshot=&runtime.snapshot }
    else { value,error:=app.scene_snapshot_capture(owner,commit_identity=false); if error!=.None { return true }; captured=value; snapshot=&captured }
    bytes,error:=normalized(owner,snapshot); if error!=.None { return true }; defer delete(bytes,document.allocator)
    return mem.compare(bytes,document.saved)!=0
}
/// Returns an owned scene title carrying the unsaved authored-state indicator.
title :: proc(document:^State)->string { context.allocator=document.allocator; name:="Untitled"; if state,present:=ecs.get_resource(&document.owner.world,app.Scene_File_State); present { name=state.name }; return fmt.aprintf("%s%s",name,"*" if dirty(document) else "") }
@(private="package")
set_path :: proc(document:^State,path:string) { delete(document.path,document.allocator); document.path=strings.clone(path,document.allocator) }
@(private="package")
fail :: proc(document:^State,error:editor.Scene_Error)->editor.Scene_Error { document.last_error=error; document.dialog=.Error; return error }
