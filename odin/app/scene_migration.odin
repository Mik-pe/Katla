//! Older scene readers explicitly convert authored descriptors into the single current schema.
package app

import editor "../editor"
import "core:encoding/json"
import "core:strings"
import "core:mem"
import "core:path/filepath"

@(private="package")
scene_migration_set :: proc(fields:^json.Object,name:string,value:json.Value) {
    if old,present:=fields^[name]; present { json.destroy_value(old); fields^[name]=value }
    else { fields^[strings.clone(name)]=value }
}
@(private="package")
scene_migration_remove :: proc(fields:^json.Object,name:string) {
    for key,value in fields^ { if key==name { delete_key(fields,key); json.destroy_value(value); delete(key); return } }
}
@(private="package")
scene_migration_clone :: proc(value:json.Value)->json.Value { copy,valid:=scene_value_clone(value); if !valid { return nil }; return copy }
@(private="package")
scene_migration_asset :: proc(path:string)->json.Value {
    normalized,allocated:=strings.replace_all(path,"\\","/"); defer { if allocated { delete(normalized) } }
    if strings.has_prefix(normalized,"resources/") { return trigger_json_value(struct {Resource:string}{normalized[len("resources/"):]}) }
    if filepath.is_abs(normalized) { return trigger_json_value(struct {File:string}{normalized}) }
    return trigger_json_value(struct {Scene:string}{normalized})
}
@(private="package")
scene_migration_reference :: proc(value:json.Value,names:map[string]u64)->(json.Value,bool) {
    name,is_name:=value.(string); if !is_name { return nil,false }; id,present:=names[name]; if !present || id==0 { return nil,false }; return scene_document_key_value(id),true
}
@(private="package")
scene_migration_rules :: proc(rules:json.Value,names:map[string]u64)->bool {
    array,is_array:=rules.(json.Array); if !is_array { return false }
    for rule in array {
        object,is_object:=rule.(json.Object); if !is_object { return false }
        if reference,present:=scene_gameplay_present(object,"other_entity"); present { mapped,valid:=scene_migration_reference(reference,names); if !valid { return false }; scene_migration_set(&object,"other_entity",mapped) }
        actions,is_actions:=object["actions"].(json.Array); if !is_actions { return false }
        for action in actions { fields,is_fields:=action.(json.Object); if !is_fields { return false }; if target,is_target:=fields["target"].(json.Object); is_target { if reference,present:=target["entity"]; present { mapped,valid:=scene_migration_reference(reference,names); if !valid { return false }; scene_migration_set(&target,"entity",mapped) } } }
    }
    return true
}
/// Converts v0/v1/v2 into owned v3 descriptors without modifying input or publishing engine state.
scene_document_migrate :: proc(document:json.Value,allocator:mem.Allocator=context.allocator)->(json.Value,editor.Scene_Error) {
    context.allocator=allocator
    original,is_original:=document.(json.Object); if !is_original { return nil,.Decode_Failed }
    version:json.Integer
    if raw_version,present:=original["version"]; present { parsed,is_version:=raw_version.(json.Integer); if !is_version || parsed<0 || parsed>3 { return nil,.Decode_Failed }; version=parsed }
    if version==3 { cloned,valid:=scene_value_clone(document); if !valid { return nil,.Decode_Failed }; return cloned,.None }
    if !recipe_keys(original,{"version","name","author","created_at","modified_at","engine_version","entities"}) { return nil,.Decode_Failed }
    root_value,cloned:=scene_document_public_clone(document); if !cloned { return nil,.Decode_Failed }
    success:=false; defer { if !success { json.destroy_value(root_value) } }
    root:=root_value.(json.Object); entities,is_entities:=root["entities"].(json.Array); if !is_entities || len(entities)>100_000 { return nil,.Decode_Failed }
    names:=make(map[string]u64,allocator); defer delete(names)
    for entity,index in entities {
        fields,is_fields:=entity.(json.Object); if !is_fields || !recipe_keys(fields,{"name","parent","transform","source","drawable","point_light","particle_emitter","animation","velocity","script","perspective","directional_light","audio_emitter","rigid_body","rigid_body_properties","reverb_zone","collider_shape","physics_material","trigger_volume","collision_filter","trigger_rules"}) || !scene_gameplay_required(fields,{"transform","source"}) { return nil,.Decode_Failed }
        if name,present:=scene_gameplay_present(fields,"name"); present { text,is_text:=name.(string); if !is_text { return nil,.Decode_Failed }; if _,found:=names[text]; found { names[text]=0 } else { names[text]=u64(index+1) } }
    }
    for entity,index in entities {
        fields:=entity.(json.Object); scene_migration_set(&fields,"id",scene_document_key_value(u64(index+1)))
        if reference,present:=scene_gameplay_present(fields,"parent"); present { mapped,valid:=scene_migration_reference(reference,names); if !valid { return nil,.Invalid_Operation }; scene_migration_set(&fields,"parent",mapped) }
        if rules,present:=fields["trigger_rules"]; present && !scene_migration_rules(rules,names) { return nil,.Invalid_Operation }
        source,payload,valid_source:=scene_variant(fields["source"]); if !valid_source { return nil,.Decode_Failed }
        switch source {
        case "GltfModel","StlModel":
            object,is_object:=payload.(json.Object); if !is_object || !recipe_keys(object,{"path"}) { return nil,.Decode_Failed }; path,is_path:=object["path"].(string); if !is_path { return nil,.Decode_Failed }
            scene_migration_set(&object,"path",scene_migration_asset(path))
        case "ParticleEmitter","Light","Trigger": if payload!=nil { return nil,.Decode_Failed }
        case "Cube","Sphere","Plane","Cylinder","Torus":
        case: return nil,.Decode_Failed
        }
        if source=="Light" || source=="ParticleEmitter" { scene_migration_remove(&fields,"drawable") }
        if source=="Light" { if _,present:=scene_gameplay_present(fields,"point_light"); !present { scene_migration_set(&fields,"point_light",trigger_json_value(point_light_default())) } }
        if particles,present:=scene_gameplay_present(fields,"particle_emitter"); present {
            object,is_object:=particles.(json.Object)
            required:=([17]string{"position","emit_rate","base_lifetime","lifetime_variation","velocity_direction","velocity_magnitude","velocity_cone_angle","base_scale","scale_variation","color","color_variation","gravity","turbulence_strength","turbulence_frequency","shape","shape_params","active"})
            if !is_object || !recipe_keys(object,required[:]) || !scene_gameplay_required(object,required[:]) { return nil,.Decode_Failed }
            if _,position_valid:=recipe_vector(object["position"],3); !position_valid { return nil,.Decode_Failed }; scene_migration_remove(&object,"position")
        } else if source=="ParticleEmitter" { scene_migration_set(&fields,"particle_emitter",particle_document(particle_defaults())) }
        if body,present:=scene_gameplay_present(fields,"rigid_body"); present {
            kind,body_payload,valid:=scene_variant(body); if !valid || body_payload!=nil || (kind!="Static" && kind!="Dynamic" && kind!="Kinematic") { return nil,.Decode_Failed }
            descriptor:=make(json.Object,allocator); descriptor[strings.clone("kind")]=strings.clone(kind)
            if properties,has_properties:=scene_gameplay_present(fields,"rigid_body_properties"); has_properties {
                object,is_object:=properties.(json.Object); if !is_object || !recipe_keys(object,{"gravity_scale","ccd_enabled","linear_velocity"}) || !scene_gameplay_required(object,{"gravity_scale","ccd_enabled","linear_velocity"}) { json.destroy_value(descriptor); return nil,.Decode_Failed }
                for name,value in object { descriptor[strings.clone(name)]=scene_migration_clone(value) }
            }
            scene_migration_set(&fields,"rigid_body",descriptor)
        }
        scene_migration_remove(&fields,"rigid_body_properties")
        if collider,present:=scene_gameplay_present(fields,"collider_shape"); present {
            kind,collider_payload,valid:=scene_variant(collider); if !valid { return nil,.Decode_Failed }
            if kind=="Trimesh" || kind=="ConvexHull" { object,is_object:=collider_payload.(json.Object); if !is_object || !recipe_keys(object,{"mesh_handle_index","mesh_handle_generation"}) || !scene_gameplay_required(object,{"mesh_handle_index","mesh_handle_generation"}) { return nil,.Decode_Failed }; for value in object { integer,is_integer:=object[value].(json.Integer); if !is_integer || integer<0 || integer>i64(max(u32)) { return nil,.Decode_Failed } }; scene_migration_set(&fields,"collider_shape",strings.clone(kind)) }
        }
        if script,present:=scene_gameplay_present(fields,"script"); present {
            object,is_object:=script.(json.Object); if !is_object || !recipe_keys(object,{"script_path"}) { return nil,.Decode_Failed }; path,is_path:=object["script_path"].(string); if !is_path { return nil,.Decode_Failed }
            reference:json.Value
            if !strings.contains(path,"/") && !strings.contains(path,"\\") {
                extension:=filepath.ext(path); basename:=path[:len(path)-len(extension)]; source_path:=strings.concatenate({"scripts/",basename,".luau"}); reference=trigger_json_value(struct {Resource:string}{source_path}); delete(source_path)
            } else { reference=scene_migration_asset(path) }
            descriptor:=make(json.Object,allocator); descriptor[strings.clone("path")]=reference; scene_migration_set(&fields,"script",descriptor)
        }
        if audio,present:=scene_gameplay_present(fields,"audio_emitter"); present {
            object,is_object:=audio.(json.Object); if !is_object || !recipe_keys(object,{"source_path","volume","looping","playing","spatial","min_distance","max_distance","rolloff_factor","distance_model"}) { return nil,.Decode_Failed }; path,is_path:=object["source_path"].(string); if !is_path { return nil,.Decode_Failed }
            reference:=scene_migration_asset(path); scene_migration_remove(&object,"source_path"); scene_migration_set(&object,"path",reference)
        }
    }
    scene_migration_set(&root,"version",json.Integer(3)); scene_migration_set(&root,"next_entity_id",scene_document_key_value(u64(len(entities)+1)))
    success=true; return root,.None
}
/// Reads an actual older/current document through the one canonical scene admission path.
scene_document_prepare :: proc(owner:^Authoring,document:json.Value,origin:string="")->(Scene_Snapshot,editor.Scene_Error) {
    converted,error:=scene_document_migrate(document,owner.world.allocator); if error!=.None { return {},error }; defer json.destroy_value(converted)
    return scene_document_decode(owner,converted,origin)
}
