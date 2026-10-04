//! Scene transport uses serde enum shapes while RON publication retains explicit variant syntax.
package app

import "core:encoding/json"
import ron "../encoding/ron"
import "core:strings"
import "core:fmt"

@(private="package")
scene_value_clone :: proc(value:json.Value)->(json.Value,bool) { cloned,err:=ron.clone_value(value); return cloned,err.kind==.None }
@(private="package")
scene_variant_ron :: proc(value:json.Value,newtype:bool)->json.Value {
    if _,_,_,variant:=ron.variant_read(value); variant { return value }
    name,payload,valid:=scene_variant(value); if !valid { return value }
    if object,is_object:=value.(json.Object); is_object { name=strings.clone(name); for key in object { delete(key) }; delete(object) }
    else { name=strings.clone(name); if text,is_text:=value.(string); is_text { delete(text) } }
    if payload!=nil && newtype { wrapped:=make(json.Array,1,context.allocator); wrapped[0]=payload; payload=wrapped }
    variant:=ron.variant_value(name,payload,payload!=nil); delete(name); return variant
}
/// Clones scene/prefab JSON and converts its built-in enum fields to canonical RON values.
scene_document_ron_clone :: proc(document:json.Value)->(json.Value,bool) {
    result,ok:=scene_value_clone(document); if !ok { return nil,false }
    root,is_root:=result.(json.Object); if !is_root { json.destroy_value(result); return nil,false }
    scene:=root
    if nested,is_nested:=root["scene"].(json.Object); is_nested { scene=nested }
    entities,is_entities:=scene["entities"].(json.Array); if !is_entities { json.destroy_value(result); return nil,false }
    for entity in entities {
        row,is_row:=entity.(json.Object); if !is_row { json.destroy_value(result); return nil,false }
        if source,present:=row["source"]; present {
            row["source"]=scene_variant_ron(source,false)
            _,payload,_:=scene_variant(row["source"])
            if fields,is_fields:=payload.(json.Object); is_fields { if path,has_path:=fields["path"]; has_path { fields["path"]=scene_variant_ron(path,true) } }
        }
        if collider,present:=row["collider_shape"]; present { kind,_,_:=scene_variant(collider); row["collider_shape"]=scene_variant_ron(collider,kind=="Box" || kind=="Sphere") }
        if body,is_body:=row["rigid_body"].(json.Object); is_body { if kind,present:=body["kind"]; present { body["kind"]=scene_variant_ron(kind,false) } }
        if script,is_script:=row["script"].(json.Object); is_script { if path,present:=script["path"]; present { script["path"]=scene_variant_ron(path,true) } }
        if particle,is_particle:=row["particle_emitter"].(json.Object); is_particle { if shape,present:=particle["shape"]; present { particle["shape"]=scene_variant_ron(shape,false) } }
        if trigger,present:=row["trigger_volume"]; present { row["trigger_volume"]=scene_variant_ron(trigger,false) }
    }
    return result,true
}
@(private="package")
scene_public_mutate :: proc(value:json.Value)->json.Value {
    #partial switch tree in value {
    case json.Object:
        if number,is_uint:=ron.uint_read(tree); is_uint { text:=fmt.aprintf("%d",number); json.destroy_value(tree); return text }
        if name,payload,present,is_variant:=ron.variant_read(tree); is_variant {
            for key in tree { delete(key) }; delete(tree)
            if !present { return name }
            if array,is_array:=payload.(json.Array); is_array && len(array)==1 { payload=array[0]; delete(array) }
            object:=make(json.Object,context.allocator); object[name]=scene_public_mutate(payload); return object
        }
        object:=tree; for key,item in object { object[key]=scene_public_mutate(item) }; return object
    case json.Array: for item,i in tree { tree[i]=scene_public_mutate(item) }
    }
    return value
}
/// Clones current parsed asset enums into the ordinary externally tagged JSON document format.
scene_document_public_clone :: proc(document:json.Value)->(json.Value,bool) { cloned,ok:=scene_value_clone(document); if !ok { return nil,false }; return scene_public_mutate(cloned),true }
