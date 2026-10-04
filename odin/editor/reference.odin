//! Explicit generational-reference mapping for scene staging and restoration.
package editor

import ecs "../ecs"
import "core:reflect"
import "core:strings"

/// Strict maps reject references outside a captured scene; partial maps preserve other identities.
Reference_Map :: struct { entities:map[ecs.Entity_Id]ecs.Entity_Id, strict:bool }

/// Maps one reference without treating zero as absent: zero is a valid first-generation entity.
reference_map_entity :: proc(entity:^ecs.Entity_Id,mapping:Reference_Map)->bool {
    replacement,exists:=mapping.entities[entity^]
    if exists { entity^=replacement; return true }
    return !mapping.strict
}

/// Uses the application's explicit mapper or walks typed references in ordinary value containers.
component_map_references :: proc(entry:^Editor_Entry,value:rawptr,mapping:Reference_Map)->bool {
    if !entry.has_references { return true }
    if entry.reference_map!=nil { return entry.reference_map(value,mapping) }
    return reflected_reference_map(any{value,entry.T},mapping)
}

@(private="package")
reflected_reference_map :: proc(value:any,mapping:Reference_Map)->bool {
    if value.id==ecs.Entity_Id { return reference_map_entity(cast(^ecs.Entity_Id)value.data,mapping) }
    info:=reflect.type_info_base(type_info_of(value.id))
    #partial switch t in info.variant {
    case reflect.Type_Info_Struct:
        for field in reflect.struct_fields_zipped(value.id) {
            pointer:=rawptr(uintptr(value.data)+uintptr(field.offset))
            tag:=reflect.struct_tag_get(field.tag,"inspect")
            if strings.contains(tag,"entity_ref") && field.type.id==u64 {
                if !reference_map_entity(cast(^ecs.Entity_Id)pointer,mapping) { return false }
            } else if !reflected_reference_map(any{pointer,field.type.id},mapping) { return false }
        }
    case reflect.Type_Info_Union:
        for variant in t.variants { if reference_type_contains(variant.id) { return false } }
    case reflect.Type_Info_Map:
        if reference_type_contains(t.key.id) || reference_type_contains(t.value.id) { return false }
    case reflect.Type_Info_Array,reflect.Type_Info_Slice,reflect.Type_Info_Dynamic_Array:
        iterator:int
        for element,_ in reflect.iterate_array(value,&iterator) {
            if !reflected_reference_map(element,mapping) { return false }
        }
    }
    return true
}

/// Rewrites references in every registered live component after an entity receives a fresh ID.
editor_remap_world_references :: proc(w:^ecs.World,reg:^Component_Registry,remap:Entity_Remap) {
    mapping:=make(map[ecs.Entity_Id]ecs.Entity_Id,w.allocator); defer delete(mapping)
    mapping[remap.before]=remap.after
    entities:=ecs.entity_ids(w); defer delete(entities)
    for entity in entities {
        for _,entry in reg.entries {
            if value:=ecs.component_address(w,entity,entry.T); value!=nil {
                ok:=component_map_references(entry,value,{mapping,false})
                assert(ok,"partial reference maps must preserve unmapped IDs")
            }
        }
    }
}

@(private="package")
reference_type_contains :: proc(T:typeid)->bool {
    if T==ecs.Entity_Id { return true }
    ti:=reflect.type_info_base(type_info_of(T))
    #partial switch info in ti.variant {
    case reflect.Type_Info_Struct:
        for field in reflect.struct_fields_zipped(T) {
            if strings.contains(reflect.struct_tag_get(field.tag,"inspect"),"entity_ref") && field.type.id==u64 { return true }
            if reference_type_contains(field.type.id) { return true }
        }
    case reflect.Type_Info_Array: return reference_type_contains(info.elem.id)
    case reflect.Type_Info_Slice: return reference_type_contains(info.elem.id)
    case reflect.Type_Info_Dynamic_Array: return reference_type_contains(info.elem.id)
    case reflect.Type_Info_Union:
        for variant in info.variants { if reference_type_contains(variant.id) { return true } }
    case reflect.Type_Info_Map:
        return reference_type_contains(info.key.id) || reference_type_contains(info.value.id)
    }
    return false
}
