//! Application file tools use retained project/resource roots and ordinary CPU scene authority.
package app

import asset "../agent/assets"
import resources "../resources"
import ecs "../ecs"
import editor "../editor"
import "core:strings"
import "core:encoding/json"
import "core:path/filepath"

/// Owns independently confined resource and project roots outside persistent scene components.
Asset_Roots :: struct { resource,project:resources.Root }
@(private="package")
asset_roots_destroy :: proc(value:rawptr) { roots:=cast(^Asset_Roots)value; resources.root_destroy(&roots.resource); resources.root_destroy(&roots.project); roots^={} }

/// Opens both roots before publishing their owner; a failure closes every partial handle.
asset_resources_init :: proc(app:^Authoring,project_path,resource_path:string)->resources.Error {
    project,project_error:=resources.root_open(project_path,app.world.allocator); if project_error!=.None { return project_error }
    resource,resource_error:=resources.root_open(resource_path,app.world.allocator)
    if resource_error!=.None { resources.root_destroy(&project); return resource_error }
    project_prefix:=strings.concatenate({project.path,"/"},app.world.allocator); defer delete(project_prefix,app.world.allocator)
    if !strings.has_prefix(resource.path,project_prefix) { resources.root_destroy(&resource); resources.root_destroy(&project); return .Invalid_Path }
    ecs.insert_resource(&app.world,Asset_Roots{resource,project},ecs.Value_Ops{destroy=asset_roots_destroy})
    return .None
}

/// Performs actual bounded filesystem reads/searches; errors are returned before any scene mutation.
asset_execute :: proc(app:^Authoring,request:asset.Request)->(editor.Tool_Result,editor.Undo_Group) {
    context.allocator=app.world.allocator
    result:=error_result(&app.world,.None)
    roots:=ecs.get_resource_mut(&app.world,Asset_Roots)
    if roots==nil { result.error=.Invalid_Operation; return result,{} }
    switch request.action {
    case .Search:
        matches,err:=resources.search(&roots.resource,request.query,request.extensions,request.limit,lowercase=scene_query_lower)
        if err!=.None { result.error=.Invalid_Operation; return result,{} }; defer resources.search_result_destroy(&matches)
        prefix:=roots.resource.path[len(roots.project.path)+1:]
        paths:=make([]string,len(matches.assets),app.world.allocator)
        defer { for path in paths { delete(path,app.world.allocator) }; delete(paths,app.world.allocator) }
        for path,i in matches.assets { paths[i]=strings.concatenate({prefix,"/",path},app.world.allocator) }
        result.data,_=json.marshal(struct { assets,project_paths:[]string,total:int,truncated:bool,root,path_contract:string }{matches.assets[:],paths,matches.total,matches.truncated,roots.resource.path,"Paths are relative to the discovered resources root; pass assets to spawn_model/behavior set_script, project_paths to prefab read/instantiate."},allocator=app.world.allocator)
    case .Read:
        bytes,err:=resources.read_text(&roots.project,request.path)
        if err!=.None { result.error=.Invalid_Operation; return result,{} }; defer delete(bytes,app.world.allocator)
        result.data,_=json.marshal(struct { path,content:string }{request.path,string(bytes)},allocator=app.world.allocator)
    case .List:
        directory:=request.path; if directory=="." { directory="" }; if !resources.valid_relative_path(directory,true) { result.error=.Invalid_Operation; return result,{} }
        matches,err:=resources.search(&roots.project,"",nil,resources.MAX_ENTRIES,directory)
        if err!=.None { result.error=.Invalid_Operation; return result,{} }; defer resources.search_result_destroy(&matches)
        Entry :: struct {name,path:string,size:i64}
        entries:=make([dynamic]Entry,app.world.allocator); defer delete(entries)
        for path in matches.assets {
            if request.has_filter || request.filter!="" {
                extension,has_extension:=resources.file_extension(path); if !has_extension || extension!=request.filter { continue }
            }
            size,size_error:=resources.file_size(&roots.project,path); if size_error!=.None { result.error=.Invalid_Operation; return result,{} }
            append(&entries,Entry{filepath.base(path),path,size})
        }
        path:=request.path; if path=="" { path="." }
        result.data,_=json.marshal(struct {path:string,count:int,entries:[]Entry}{path,len(entries),entries[:]},allocator=app.world.allocator)
    }
    if result.data==nil { result.error=.Decode_Failed }
    return result,{}
}
