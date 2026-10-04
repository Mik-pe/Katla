//! File mutation envelopes retain owned UTF-8 arguments until owner-thread execution.
package assets

import "core:encoding/json"
import "core:mem"
import "core:unicode/utf8"

Resource_Write_Action :: enum { Create, Write }
Resource_Write_Request :: struct { action:Resource_Write_Action,path,content,template:string,has_template:bool }
Resource_Write_Decoded :: struct { request:Resource_Write_Request,tree:json.Value,allocator:mem.Allocator }

/// Releases the request's parsed string storage.
resource_write_destroy :: proc(decoded:^Resource_Write_Decoded) { context.allocator=decoded.allocator; json.destroy_value(decoded.tree); decoded^={} }

/// Admits only create_resource/write_resource and their Rust application arguments.
resource_write_decode :: proc(tool:string,data:[]byte,allocator:=context.allocator)->(Resource_Write_Decoded,Error) {
    context.allocator=allocator
    tree,err:=json.parse(data,spec=.JSON,parse_integers=true,allocator=allocator)
    if err!=nil { return {},.Invalid_JSON }
    decoded:=Resource_Write_Decoded{tree=tree,allocator=allocator}
    success:=false; defer { if !success { resource_write_destroy(&decoded) } }
    fields,valid:=tree.(json.Object); if !valid { return {},.Invalid_Arguments }
    switch tool {
    case "create_resource": decoded.request.action=.Create
    case "write_resource": decoded.request.action=.Write
    case: return {},.Invalid_Arguments
    }
    for name,value in fields {
        if name!="path" && name!="content" && (name!="template" || decoded.request.action!=.Create) { return {},.Invalid_Arguments }
        if _,is_null:=value.(json.Null); is_null && decoded.request.action==.Create && name!="path" { continue }
        text,is_string:=value.(string); if !is_string || !utf8.valid_string(text) { return {},.Invalid_Arguments }
        switch name {
        case "path": decoded.request.path=text
        case "content": decoded.request.content=text
        case "template": decoded.request.template=text; decoded.request.has_template=true
        }
    }
    if decoded.request.path=="" { return {},.Invalid_Arguments }
    if decoded.request.action==.Write { if _,present:=fields["content"]; !present { return {},.Invalid_Arguments } }
    success=true; return decoded,.None
}
