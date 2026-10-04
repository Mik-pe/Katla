//! Keyword resource generation retains an owned request until scene-owner execution.
package agent

import "core:encoding/json"
import "core:mem"
import "core:unicode/utf8"

Resource_Generation_Kind :: enum { Particle_System, Scene }
/// Produces a new project-relative JSON definition without replacing existing files.
Resource_Generation_Request :: struct { path,description:string,kind:Resource_Generation_Kind }
Decoded_Resource_Generation :: struct { request:Resource_Generation_Request,tree:json.Value,allocator:mem.Allocator }
/// Releases parsed strings using their captured owner.
resource_generation_destroy :: proc(decoded:^Decoded_Resource_Generation) { context.allocator=decoded.allocator; json.destroy_value(decoded.tree); decoded^={} }
/// Validates the advertised generation contract before mailbox admission.
resource_generation_decode :: proc(data:[]byte,allocator:=context.allocator)->(Decoded_Resource_Generation,Call_Error) {
    context.allocator=allocator
    if len(data)>1<<20 || !utf8.valid_string(string(data)) { return {},.Invalid_JSON }
    tree,error:=json.parse(data,spec=.JSON,allocator=allocator); if error!=nil { return {},.Invalid_JSON }
    result:=Decoded_Resource_Generation{tree=tree,allocator=allocator}; accepted:=false; defer { if !accepted { resource_generation_destroy(&result) } }
    object,valid:=tree.(json.Object); if !valid || len(object)!=3 { return {},.Invalid_Arguments }
    for key in object { if key!="path" && key!="resource_type" && key!="description" { return {},.Invalid_Arguments } }
    path,is_path:=object["path"].(string); description,is_description:=object["description"].(string); kind,is_kind:=object["resource_type"].(string)
    if !is_path || len(path)==0 || len(path)>4096 || !is_description || len(description)>65536 || !is_kind { return {},.Invalid_Arguments }
    switch kind {
    case "particle_system": result.request.kind=.Particle_System
    case "scene": result.request.kind=.Scene
    case: return {},.Invalid_Arguments
    }
    result.request.path=path; result.request.description=description; accepted=true; return result,.None
}
