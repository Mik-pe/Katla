//! Application scene identities, local placement and exact hierarchy matrices.
package app

import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:strings"

/// Owns a human label independently from runtime or persistent entity identity.
Scene_Name :: struct { name:string }
/// Local TRS is composed into exact matrices, preserving hierarchy shear.
Scene_Transform :: struct { local:km.Transform }
/// Stores stable document-local identity without exposing runtime slots.
Scene_Key :: struct { value:u64 `inspect:"skip"` }
/// Monotonic authoring identity survives deletion and capture.
Scene_Identity :: struct { next_entity_id:u64 }

/// Holds one generational parent; absence of the component means a root.
Scene_Parent :: struct { entity:ecs.Entity_Id `inspect:"skip"` }

@(private="package")
scene_name_destroy :: proc(value:rawptr) { name:=cast(^Scene_Name)value; delete(name.name); name^={} }
@(private="package")
scene_name_clone :: proc(dst,src:rawptr) { target:=cast(^Scene_Name)dst; source:=cast(^Scene_Name)src; target^={strings.clone(source.name)} }

/// Installs owned label, transform and parent codecs for snapshots and application tools.
scene_components_register :: proc(app:^Authoring) {
    context.allocator=app.world.allocator
    editor.editor_register(&app.world,&app.registry,"SceneKey",Scene_Key{},spawn_default=false,duplicate=false)
    if _,exists:=ecs.get_resource(&app.world,Scene_Identity); !exists { ecs.insert_resource(&app.world,Scene_Identity{1}) }
    editor.editor_register(&app.world,&app.registry,"SceneName",Scene_Name{},ecs.Value_Ops{scene_name_destroy,scene_name_clone})
    editor.editor_register(&app.world,&app.registry,"SceneTransform",Scene_Transform{km.TRANSFORM_IDENTITY})
    editor.editor_register(&app.world,&app.registry,"SceneParent",Scene_Parent{},spawn_default=false)
}

/// Resolves current locals iteratively and rejects stale parents, missing transforms and cycles.
scene_world_matrix :: proc(app:^Authoring,entity:ecs.Entity_Id)->(km.Mat4,editor.Scene_Error) {
    ancestors:=make([dynamic]ecs.Entity_Id,app.world.allocator); defer delete(ancestors)
    current:=entity
    for {
        if !ecs.entity_exists(&app.world,current) { return {},.Entity_Not_Found }
        for visited in ancestors { if current==visited { return {},.Invalid_Operation } }
        append(&ancestors,current)
        if _,ok:=ecs.get_component(&app.world,current,Scene_Transform); !ok { return {},.Component_Not_Found }
        parent,exists:=ecs.get_component(&app.world,current,Scene_Parent)
        if !exists { break }
        current=parent.entity
    }
    world_matrix:=km.identity(km.Mat4)
    for i:=len(ancestors)-1;i>=0;i-=1 {
        transform,_:=ecs.get_component(&app.world,ancestors[i],Scene_Transform)
        world_matrix=km.matrix_mul(world_matrix,km.transform_to_mat4(transform.local))
    }
    return world_matrix,.None
}

/// Validates a prospective parent chain before applying one local relationship change.
scene_set_parent :: proc(app:^Authoring,entity,parent:ecs.Entity_Id,has_parent:bool)->editor.Scene_Error {
    if app.mode!=.Editing { return .Editing_Required }
    if !ecs.entity_exists(&app.world,entity) { return .Entity_Not_Found }
    if _,protected:=ecs.get_component(&app.world,entity,Editor_Hidden); protected { return .Protected_Entity }
    if has_parent {
        current:=parent
        visited:=make(map[ecs.Entity_Id]bool,app.world.allocator); defer delete(visited)
        for {
            if !ecs.entity_exists(&app.world,current) { return .Entity_Not_Found }
            if current==entity || visited[current] { return .Invalid_Operation }
            if _,protected:=ecs.get_component(&app.world,current,Editor_Hidden); protected { return .Protected_Entity }
            visited[current]=true
            next,exists:=ecs.get_component(&app.world,current,Scene_Parent); if !exists { break }; current=next.entity
        }
        ecs.add_component(&app.world,entity,Scene_Parent{parent})
    } else { ecs.remove_component(&app.world,entity,Scene_Parent) }
    return .None
}
