//! Mesh components own CPU preparation; snapshots persist reproducible sources and never GPU handles.
package app

import ecs "../ecs"
import editor "../editor"
import "core:mem"
import "core:strings"
import "core:encoding/json"

/// A recipe path is resource-relative; generated geometry carries its complete validated descriptor.
Mesh_Source_Kind :: enum { Empty, Geometry, Recipe }
/// Owns its origin and descriptor; native resource handles remain external application cache state.
Mesh_Source :: struct { kind:Mesh_Source_Kind,path:string,geometry:[]byte }
/// One authored source and its prepared immutable CPU stream, ready for native upload.
Scene_Mesh :: struct { source:Mesh_Source `inspect:"skip"`,geometry:Mesh_Geometry `inspect:"skip"` }

/// Releases source strings and wire geometry with the allocator that owns the prepared mesh.
mesh_source_destroy :: proc(source:^Mesh_Source,allocator:=context.allocator) { delete(source.path,allocator); delete(source.geometry,allocator); source^={} }
@(private="package")
scene_mesh_destroy :: proc(value:rawptr) { mesh:=cast(^Scene_Mesh)value; allocator:=mesh.geometry.allocator; if allocator.procedure==nil { allocator=context.allocator }; mesh_source_destroy(&mesh.source,allocator); mesh_geometry_destroy(&mesh.geometry); mesh^={} }
@(private="package")
scene_mesh_clone :: proc(dst,src:rawptr) {
    source:=cast(^Scene_Mesh)src; result:=cast(^Scene_Mesh)dst
    result.source={source.source.kind,strings.clone(source.source.path),make([]byte,len(source.source.geometry),context.allocator)}; copy(result.source.geometry,source.source.geometry)
    result.geometry=mesh_geometry_clone(&source.geometry,context.allocator)
}

/// Prepares genuine geometry and clones the source before any entity is allocated.
scene_mesh_prepare :: proc(app:^Authoring,source:Mesh_Source)->(Scene_Mesh,Mesh_Error) {
    context.allocator=app.world.allocator
    geometry:Mesh_Geometry
    switch source.kind {
    case .Empty: geometry.allocator=app.world.allocator
    case .Recipe:
        roots:=ecs.get_resource_mut(&app.world,Asset_Roots); if roots==nil || !strings.has_suffix(source.path,".katmesh") { return {},.Invalid_Geometry }
        mesh,err:=mesh_recipe_load(&roots.resource,source.path); if err!=.None { return {},err }; geometry=mesh
    case .Geometry:
        tree,parse_error:=json.parse(source.geometry,spec=.JSON,parse_integers=true,allocator=app.world.allocator)
        if parse_error!=nil { return {},.Invalid_Geometry }; defer json.destroy_value(tree)
        object,is_object:=tree.(json.Object); if !is_object { return {},.Invalid_Geometry }
        if _,_,valid:=recipe_geometry_budget(object); !valid { return {},.Invalid_Geometry }
        mesh,err:=recipe_geometry_compile(object,app.world.allocator); if err!=.None { return {},err }; geometry=mesh
    }
    result:=Scene_Mesh{geometry=geometry,source={kind=source.kind,path=strings.clone(source.path,app.world.allocator),geometry=make([]byte,len(source.geometry),app.world.allocator)}}
    copy(result.source.geometry,source.geometry); return result,.None
}

@(private="package")
scene_mesh_encode :: proc(state,value:rawptr,allocator:mem.Allocator)->([]byte,bool) {
    source:=(cast(^Scene_Mesh)value).source
    switch source.kind {
    case .Empty:
        data,err:=json.marshal(struct { kind:string }{"Empty"},allocator=allocator); return data,err==nil
    case .Recipe:
        data,err:=json.marshal(struct { kind,path:string }{"Recipe",source.path},allocator=allocator); return data,err==nil
    case .Geometry:
        tree,parse_error:=json.parse(source.geometry,spec=.JSON,parse_integers=true,allocator=allocator)
        if parse_error!=nil { return nil,false }; defer json.destroy_value(tree)
        data,err:=json.marshal(struct { kind:string,document:json.Value }{"Geometry",tree},allocator=allocator); return data,err==nil
    }
    return nil,false
}
@(private="package")
scene_mesh_decode :: proc(state:rawptr,data:[]byte,allocator:mem.Allocator)->(rawptr,bool) {
    app:=cast(^Authoring)state; context.allocator=allocator
    result:=new(Scene_Mesh,allocator)
    tree,parse_error:=json.parse(data,spec=.JSON,parse_integers=true,allocator=allocator)
    if parse_error!=nil { return result,false }; defer json.destroy_value(tree)
    object,is_object:=tree.(json.Object); if !is_object { return result,false }
    kind,is_kind:=object["kind"].(string); if !is_kind { return result,false }
    source:Mesh_Source
    switch kind {
    case "Empty": if !recipe_keys(object,{"kind"}) { return result,false }; source.kind=.Empty
    case "Recipe":
        if !recipe_keys(object,{"kind","path"}) { return result,false }
        path,is_path:=object["path"].(string); if !is_path { return result,false }; source.kind=.Recipe; source.path=path
    case "Geometry":
        if !recipe_keys(object,{"kind","document"}) { return result,false }; source.kind=.Geometry
        descriptor,marshal_error:=json.marshal(object["document"],allocator=allocator); if marshal_error!=nil { return result,false }; source.geometry=descriptor
    case: return result,false
    }
    defer { if source.kind==.Geometry { delete(source.geometry,allocator) } }
    prepared,prepare_error:=scene_mesh_prepare(app,source)
    if prepare_error!=.None { return result,false }; result^=prepared; return result,true
}

/// Installs source-only persistence and preparation on the stationary application owner.
scene_mesh_register :: proc(app:^Authoring) {
    editor.editor_register(&app.world,&app.registry,"SceneMesh",Scene_Mesh{},ecs.Value_Ops{scene_mesh_destroy,scene_mesh_clone},spawn_default=false)
    entry:=app.registry.entries["SceneMesh"]; entry.value_state=app; entry.encode_owned=scene_mesh_encode; entry.decode_owned=scene_mesh_decode
}
