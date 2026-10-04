#+test
package app
import "core:testing"
import "core:mem"
import "core:encoding/json"
import ecs "../ecs"

@(test)
test_scene_gameplay_document_roundtrip_and_generational_rule_restore :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner)
    source:string=`{"version":3,"name":"Gameplay","next_entity_id":3,"entities":[
        {"id":1,"name":"Entrance","source":"Empty","transform":{},"rigid_body":{"kind":"Kinematic","gravity_scale":0.5,"ccd_enabled":true,"linear_velocity":[1,2,3]},"collider_shape":{"Box":[1,2,3]},"physics_material":{"friction":0.7,"restitution":0.2,"density":2},"collision_filter":{"layers":4,"mask":8},"trigger_volume":"TriggerVolumeDescriptor","particle_emitter":{"emit_rate":0,"active":false,"shape":"Sphere","shape_params":[2,0,0,0],"timed_emission":1.25},"script":{"path":{"Resource":"scripts/prefab-effect.luau"}},"trigger_rules":[{"event":"enter","other_entity":2,"once":true,"actions":[{"action":"set_particles_active","target":{"kind":"entity","entity":1},"active":true},{"action":"burst_particles","target":{"kind":"trigger"},"count":32},{"action":"emit","name":"activated"}]}]},
        {"id":2,"name":"Actor","source":"Empty","transform":{},"velocity":{"velocity":[1,0,0],"acceleration":[0,-1,0]},"animation":{"current_clip":null,"playing":false,"loop_animation":true,"speed":2,"time":0,"duration":0,"blend_weight":1}}
    ]}`
    tree,parse_error:=json.parse(transmute([]byte)source,spec=.JSON,parse_integers=true); testing.expect(t,parse_error==nil)
    snapshot,decode_error:=scene_document_decode(&owner,tree); testing.expect(t,decode_error==.None && len(snapshot.entities)==2); json.destroy_value(tree)
    testing.expect(t,scene_snapshot_restore(&owner,&snapshot)==.None); scene_snapshot_destroy(&snapshot)
    ids:=ecs.entity_ids(&owner.world); trigger,actor:ecs.Entity_Id
    for entity in ids { name,present:=ecs.get_component(&owner.world,entity,Scene_Name); if present { if name.name=="Entrance" { trigger=entity } else if name.name=="Actor" { actor=entity } } }; delete(ids)
    rules:=ecs.get_component_mut(&owner.world,trigger,Trigger_Rules); testing.expect(t,rules!=nil && rules.rules[0].other==actor && rules.rules[0].actions[0].target.entity==trigger)
    body:=ecs.get_component_mut(&owner.world,trigger,Physics_Body); testing.expect(t,body!=nil && body.density==2 && body.gravity_scale==0.5 && body.ccd && body.sensor && body.layers==4 && body.mask==8)
    particles:=ecs.get_component_mut(&owner.world,trigger,Particle_Emitter); testing.expect(t,particles!=nil && particles.descriptor.shape==.Sphere && particles.descriptor.has_timed_emission && particles.descriptor.timed_emission==1.25)
    captured,capture_error:=scene_snapshot_capture(&owner); testing.expect(t,capture_error==.None)
    encoded:=make(json.Array,len(captured.entities))
    for row,i in captured.entities {
        fields:=make(json.Object); testing.expect(t,scene_builtin_components_encode(&owner,row,&fields)==.None); encoded[i]=fields
        rebuilt:=Scene_Entity{key=row.key,components=make([dynamic]Scene_Component,owner.world.allocator)}
        testing.expect(t,scene_builtin_components_decode(&owner,&rebuilt,fields)==.None)
        rebuilt_snapshot:=Scene_Snapshot{entities=make([dynamic]Scene_Entity,owner.world.allocator),allocator=owner.world.allocator}; append(&rebuilt_snapshot.entities,rebuilt); scene_snapshot_destroy(&rebuilt_snapshot)
        if row.key==1 { descriptor,is_rules:=fields["trigger_rules"].(json.Array); testing.expect(t,is_rules && len(descriptor)==1); if is_rules && len(descriptor)==1 { rule,_:=descriptor[0].(json.Object); other,is_integer:=rule["other_entity"].(json.Integer); testing.expect(t,is_integer && other==2) } }
    }
    json.destroy_value(json.Value(encoded)); scene_snapshot_destroy(&captured); authoring_destroy(&owner)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(test)
test_scene_gameplay_rejects_unimplemented_physics_and_invalid_descriptors :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); register_test_scene_runtime(&owner)
    for source in ([]string{`{"rigid_body":{"kind":"Dynamic"}}`,`{"collider_shape":{"Sphere":0}}`,`{"collider_shape":"Heightfield"}`,`{"physics_material":{"density":-1},"collider_shape":{"Sphere":1}}`,`{"animation":{"playing":true}}`,`{"particle_emitter":{"base_lifetime":0}}`,`{"joint":{}}`}) {
        tree,parse_error:=json.parse(transmute([]byte)source,spec=.JSON,parse_integers=true); testing.expect(t,parse_error==nil); fields,_:=tree.(json.Object)
        row:=Scene_Entity{key=1,components=make([dynamic]Scene_Component,owner.world.allocator)}; error:=scene_builtin_components_decode(&owner,&row,fields); testing.expect(t,error!=.None)
        snapshot:=Scene_Snapshot{entities=make([dynamic]Scene_Entity,owner.world.allocator),allocator=owner.world.allocator}; append(&snapshot.entities,row); scene_snapshot_destroy(&snapshot); json.destroy_value(tree)
    }
}
