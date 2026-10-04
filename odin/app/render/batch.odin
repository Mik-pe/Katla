//! Prepared scene mesh streams are owned CPU upload inputs, separate from authored components.
package render

import app ".."
import ecs "../../ecs"
import editor "../../editor"
import gfx "../../gfx"
import "core:mem"

/// Identity changes trigger explicit native preparation; handles never enter persistent components.
Batch_Entry :: struct { entity:ecs.Entity_Id, vertices,indices:rawptr, vertex_count,index_count:int }
Batch_Error_Kind :: enum { None, Rebuild_Required, Invalid_Geometry, Invalid_Scene, Unsupported_Model }
Batch_Error :: struct { kind:Batch_Error_Kind, scene:editor.Scene_Error }
/// Owns flattened real triangle streams, object slots and draw ranges for the authored world.
Scene_Batch :: struct { geometry:Geometry, entries:[]Batch_Entry, objects:[]Object_Data, draws:[]gfx.Draw_Op, allocator:mem.Allocator, models_supported:bool }
/// Prepares all visible authored meshes atomically; empty mesh sources produce no draw work.
scene_batch_prepare :: proc(owner:^app.Authoring,allocator:=context.allocator)->(Scene_Batch,Batch_Error) {
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    return scene_batch_prepare_entities(owner,ids[:],allocator)
}
/// Prepares an explicit staged replacement without consuming the still-published old scene.
scene_batch_prepare_entities :: proc(owner:^app.Authoring,ids:[]ecs.Entity_Id,allocator:=context.allocator)->(Scene_Batch,Batch_Error) {
    result:=Scene_Batch{allocator=allocator}
    success:=false; defer { if !success { scene_batch_destroy(&result) } }
    entries:=make([dynamic]Batch_Entry,0,len(ids),allocator); defer delete(entries)
    vertices:=make([dynamic]Vertex,allocator); defer delete(vertices)
    objects:=make([dynamic]Object_Data,0,len(ids),allocator); defer delete(objects)
    draws:=make([dynamic]gfx.Draw_Op,0,len(ids),allocator); defer delete(draws)
    for id,i in ids {
        if !ecs.entity_exists(&owner.world,id) { return {},{.Invalid_Scene,.Entity_Not_Found} }
        for previous in ids[:i] { if previous==id { return {},{.Invalid_Scene,.Invalid_Operation} } }
        if _,hidden:=ecs.get_component(&owner.world,id,app.Editor_Hidden); hidden { continue }
        if _,model:=ecs.get_component(&owner.world,id,app.Scene_Model); model { return {},{kind=.Unsupported_Model} }
        mesh,present:=ecs.get_component(&owner.world,id,app.Scene_Mesh); if !present || mesh.source.kind==.Empty { continue }
        object,scene_error:=scene_object_data(owner,id); if scene_error!=.None { return {},{.Invalid_Scene,scene_error} }
        geometry,geometry_error:=geometry_from_mesh(&mesh.geometry,allocator)
        if geometry_error!=.None { return {},{kind=.Invalid_Geometry} }
        if u64(len(vertices))+u64(len(geometry.vertices))>u64(max(u32)) || u64(len(entries))>=u64(max(u32)) { geometry_destroy(&geometry); return {},{kind=.Invalid_Geometry} }
        append(&draws,gfx.Draw{u32(len(geometry.vertices)),1,u32(len(vertices)),u32(len(entries))})
        append(&vertices,..geometry.vertices); geometry_destroy(&geometry)
        append(&objects,object)
        append(&entries,Batch_Entry{id,raw_data(mesh.geometry.vertices),raw_data(mesh.geometry.indices),len(mesh.geometry.vertices),len(mesh.geometry.indices)})
    }
    result.geometry={make([]Vertex,len(vertices),allocator),allocator}; copy(result.geometry.vertices,vertices[:])
    result.entries=make([]Batch_Entry,len(entries),allocator); copy(result.entries,entries[:])
    result.objects=make([]Object_Data,len(objects),allocator); copy(result.objects,objects[:])
    result.draws=make([]gfx.Draw_Op,len(draws),allocator); copy(result.draws,draws[:])
    success=true; return result,{}
}
/// Refreshes factors/placement only when the current CPU mesh revision still matches native inputs.
scene_batch_refresh :: proc(batch:^Scene_Batch,owner:^app.Authoring)->Batch_Error {
    count:int
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    for id in ids {
        if _,hidden:=ecs.get_component(&owner.world,id,app.Editor_Hidden); hidden { continue }
        if _,model:=ecs.get_component(&owner.world,id,app.Scene_Model); model { if batch.models_supported { continue }; return {kind=.Unsupported_Model} }
        mesh,present:=ecs.get_component(&owner.world,id,app.Scene_Mesh)
        if present && mesh.source.kind!=.Empty { count+=1 }
    }
    if count!=len(batch.entries) { return {kind=.Rebuild_Required} }
    for entry in batch.entries {
        mesh,present:=ecs.get_component(&owner.world,entry.entity,app.Scene_Mesh)
        if !present || raw_data(mesh.geometry.vertices)!=entry.vertices || raw_data(mesh.geometry.indices)!=entry.indices || len(mesh.geometry.vertices)!=entry.vertex_count || len(mesh.geometry.indices)!=entry.index_count { return {kind=.Rebuild_Required} }
        if _,hidden:=ecs.get_component(&owner.world,entry.entity,app.Editor_Hidden); hidden { return {kind=.Rebuild_Required} }
        if _,error:=scene_object_data(owner,entry.entity); error!=.None { return {.Invalid_Scene,error} }
    }
    for entry,i in batch.entries { data,_:=scene_object_data(owner,entry.entity); batch.objects[i]=data }
    return {}
}
/// Releases prepared CPU input owners after native immutable upload has retained its own storage.
scene_batch_destroy :: proc(batch:^Scene_Batch) {
    geometry_destroy(&batch.geometry); delete(batch.entries,batch.allocator); delete(batch.objects,batch.allocator); delete(batch.draws,batch.allocator); batch^={}
}
