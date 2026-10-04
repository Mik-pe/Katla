//! Staged component snapshots preserve owned values and bind references after all entities exist.
package app

import ecs "../ecs"
import editor "../editor"
import "core:mem"
import "core:strings"

/// A registered component's owned wire value; runtime references use document-local keys.
Scene_Component :: struct { name:string, data:[]byte }
/// Document-local keys remain independent from runtime entity slots and generations.
Scene_Entity :: struct { key:ecs.Entity_Id, components:[dynamic]Scene_Component }
/// Owns a reconstructible CPU scene, excluding protected editor entities.
Scene_Snapshot :: struct { entities:[dynamic]Scene_Entity, next_entity_id:u64, allocator:mem.Allocator }

/// Releases snapshot wire values and component names with the captured allocator.
scene_snapshot_destroy :: proc(snapshot:^Scene_Snapshot) {
    for entity in snapshot.entities {
        for component in entity.components { delete(component.name,snapshot.allocator); delete(component.data,snapshot.allocator) }
        delete(entity.components)
    }
    delete(snapshot.entities); snapshot^={}
}

/// Captures every registered visible component, rejecting references outside the captured scene.
scene_snapshot_capture :: proc(app:^Authoring)->(Scene_Snapshot,editor.Scene_Error) {
    allocator:=app.world.allocator; context.allocator=allocator
    result:=Scene_Snapshot{entities=make([dynamic]Scene_Entity,allocator),allocator=allocator}
    success:=false; defer { if !success { scene_snapshot_destroy(&result) } }
    ids:=ecs.entity_ids(&app.world); defer delete(ids)
    mapping:=make(map[ecs.Entity_Id]ecs.Entity_Id,allocator); defer delete(mapping)
    used:=make(map[u64]bool,allocator); defer delete(used)
    known_types:=make(map[typeid]bool,allocator); defer delete(known_types)
    known_types[Scene_Key]=true
    for _,entry in app.registry.entries { known_types[entry.T]=true }
    next_key:u64=1
    if identity,exists:=ecs.get_resource(&app.world,Scene_Identity); exists { next_key=max(identity.next_entity_id,1) }
    for id in ids {
        if _,hidden:=ecs.get_component(&app.world,id,Editor_Hidden); hidden { continue }
        if key,exists:=ecs.get_component(&app.world,id,Scene_Key); exists {
            if key.value==0 || key.value==max(u64) || used[key.value] { return {},.Invalid_Operation }
            used[key.value]=true; mapping[id]=ecs.Entity_Id(key.value); next_key=max(next_key,key.value+1)
        }
    }
    for id in ids {
        if _,hidden:=ecs.get_component(&app.world,id,Editor_Hidden); hidden { continue }
        for T in app.world.stores { if !known_types[T] && ecs.component_address(&app.world,id,T)!=nil { return {},.Component_Not_Found } }
        if _,existing:=mapping[id]; !existing {
            if next_key==max(u64) { return {},.Invalid_Operation }
            mapping[id]=ecs.Entity_Id(next_key); next_key+=1
        }
    }
    if len(mapping)>100_000 { return {},.Invalid_Operation }
    wire_bytes:=0
    names:=editor.editor_type_names(&app.registry); defer delete(names)
    for id in ids {
        key,included:=mapping[id]; if !included { continue }
        row:=Scene_Entity{key=key,components=make([dynamic]Scene_Component,allocator)}
        append(&result.entities,row)
        stored:=&result.entities[len(result.entities)-1]
        for name in names {
            entry:=app.registry.entries[name]
            value:=ecs.component_address(&app.world,id,entry.T); if value==nil { continue }
            bytes,err:=editor.editor_component_json(&app.world,id,entry)
            if err!=.None { return {},err }
            decoded,ok:=editor.editor_decode_value(entry,bytes,allocator); delete(bytes,allocator)
            if !ok { if entry.ops.destroy!=nil { entry.ops.destroy(decoded) }; mem.free(decoded,allocator); return {},.Decode_Failed }
            mapped:=editor.component_map_references(entry,decoded,{mapping,true})
            data,encoded:=editor.editor_encode_value(entry,decoded,allocator)
            if entry.ops.destroy!=nil { entry.ops.destroy(decoded) }; mem.free(decoded,allocator)
            if !mapped { delete(data,allocator); return {},.Invalid_Operation }
            if !encoded { return {},.Decode_Failed }
            wire_bytes+=len(name)+len(data)
            if wire_bytes>64*1024*1024 { delete(data,allocator); return {},.Invalid_Operation }
            append(&stored.components,Scene_Component{strings.clone(name,allocator),data})
        }
    }
    result.next_entity_id=next_key
    if _,registered:=ecs.component_ops(&app.world,Scene_Key); !registered { ecs.register_component(&app.world,Scene_Key) }
    for entity,key in mapping { ecs.add_component(&app.world,entity,Scene_Key{u64(key)}) }
    ecs.insert_resource(&app.world,Scene_Identity{next_key})
    success=true; return result,.None
}

/// Stages and decodes replacement entities before retiring the authored scene; failures preserve it.
scene_snapshot_restore :: proc(app:^Authoring,snapshot:^Scene_Snapshot)->editor.Scene_Error {
    context.allocator=app.world.allocator
    staged:=make([dynamic]ecs.Entity_Id,app.world.allocator); defer delete(staged)
    mapping:=make(map[ecs.Entity_Id]ecs.Entity_Id,app.world.allocator); defer delete(mapping)
    if len(snapshot.entities)>100_000 || snapshot.next_entity_id==0 { return .Invalid_Operation }
    success:=false
    defer { if !success { for entity in staged { ecs.destroy_entity(&app.world,entity) } } }
    for row in snapshot.entities {
        if row.key==0 || u64(row.key)>=snapshot.next_entity_id { return .Invalid_Operation }
        if _,duplicate:=mapping[row.key]; duplicate { return .Invalid_Operation }
        entity:=ecs.create_entity(&app.world); append(&staged,entity); mapping[row.key]=entity
        ecs.add_component(&app.world,entity,Scene_Key{u64(row.key)})
    }
    for row in snapshot.entities {
        entity:=mapping[row.key]
        seen:=make(map[string]bool,app.world.allocator)
        defer delete(seen)
        for component in row.components {
            if seen[component.name] { return .Invalid_Operation }; seen[component.name]=true
            entry:=app.registry.entries[component.name]; if entry==nil { return .Component_Not_Found }
            decoded,ok:=editor.editor_decode_value(entry,component.data,app.world.allocator)
            transferred:=false
            defer {
                if !transferred && entry.ops.destroy!=nil { entry.ops.destroy(decoded) }
                mem.free(decoded,app.world.allocator)
            }
            if !ok { return .Decode_Failed }
            if entry.T==Scene_Key && (cast(^Scene_Key)decoded).value!=u64(row.key) { return .Invalid_Operation }
            if !editor.component_map_references(entry,decoded,{mapping,true}) { return .Invalid_Operation }
            if !ecs.insert_component_value(&app.world,entity,entry.T,decoded) { return .Entity_Not_Found }
            transferred=true
        }
    }
    for entity in staged {
        if _,has_parent:=ecs.get_component(&app.world,entity,Scene_Parent); has_parent {
            if _,err:=scene_world_matrix(app,entity); err!=.None { return err }
        }
    }
    ids:=ecs.entity_ids(&app.world); defer delete(ids)
    staged_set:=make(map[ecs.Entity_Id]bool,app.world.allocator); defer delete(staged_set)
    for entity in staged { staged_set[entity]=true }
    for entity in ids {
        if _,hidden:=ecs.get_component(&app.world,entity,Editor_Hidden); hidden { continue }
        if !staged_set[entity] { ecs.destroy_entity(&app.world,entity) }
    }
    ecs.insert_resource(&app.world,Scene_Identity{snapshot.next_entity_id})
    success=true
    return .None
}
