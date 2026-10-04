//! Canonical load_scene and save_scene envelopes carry only explicit confined project paths.
package assets

import "core:encoding/json"
import "core:mem"

Scene_File_Action :: enum { Load, Save }
Scene_File_Request :: struct { action:Scene_File_Action,path:string,has_path:bool }
Decoded_Scene_File :: struct {request:Scene_File_Request,tree:json.Value,allocator:mem.Allocator}
/// Releases borrowed transport path storage.
scene_file_destroy :: proc(decoded:^Decoded_Scene_File) { context.allocator=decoded.allocator; json.destroy_value(decoded.tree); decoded^={} }
/// Validates the two established scene file tools before the owner resolves the active scene origin.
scene_file_decode :: proc(tool:string,data:[]byte,allocator:=context.allocator)->(Decoded_Scene_File,Error) {
    context.allocator=allocator
    tree,parse_error:=json.parse(data,spec=.JSON,parse_integers=true,allocator=allocator); if parse_error!=nil { return {},.Invalid_JSON }
    decoded:=Decoded_Scene_File{tree=tree,allocator=allocator}; success:=false; defer { if !success { scene_file_destroy(&decoded) } }
    object,valid:=tree.(json.Object); if !valid { return {},.Invalid_Arguments }
    switch tool {
    case "load_scene": decoded.request.action=.Load
    case "save_scene": decoded.request.action=.Save
    case: return {},.Invalid_Arguments
    }
    for key in object { if key!="path" { return {},.Invalid_Arguments } }
    if value,present:=object["path"]; present { if _,is_null:=value.(json.Null); is_null && decoded.request.action==.Save { success=true; return decoded,.None }; path,is_path:=value.(string); if !is_path || len(path)==0 { return {},.Invalid_Arguments }; decoded.request.path=path; decoded.request.has_path=true }
    if decoded.request.action==.Load && !decoded.request.has_path { return {},.Invalid_Arguments }
    success=true; return decoded,.None
}
