//! Real katmesh document authoring validates geometry before atomically publishing project assets.
package app

import asset "../agent/assets"
import resources "../resources"
import ron "../encoding/ron"
import editor "../editor"
import ecs "../ecs"
import km "../math"
import "core:encoding/json"
import "core:strings"

/// Executes reusable mesh authoring through the same retained roots used by live asset discovery.
asset_authoring_execute :: proc(app:^Authoring,request:asset.Prefab_Request)->(editor.Tool_Result,editor.Undo_Group) {
    context.allocator=app.world.allocator; result:=error_result(&app.world,.None)
    if request.action==.Describe {
        text:string=`{"mesh":{"version":1,"name":"Seat","parts":[{"id":"seat","transform":{"position":[0,0.45,0],"rotation":[0,0,0,1],"scale":[1,1,1]},"geometry":{"kind":"cube","size":[0.8,0.1,0.8]}}]},"geometry_kinds":["cube","sphere","plane","cylinder","cone","torus","triangles"],"operations":["describe","read","validate","write","instantiate","capture","remove"],"path_contract":"project-relative .katmesh and project-relative or intentional absolute .katprefab paths; complete documents compile before publication; capture writes a subtree and remove retains shared undo."}`
        result.data=make([]byte,len(text),app.world.allocator); copy(result.data,transmute([]byte)text); return result,{}
    }
    if request.action==.Remove { return asset_remove_prefab(app,request.root_entity) }
    if request.action==.Capture { return asset_capture_prefab(app,request) }
    if strings.has_suffix(request.path,".katprefab") { if !asset_document_path_valid(request.path) { result.error=.Invalid_Operation; return result,{} }; return asset_authoring_prefab(app,request) }
    roots:=ecs.get_resource_mut(&app.world,Asset_Roots)
    if roots==nil || !resources.valid_relative_path(request.path) || !strings.has_suffix(request.path,".katmesh") { result.error=.Invalid_Operation; return result,{} }
    if request.action==.Instantiate { return asset_instantiate_mesh(app,request) }
    document:=request.document
    owned_document:=false
    defer { if owned_document { json.destroy_value(document) } }
    if request.action==.Read {
        bytes,err:=resources.read_text(&roots.project,request.path)
        if err!=.None { result.error=.Invalid_Operation; return result,{} }; defer delete(bytes,app.world.allocator)
        value,parse_error:=ron.parse(string(bytes),app.world.allocator)
        if parse_error.kind!=.None { result.error=.Decode_Failed; return result,{} }; document=value; owned_document=true
    }
    mesh,compile_error:=mesh_recipe_compile(document,app.world.allocator)
    if compile_error!=.None { result.error=.Invalid_Operation; return result,{} }; defer mesh_geometry_destroy(&mesh)
    result.data,_=json.marshal(struct {
        path:string,document:json.Value,vertices,triangles:int,bounds_min,bounds_max:km.Vec3,published:bool,
    }{request.path,document,len(mesh.vertices),len(mesh.indices)/3,km.aabb_min(mesh.bounds),km.aabb_max(mesh.bounds),false},allocator=app.world.allocator)
    if result.data==nil { result.error=.Decode_Failed; return result,{} }
    if request.action==.Write {
        if app.mode!=.Editing { result.error=.Editing_Required; return result,{} }
        published_data,marshal_error:=json.marshal(struct {
            path:string,document:json.Value,vertices,triangles:int,bounds_min,bounds_max:km.Vec3,published:bool,
        }{request.path,document,len(mesh.vertices),len(mesh.indices)/3,km.aabb_min(mesh.bounds),km.aabb_max(mesh.bounds),true},allocator=app.world.allocator)
        if marshal_error!=nil { result.error=.Decode_Failed; return result,{} }
        selected:=false; defer { if !selected { delete(published_data,app.world.allocator) } }
        bytes,write_error:=ron.write(document,app.world.allocator)
        if write_error.kind!=.None { result.error=.Decode_Failed; return result,{} }; defer delete(bytes,app.world.allocator)
        did_publish,error:=resources.write_atomic(&roots.project,request.path,bytes)
        if did_publish { delete(result.data,app.world.allocator); result.data=published_data; selected=true }
        if error!=.None { result.error=.Invalid_Operation }
    }
    return result,{}
}
