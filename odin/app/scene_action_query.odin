//! Application-aware queries describe the full authored scene, including geometry beyond the camera frustum.
package app

import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:mem"
import "core:fmt"
import "core:strings"
import "core:slice"
import "core:encoding/json"

@(private="package")
Scene_Query_Row :: struct { entity_id:string, name:Maybe(string), position:Maybe(km.Vec3), bounds:Maybe(km.AABB), parent_id:Maybe(string), components:[]string }
@(private="package")
Scene_Query_Record :: struct { row:Scene_Query_Row, parent:string, components:[dynamic]string }

@(private="package")
scene_action_query :: proc(owner:^Authoring,op:editor.Scene_Op)->editor.Tool_Result {
    context.allocator=owner.world.allocator
    result:=error_result(&owner.world,.None)
    if op.has_radius!=op.has_query_position || (op.has_radius && (!mesh_finite(op.radius) || op.radius<0)) { result.error=.Invalid_Operation; return result }
    if op.has_query_position { for v in op.position { if !mesh_finite(v) { result.error=.Invalid_Operation; return result } } }
    entry:=owner.registry.entries[op.component]
    if op.component!="" && entry==nil { result.error=.Component_Not_Found; return result }
    ids:=ecs.entity_ids(&owner.world); defer delete(ids); slice.sort(ids[:])
    if op.kind==.Get_Hierarchy { return scene_action_hierarchy(owner,ids[:],&result) }
    limit:=64; if op.limit>0 { limit=clamp(op.limit,1,256) }
    records:=make([]Scene_Query_Record,min(limit,len(ids)),owner.world.allocator)
    defer {
        for &record in records { delete(record.row.entity_id,owner.world.allocator); delete(record.parent,owner.world.allocator); delete(record.components) }
        delete(records,owner.world.allocator)
    }
    rows:=make([dynamic]Scene_Query_Row,owner.world.allocator); defer delete(rows)
    names:=editor.editor_type_names(&owner.registry); defer delete(names)
    filter:=scene_query_lower(op.name_filter,owner.world.allocator); defer delete(filter,owner.world.allocator)
    total:=0
    for id in ids {
        if _,hidden:=ecs.get_component(&owner.world,id,Editor_Hidden); hidden { continue }
        if entry!=nil && ecs.component_address(&owner.world,id,entry.T)==nil { continue }
        name,has_name:=ecs.get_component(&owner.world,id,Scene_Name)
        if op.has_name_filter || op.name_filter!="" {
            if !has_name { continue }
            lowered:=scene_query_lower(name.name,owner.world.allocator)
            matches:=strings.contains(lowered,filter); delete(lowered,owner.world.allocator)
            if !matches { continue }
        }
        world_matrix,transform_error:=scene_world_matrix(owner,id)
        has_position:=transform_error==.None
        position:km.Vec3; bounds:km.AABB; has_bounds:bool
        if has_position {
            for column in world_matrix { for value in column { if !mesh_finite(value) { result.error=.Invalid_Operation; clear(&result.entities); return result } } }
            position={world_matrix[3][0],world_matrix[3][1],world_matrix[3][2]}
            bounds,has_bounds,transform_error=scene_drawable_bounds(owner,id)
            if transform_error!=.None { result.error=transform_error; clear(&result.entities); return result }
        }
        if op.has_query_position {
            if !has_position { continue }
            closest:=km.aabb_closest_point(bounds,km.Vec3(op.position)) if has_bounds else position
            delta:=closest-km.Vec3(op.position)
            squared:=f64(delta[0])*f64(delta[0])+f64(delta[1])*f64(delta[1])+f64(delta[2])*f64(delta[2])
            if squared>f64(op.radius)*f64(op.radius) { continue }
        }
        total+=1
        if len(rows)>=limit { continue }
        record:=&records[len(rows)]
        record.row.entity_id=scene_action_id_text(id,owner.world.allocator)
        if has_name { record.row.name=name.name }
        if has_position { record.row.position=position }
        if has_bounds { record.row.bounds=bounds }
        if parent,present:=ecs.get_component(&owner.world,id,Scene_Parent); present { record.parent=scene_action_id_text(parent.entity,owner.world.allocator); record.row.parent_id=record.parent }
        record.components=make([dynamic]string,owner.world.allocator)
        for type_name in names { component:=owner.registry.entries[type_name]; if ecs.component_address(&owner.world,id,component.T)!=nil { append(&record.components,type_name) } }
        record.row.components=record.components[:]; append(&rows,record.row); append(&result.entities,id)
    }
    data:=struct { entities:[]Scene_Query_Row, total:int, truncated:bool, spatial_contract:string }{rows[:],total,total>limit,"Distance to render bounds if available, otherwise transform origin. No frustum or room-membership restriction."}
    encoded,marshal_error:=json.marshal(data,allocator=owner.world.allocator)
    result.data=encoded
    if marshal_error!=nil { result.error=.Invalid_Operation; clear(&result.entities) }
    return result
}
@(private="package")
scene_action_hierarchy :: proc(owner:^Authoring,ids:[]ecs.Entity_Id,result:^editor.Tool_Result)->editor.Tool_Result {
    rows:=make([dynamic]struct { id,name,parent_id:string },owner.world.allocator)
    defer { for row in rows { delete(row.id,owner.world.allocator); delete(row.parent_id,owner.world.allocator) }; delete(rows) }
    for id in ids {
        if _,hidden:=ecs.get_component(&owner.world,id,Editor_Hidden); hidden { continue }
        row:struct { id,name,parent_id:string }; row.id=scene_action_id_text(id,owner.world.allocator)
        if name,present:=ecs.get_component(&owner.world,id,Scene_Name); present { row.name=name.name }
        if parent,present:=ecs.get_component(&owner.world,id,Scene_Parent); present { row.parent_id=scene_action_id_text(parent.entity,owner.world.allocator) }
        append(&rows,row); append(&result.entities,id)
    }
    result.data,_=json.marshal(struct {entities:[]struct {id,name,parent_id:string}}{rows[:]},allocator=owner.world.allocator)
    return result^
}
@(private="package")
scene_action_id_text :: proc(id:ecs.Entity_Id,allocator:mem.Allocator)->string { return fmt.aprintf("%d",u64(id),allocator=allocator) }
