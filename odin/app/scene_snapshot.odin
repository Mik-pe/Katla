//! Staged component snapshots preserve owned values and bind references after all entities exist.
package app

import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:mem"
import "core:strings"

/// A registered component's owned wire value; runtime references use document-local keys.
Scene_Component :: struct { name:string, data:[]byte,owned_entry:^editor.Editor_Entry,owned_value:rawptr,owned_ops:ecs.Value_Ops,wire_hash:u64 }
/// Document-local keys remain independent from runtime entity slots and generations.
Scene_Entity :: struct { key:ecs.Entity_Id, components:[dynamic]Scene_Component,source_entity:ecs.Entity_Id,has_source:bool }
/// Owns a reconstructible CPU scene, excluding protected editor entities.
Scene_Snapshot :: struct { entities:[dynamic]Scene_Entity, next_entity_id:u64, allocator:mem.Allocator }

/// Releases snapshot wire values and component names with the captured allocator.
scene_snapshot_destroy :: proc(snapshot:^Scene_Snapshot) {
    for entity in snapshot.entities {
        for component in entity.components {
            if component.owned_value!=nil { if component.owned_ops.destroy!=nil { context.allocator=snapshot.allocator; component.owned_ops.destroy(component.owned_value) }; mem.free(component.owned_value,snapshot.allocator) }
            delete(component.name,snapshot.allocator); delete(component.data,snapshot.allocator)
        }
        delete(entity.components)
    }
    delete(snapshot.entities); snapshot^={}
}

/// Captures every registered visible component, rejecting references outside the captured scene.
scene_snapshot_capture :: proc(app:^Authoring,subset:[]ecs.Entity_Id=nil,detach_root:bool=false,root:ecs.Entity_Id=0,commit_identity:bool=true,subset_only:bool=false,next_key_override:u64=0)->(Scene_Snapshot,editor.Scene_Error) {
    allocator:=app.world.allocator; context.allocator=allocator
    result:=Scene_Snapshot{entities=make([dynamic]Scene_Entity,allocator),allocator=allocator}
    success:=false; defer { if !success { scene_snapshot_destroy(&result) } }
    ids:=ecs.entity_ids(&app.world); defer delete(ids)
    if subset!=nil || subset_only { clear(&ids); append(&ids,..subset); for id,i in ids { if !ecs.entity_exists(&app.world,id) { return {},.Entity_Not_Found }; if _,hidden:=ecs.get_component(&app.world,id,Editor_Hidden); hidden { return {},.Protected_Entity }; for earlier in ids[:i] { if earlier==id { return {},.Invalid_Operation } } } }
    mapping:=make(map[ecs.Entity_Id]ecs.Entity_Id,allocator); defer delete(mapping)
    used:=make(map[u64]bool,allocator); defer delete(used)
    known_types:=make(map[typeid]bool,allocator); defer delete(known_types)
    known_types[Scene_Key]=true
    for _,entry in app.registry.entries { known_types[entry.T]=true }
    next_key:u64=1
    if next_key_override>0 { next_key=next_key_override } else if identity,exists:=ecs.get_resource(&app.world,Scene_Identity); exists { next_key=max(identity.next_entity_id,1) }
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
        row:=Scene_Entity{key=key,components=make([dynamic]Scene_Component,allocator),source_entity=id,has_source=true}
        append(&result.entities,row)
        stored:=&result.entities[len(result.entities)-1]
        for name in names {
            entry:=app.registry.entries[name]
            if detach_root && id==root && entry.T==Scene_Parent { continue }
            value:=ecs.component_address(&app.world,id,entry.T); if value==nil { continue }
            decoded:=editor.editor_clone_value(entry,value,allocator)
            if detach_root && id==root && entry.T==Scene_Transform { (cast(^Scene_Transform)decoded).local=km.TRANSFORM_IDENTITY }
            retained:=false
            defer { if !retained { if entry.ops.destroy!=nil { entry.ops.destroy(decoded) }; mem.free(decoded,allocator) } }
            if !editor.component_map_references(entry,decoded,{mapping,true}) { return {},.Invalid_Operation }
            data,encoded:=editor.editor_encode_value(entry,decoded,allocator)
            if !encoded { delete(data,allocator); return {},.Decode_Failed }
            wire_bytes+=len(name)+len(data)
            if wire_bytes>64*1024*1024 { delete(data,allocator); return {},.Invalid_Operation }
            append(&stored.components,Scene_Component{name=strings.clone(name,allocator),data=data,owned_entry=entry,owned_value=decoded,owned_ops=entry.ops,wire_hash=scene_wire_hash(data)})
            retained=true
        }
    }
    result.next_entity_id=next_key
    if commit_identity { scene_snapshot_commit_keys(app,&result) }
    success=true; return result,.None
}

/// Stages replacement entities before retiring the authored scene; failures preserve it.
scene_snapshot_restore :: proc(app:^Authoring,snapshot:^Scene_Snapshot,file_publication:bool=false)->editor.Scene_Error {
    context.allocator=app.world.allocator
    stage,err:=scene_snapshot_stage(app,snapshot)
    if err!=.None { return err }
    committed:=false; defer scene_stage_destroy(app,&stage,!committed)
    observation:Scene_File_Observation; defer scene_file_observe_finish(&observation,committed)
    if file_publication && ecs.contains_resource(&app.world,Scene_File_Observer) {
        prepared,baseline_error:=scene_snapshot_capture(app,subset=stage.entities[:],commit_identity=false,subset_only=true,next_key_override=stage.next_key)
        if baseline_error!=.None { return baseline_error }; defer scene_snapshot_destroy(&prepared)
        token,observe_error:=scene_file_observe_begin(app,&prepared); if observe_error!=.None { return observe_error }; observation=token
    }
    preparation,prepare_error:=scene_prepare_begin(app,stage.entities[:],.Replace)
    if prepare_error!=.None { return prepare_error }; defer scene_prepare_finish(&preparation,committed)
    ids:=ecs.entity_ids(&app.world); defer delete(ids)
    staged_set:=make(map[ecs.Entity_Id]bool,app.world.allocator); defer delete(staged_set)
    for entity in stage.entities { staged_set[entity]=true }
    for entity in ids {
        if _,hidden:=ecs.get_component(&app.world,entity,Editor_Hidden); hidden { continue }
        if !staged_set[entity] { ecs.destroy_entity(&app.world,entity) }
    }
    ecs.insert_resource(&app.world,Scene_Identity{stage.next_key})
    committed=true
    return .None
}

@(private="package")
scene_wire_hash :: proc(data:[]byte)->u64 { hash:u64=14695981039346656037; for value in data { hash=(hash~u64(value))*1099511628211 }; return hash }
/// Clones captured owned values or decodes file DTOs; the caller releases the returned value.
scene_row_owned_decode :: proc(app:^Authoring,row:Scene_Entity,name:string)->(rawptr,bool) {
    entry:=app.registry.entries[name]; if entry==nil { return nil,false }
    for component in row.components {
        if component.name!=name { continue }
        if component.owned_value!=nil {
            if component.owned_entry!=entry || scene_wire_hash(component.data)!=component.wire_hash { return nil,false }
            return editor.editor_clone_value(entry,component.owned_value,app.world.allocator),true
        }
        return editor.editor_decode_value(entry,component.data,app.world.allocator)
    }
    return nil,false
}
/// Destroys a temporary decoded row value with its registered ownership hooks.
scene_row_owned_destroy :: proc(app:^Authoring,name:string,value:rawptr) {
    if value==nil { return }; context.allocator=app.world.allocator
    if entry:=app.registry.entries[name]; entry!=nil && entry.ops.destroy!=nil { entry.ops.destroy(value) }; mem.free(value,app.world.allocator)
}

/// Publishes keys only after capture's enclosing transaction has succeeded.
scene_snapshot_commit_keys :: proc(app:^Authoring,snapshot:^Scene_Snapshot) {
    if _,registered:=ecs.component_ops(&app.world,Scene_Key); !registered { ecs.register_component(&app.world,Scene_Key) }
    for row in snapshot.entities { if row.has_source { assert(ecs.entity_exists(&app.world,row.source_entity)); ecs.add_component(&app.world,row.source_entity,Scene_Key{u64(row.key)}) } }
    ecs.insert_resource(&app.world,Scene_Identity{snapshot.next_entity_id})
}
