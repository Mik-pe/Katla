//! Resource creation and edits publish real files below the retained project handle.
package app

import asset "../agent/assets"
import ecs "../ecs"
import editor "../editor"
import resources "../resources"
import "core:encoding/json"
import "core:fmt"
import "core:unicode/utf8"

/// Uses the existing public templates and makes creation exclusive even across concurrent writers.
resource_write_execute :: proc(owner:^Authoring,request:asset.Resource_Write_Request)->(editor.Tool_Result,editor.Undo_Group) {
    context.allocator=owner.world.allocator
    result:=error_result(&owner.world,.None)
    if owner.mode!=.Editing { result.error=.Editing_Required; return result,{} }
    roots:=ecs.get_resource_mut(&owner.world,Asset_Roots)
    if roots==nil || !resources.valid_relative_path(request.path) || !utf8.valid_string(request.content) { result.error=.Invalid_Operation; return result,{} }
    body:=request.content; generated:[]byte
    defer delete(generated,owner.world.allocator)
    if request.action==.Create && request.has_template {
        switch request.template {
        case "scene": body=`{"version":1,"entities":[]}`
        case "material": body=`{"version":1,"shader":"pbr","properties":{}}`
        case "particle_system": body=`{"version":1,"emitter":{"rate":100.0,"lifetime":[0.5,2.0],"velocity":[0.0,1.0,0.0]}}`
        case:
            generated,_=json.marshal(struct { template:string }{request.template},allocator=owner.world.allocator)
            if generated==nil { result.error=.Decode_Failed; return result,{} }; body=string(generated)
        }
    }
    if len(body)>resources.MAX_BYTES { result.error=.Invalid_Operation; return result,{} }
    if request.action==.Write {
        old,error:=resources.read_bytes(&roots.project,request.path)
        if error!=.None { result.error=.Invalid_Operation; return result,{} }; delete(old,owner.world.allocator)
    }
    message:=fmt.aprintf("Wrote %d bytes to %s",len(body),request.path)
    if request.action==.Create { delete(message,owner.world.allocator); message=fmt.aprintf("Created %s (%d bytes)",request.path,len(body)) }; defer delete(message,owner.world.allocator)
    prepared,marshal_error:=json.marshal(struct { success:bool,message,path:string }{true,message,request.path},allocator=owner.world.allocator)
    if marshal_error!=nil { result.error=.Decode_Failed; return result,{} }
    accepted:=false; defer { if !accepted { delete(prepared,owner.world.allocator) } }
    published:bool; error:resources.Error
    if request.action==.Create { published,error=resources.create_atomic(&roots.project,request.path,transmute([]byte)body) }
    else { published,error=resources.write_atomic(&roots.project,request.path,transmute([]byte)body) }
    if published { result.data=prepared; accepted=true }
    if error!=.None { result.error=.Invalid_Operation }
    return result,{}
}
