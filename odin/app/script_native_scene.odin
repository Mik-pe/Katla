//! Preview entity admission joins genuine native participants before publishing identity changes.
package app
import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:strings"

@(private="package")
script_native_spawn :: proc(app:^Authoring)->editor.Scene_Error {
    if app.mode!=.Playing { return .Invalid_Operation }
    identity:=ecs.get_resource_mut(&app.world,Scene_Identity); if identity==nil || identity.next_entity_id==max(u64) { return .Invalid_Operation }
    key:=identity.next_entity_id
    entity:=ecs.create_entity(&app.world); accepted:=false; defer { if !accepted { ecs.destroy_entity(&app.world,entity) } }
    ecs.add_component(&app.world,entity,Scene_Key{key}); ecs.add_component(&app.world,entity,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&app.world,entity,Scene_Name{strings.clone("Entity",app.world.allocator)})
    preparation,error:=scene_prepare_begin(app,{entity},.Insert); if error!=.None { return error }
    defer { if !accepted { scene_prepare_finish(&preparation,false) } }
    if physics_error:=physics_box3d_sync(app); physics_error!=.None { return physics_error }
    scene_prepare_finish(&preparation,true); identity.next_entity_id=key+1; accepted=true; return .None
}
@(private="package")
script_native_remove :: proc(app:^Authoring,entity:ecs.Entity_Id)->editor.Scene_Error {
    if app.mode!=.Playing { return .Invalid_Operation }
    removed,error:=scene_subtree(app,entity); if error!=.None { return error }; defer delete(removed,app.world.allocator)
    for id in removed { if _,hidden:=ecs.get_component(&app.world,id,Editor_Hidden); hidden { return .Protected_Entity } }
    ids:=ecs.entity_ids(&app.world); defer delete(ids)
    preparation,prepare_error:=scene_prepare_begin(app,removed,.Remove); if prepare_error!=.None { return prepare_error }
    accepted:=false; defer { if !accepted { scene_prepare_finish(&preparation,false) } }
    if physics_error:=physics_box3d_sync_excluding(app,removed); physics_error!=.None { return physics_error }
    for id in ids {
        joint,present:=ecs.get_component(&app.world,id,Physics_Joint); if !present { continue }
        affected:=false; for target in removed { if joint.a==target || joint.b==target { affected=true; break } }
        if affected { ecs.remove_component(&app.world,id,Physics_Joint) }
    }
    for id in removed { ecs.destroy_entity(&app.world,id) }
    scene_prepare_finish(&preparation,true); accepted=true; return .None
}
