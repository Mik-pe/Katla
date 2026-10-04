//! Shared additive and replacement staging resolves complete component references before publication.
package app

import ecs "../ecs"
import editor "../editor"
import "core:mem"

/// Owns staged identities and their document map until insertion or replacement commits.
Scene_Stage :: struct { entities:[dynamic]ecs.Entity_Id,mapping:map[ecs.Entity_Id]ecs.Entity_Id,next_key:u64,allocator:mem.Allocator }
/// Rolls back staged entities when requested, then releases the staging transaction's bookkeeping.
scene_stage_destroy :: proc(app:^Authoring,stage:^Scene_Stage,rollback:bool) {
    if rollback { for entity in stage.entities { ecs.destroy_entity(&app.world,entity) } }
    delete(stage.entities); delete(stage.mapping); stage^={}
}
/// Decodes every component and binds references; fresh_key_start assigns new global scene keys for insertion.
scene_snapshot_stage :: proc(app:^Authoring,snapshot:^Scene_Snapshot,fresh_key_start:u64=0)->(Scene_Stage,editor.Scene_Error) {
    context.allocator=app.world.allocator
    stage:=Scene_Stage{entities=make([dynamic]ecs.Entity_Id,app.world.allocator),mapping=make(map[ecs.Entity_Id]ecs.Entity_Id,app.world.allocator),allocator=app.world.allocator,next_key=fresh_key_start}
    success:=false; defer { if !success { scene_stage_destroy(app,&stage,true) } }
    if len(snapshot.entities)>100_000 || snapshot.next_entity_id==0 { return {},.Invalid_Operation }
    if fresh_key_start>0 && u64(len(snapshot.entities))>max(u64)-fresh_key_start { return {},.Invalid_Operation }
    for row in snapshot.entities {
        if row.key==0 || u64(row.key)>=snapshot.next_entity_id { return {},.Invalid_Operation }
        if _,duplicate:=stage.mapping[row.key]; duplicate { return {},.Invalid_Operation }
        entity:=ecs.create_entity(&app.world); append(&stage.entities,entity); stage.mapping[row.key]=entity
        key:=u64(row.key); if fresh_key_start>0 { key=stage.next_key; stage.next_key+=1 }
        ecs.add_component(&app.world,entity,Scene_Key{key})
    }
    for row in snapshot.entities {
        entity:=stage.mapping[row.key]
        seen:=make(map[string]bool,app.world.allocator)
        defer delete(seen)
        for component in row.components {
            if seen[component.name] { return {},.Invalid_Operation }; seen[component.name]=true
            entry:=app.registry.entries[component.name]; if entry==nil { return {},.Component_Not_Found }
            decoded:rawptr; ok:bool
            if component.owned_value!=nil {
                if component.owned_entry!=entry { return {},.Invalid_Operation }
                if scene_wire_hash(component.data)!=component.wire_hash { return {},.Decode_Failed }
                decoded=editor.editor_clone_value(entry,component.owned_value,app.world.allocator); ok=true
            } else { decoded,ok=editor.editor_decode_value(entry,component.data,app.world.allocator) }
            transferred:=false
            defer {
                if !transferred && entry.ops.destroy!=nil { entry.ops.destroy(decoded) }
                mem.free(decoded,app.world.allocator)
            }
            if !ok { return {},.Decode_Failed }
            if entry.T==Scene_Key {
                if (cast(^Scene_Key)decoded).value!=u64(row.key) { return {},.Invalid_Operation }
                if fresh_key_start>0 { continue }
            }
            if !editor.component_map_references(entry,decoded,{stage.mapping,true}) { return {},.Invalid_Operation }
            if !ecs.insert_component_value(&app.world,entity,entry.T,decoded) { return {},.Entity_Not_Found }
            transferred=true
        }
    }
    for entity,i in stage.entities {
        captured:=snapshot.entities[i].has_source
        mesh,has_mesh:=ecs.get_component(&app.world,entity,Scene_Mesh)
        model,has_model:=ecs.get_component(&app.world,entity,Scene_Model)
        if has_mesh && mesh.source.kind!=.Empty && has_model { return {},.Invalid_Operation }
        if has_model && !captured {
            if _,has_animation:=ecs.get_component(&app.world,entity,Animation_Model); !has_animation {
                entry:=app.registry.entries["AnimationModel"]; if entry==nil { return {},.Component_Not_Found }
                cloned:=editor.editor_clone_value(entry,&model.model.animation,app.world.allocator)
                ecs.insert_component_value(&app.world,entity,Animation_Model,cloned); mem.free(cloned,app.world.allocator)
            }
            if _,has_player:=ecs.get_component(&app.world,entity,Animation_Player); !has_player { ecs.add_component(&app.world,entity,animation_player_stopped()) }
        }
        if !captured && ((has_mesh && mesh.source.kind!=.Empty) || has_model) {
            if _,has_material:=ecs.get_component(&app.world,entity,Surface_Material); !has_material {
                material:=Surface_Material{roughness=0.5,ao=1}; if has_model { material.metallic=1; material.roughness=1 }; ecs.add_component(&app.world,entity,material)
            }
        }
        if _,has_parent:=ecs.get_component(&app.world,entity,Scene_Parent); has_parent {
            if _,err:=scene_world_matrix(app,entity); err!=.None { return {},err }
        }
    }
    if err:=light_scene_validate(app,stage.entities[:]); err!=.None { return {},err }
    if err:=scene_gameplay_validate_entities(app,stage.entities[:]); err!=.None { return {},err }
    success=true; return stage,.None
}
