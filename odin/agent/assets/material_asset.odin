//! Reusable material requests retain one complete document until application execution.
package assets

import "core:encoding/json"
import "core:mem"
import ecs "../../ecs"
import ron "../../encoding/ron"

Material_Asset_Action :: enum { Describe, Read, Validate, Write, Capture, Apply }
/// Paths are project-relative .katmat destinations; documents are complete replacements.
Material_Asset_Request :: struct { action:Material_Asset_Action,path:string,document:json.Value,entity:ecs.Entity_Id,entities:[]ecs.Entity_Id }
Decoded_Material_Asset :: struct { request:Material_Asset_Request,tree:json.Value,allocator:mem.Allocator }
/// Releases the request tree and its exact generational target array.
material_asset_destroy :: proc(decoded:^Decoded_Material_Asset) {
    context.allocator=decoded.allocator
    delete(decoded.request.entities,decoded.allocator); json.destroy_value(decoded.tree); decoded^={}
}
/// Admits only the six discriminated material operations and complete decimal identifiers.
material_asset_decode :: proc(data:[]byte,allocator:=context.allocator)->(Decoded_Material_Asset,Error) {
    context.allocator=allocator
    tree,parse_error:=ron.parse_json(data,allocator); if parse_error.kind!=.None { return {},.Invalid_JSON }
    decoded:=Decoded_Material_Asset{tree=tree,allocator=allocator}
    accepted:=false; defer { if !accepted { material_asset_destroy(&decoded) } }
    object,is_object:=tree.(json.Object); if !is_object { return {},.Invalid_Arguments }
    action,is_action:=object["action"].(string); if !is_action { return {},.Invalid_Arguments }
    allowed:[]string
    switch action {
    case "describe": decoded.request.action=.Describe; allowed={"action"}
    case "read": decoded.request.action=.Read; allowed={"action","path"}
    case "validate": decoded.request.action=.Validate; allowed={"action","path","document"}
    case "write": decoded.request.action=.Write; allowed={"action","path","document"}
    case "capture": decoded.request.action=.Capture; allowed={"action","path","entity_id"}
    case "apply": decoded.request.action=.Apply; allowed={"action","path","entity_ids"}
    case: return {},.Invalid_Arguments
    }
    for key in object { found:=false; for name in allowed { if name==key { found=true; break } }; if !found { return {},.Invalid_Arguments } }
    if decoded.request.action!=.Describe {
        path,is_path:=object["path"].(string); if !is_path || len(path)==0 { return {},.Invalid_Arguments }; decoded.request.path=path
    }
    if decoded.request.action==.Validate || decoded.request.action==.Write {
        document,is_document:=object["document"].(json.Object); if !is_document { return {},.Invalid_Arguments }; decoded.request.document=document
    }
    if decoded.request.action==.Capture {
        id,valid:=material_asset_entity(object["entity_id"]); if !valid { return {},.Invalid_Arguments }; decoded.request.entity=id
    }
    if decoded.request.action==.Apply {
        entities,is_entities:=object["entity_ids"].(json.Array); if !is_entities || len(entities)==0 || len(entities)>256 { return {},.Invalid_Arguments }
        decoded.request.entities=make([]ecs.Entity_Id,len(entities),allocator)
        for value,index in entities {
            id,valid:=material_asset_entity(value); if !valid { return {},.Invalid_Arguments }
            for previous in decoded.request.entities[:index] { if previous==id { return {},.Invalid_Arguments } }
            decoded.request.entities[index]=id
        }
    }
    accepted=true; return decoded,.None
}
@(private="package")
material_asset_entity :: proc(value:json.Value)->(ecs.Entity_Id,bool) {
    text,is_text:=value.(string); if !is_text { return {},false }
    number,valid:=ron.decimal_u64(text); return ecs.Entity_Id(number),valid
}
