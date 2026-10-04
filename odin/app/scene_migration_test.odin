#+test
package app

import "core:testing"
import "core:encoding/json"
import "core:strings"
import ecs "../ecs"
import editor "../editor"
import resources "../resources"
import ron "../encoding/ron"

MIGRATION_V1 :: #load("scene_migration_fixtures/v1.katla",string)
MIGRATION_V1_DEFAULT :: #load("scene_migration_fixtures/v1-default.katla",string)
MIGRATION_V2_DEFAULT :: #load("scene_migration_fixtures/v2-default.katla",string)
@(test)
test_scene_migration_actual_older_documents_stage_current_owned_components :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    testing.expect_value(t,asset_resources_init(&owner,".","resources"),resources.Error.None)
    for text in ([3]string{MIGRATION_V1,MIGRATION_V1_DEFAULT,MIGRATION_V2_DEFAULT}) {
        tree,parse_error:=ron.parse(text); testing.expect(t,parse_error.kind==.None); if parse_error.kind!=.None { continue }; defer json.destroy_value(tree)
        snapshot,error:=scene_document_prepare(&owner,tree,"resources/default.katla"); testing.expect_value(t,error,editor.Scene_Error.None); defer scene_snapshot_destroy(&snapshot); if error!=.None { continue }
        testing.expect(t,len(snapshot.entities)>2 && snapshot.next_entity_id==u64(len(snapshot.entities)+1))
        testing.expect_value(t,scene_snapshot_restore(&owner,&snapshot),editor.Scene_Error.None)
        ids:=ecs.entity_ids(&owner.world); defer delete(ids)
        primitive_count:=0
        for id in ids { if model,present:=ecs.get_component(&owner.world,id,Scene_Model); present && model.source.kind==.Primitive { primitive_count+=1 } }
        testing.expect_value(t,len(ids),len(snapshot.entities)+primitive_count)
        if text!=MIGRATION_V1 { testing.expect_value(t,primitive_count,2) }
        mesh_count,light_count,script_count:int
        for id in ids {
            if _,present:=ecs.get_component(&owner.world,id,Scene_Mesh); present { mesh_count+=1 }
            if _,present:=ecs.get_component(&owner.world,id,Scene_Point_Light); present { light_count+=1 }
            if _,present:=ecs.get_component(&owner.world,id,Script_Component); present { script_count+=1 }
        }
        testing.expect(t,mesh_count>0 && light_count>0)
        if text==MIGRATION_V1 { testing.expect(t,script_count>0) }
        captured,capture_error:=scene_snapshot_capture(&owner); testing.expect_value(t,capture_error,editor.Scene_Error.None); defer scene_snapshot_destroy(&captured)
        encoded,encode_error:=scene_document_encode(&owner,&captured,"Migrated","resources/default.katla"); testing.expect_value(t,encode_error,editor.Scene_Error.None); defer json.destroy_value(encoded)
        fields:=encoded.(json.Object); version,_:=fields["version"].(json.Integer); testing.expect_value(t,version,json.Integer(3))
    }
}
@(test)
test_scene_migration_name_references_defaults_and_ambiguous_rejection :: proc(t:^testing.T) {
    text:=`(name:"Old",entities:[(name:"Root",transform:(),source:Cube(size:(1,1,1)),rigid_body:Dynamic,rigid_body_properties:(gravity_scale:0.25,ccd_enabled:true,linear_velocity:(1,2,3))),(name:"Light",parent:"Root",transform:(),source:Light,drawable:(color:(1,0,0,1),metallic:0,roughness:0.5,ao:1)),(name:"Particles",transform:(),source:ParticleEmitter),(name:"Volume",transform:(),source:Trigger,rigid_body:Static,collider_shape:Box((1,1,1)),trigger_volume:TriggerVolumeDescriptor,trigger_rules:[(event:"enter",other_entity:"Root",actions:[(action:"play_animation",target:(kind:"entity",entity:"Root"),clip:"Idle")])])])`
    tree,parse_error:=ron.parse(text); testing.expect(t,parse_error.kind==.None); defer json.destroy_value(tree)
    migrated,error:=scene_document_migrate(tree); testing.expect_value(t,error,editor.Scene_Error.None); defer json.destroy_value(migrated); if error!=.None { return }
    root:=migrated.(json.Object); entities:=root["entities"].(json.Array)
    light:=entities[1].(json.Object); parent,_:=scene_document_key(light["parent"]); testing.expect_value(t,parent,u64(1)); _,has_drawable:=light["drawable"]; testing.expect(t,!has_drawable)
    testing.expect(t,light["point_light"]!=nil && entities[2].(json.Object)["particle_emitter"]!=nil)
    rules:=entities[3].(json.Object)["trigger_rules"].(json.Array); rule:=rules[0].(json.Object); other,_:=scene_document_key(rule["other_entity"]); testing.expect_value(t,other,u64(1))
    original:=tree.(json.Object); _,changed:=original["next_entity_id"]; testing.expect(t,!changed)
    duplicate,_:=strings.replace(text,`name:"Particles"`,`name:"Root"`,1); defer delete(duplicate)
    missing,_:=strings.replace(text,`parent:"Root"`,`parent:"Missing"`,1); defer delete(missing)
    future,_:=strings.replace(text,`name:"Old"`,`version:4,name:"Old"`,1); defer delete(future)
    for invalid in ([3]string{duplicate,missing,future}) {
        parsed,err:=ron.parse(invalid); testing.expect(t,err.kind==.None); defer json.destroy_value(parsed)
        rejected,failure:=scene_document_migrate(parsed); defer json.destroy_value(rejected); testing.expect(t,failure!=.None && rejected==nil)
    }
}
