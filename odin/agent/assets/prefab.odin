//! Reusable asset authoring requests retain complete documents until owner-thread execution.
package assets

import "core:encoding/json"
import "core:mem"

Prefab_Action :: enum { Describe, Read, Validate, Write }
/// Mesh documents are complete authoring input; omitted fields are not implicit patches.
Prefab_Request :: struct { action:Prefab_Action,path:string,document:json.Value }
Decoded_Prefab :: struct { request:Prefab_Request,tree:json.Value,allocator:mem.Allocator }
/// Releases the borrowed document's owning transport tree.
prefab_destroy :: proc(decoded:^Decoded_Prefab) { context.allocator=decoded.allocator; json.destroy_value(decoded.tree); decoded^={} }
/// Validates implemented document operations before the application resolves project paths.
prefab_decode :: proc(data:[]byte,allocator:=context.allocator)->(Decoded_Prefab,Error) {
    context.allocator=allocator
    tree,parse_error:=json.parse(data,spec=.JSON,parse_integers=true,allocator=allocator); if parse_error!=nil { return {},.Invalid_JSON }
    decoded:=Decoded_Prefab{tree=tree,allocator=allocator}; success:=false; defer { if !success { prefab_destroy(&decoded) } }
    object,ok:=tree.(json.Object); if !ok { return {},.Invalid_Arguments }
    action,is_action:=object["action"].(string); if !is_action { return {},.Invalid_Arguments }
    allowed:[]string
    switch action {
    case "describe": decoded.request.action=.Describe; allowed={"action"}
    case "read": decoded.request.action=.Read; allowed={"action","path"}
    case "validate": decoded.request.action=.Validate; allowed={"action","path","document"}
    case "write": decoded.request.action=.Write; allowed={"action","path","document"}
    case: return {},.Invalid_Arguments
    }
    for key in object { found:=false; for name in allowed { if name==key { found=true; break } }; if !found { return {},.Invalid_Arguments } }
    if decoded.request.action!=.Describe {
        path,is_path:=object["path"].(string); if !is_path || len(path)==0 { return {},.Invalid_Arguments }; decoded.request.path=path
    }
    if decoded.request.action==.Validate || decoded.request.action==.Write {
        document,present:=object["document"]; if !present { return {},.Invalid_Arguments }
        if _,is_object:=document.(json.Object); !is_object { return {},.Invalid_Arguments }; decoded.request.document=document
    }
    success=true; return decoded,.None
}
