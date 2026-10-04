#+test
//! Joint restoration and rejection exercise real scene generation maps before native synchronization.
package app
import "core:testing"
import "core:mem"
import "core:encoding/json"
import ron "../encoding/ron"
import ecs "../ecs"
import km "../math"

@(test)
test_physics_joint_document_restore_remaps_and_rejects_invalid_endpoints_atomically :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner)
    source:string=`{"version":3,"name":"Joint","next_entity_id":4,"entities":[
        {"id":1,"source":"Empty","transform":{},"rigid_body":{"kind":"Kinematic"},"collider_shape":{"Sphere":0.1}},
        {"id":2,"source":"Empty","transform":{"position":[0,-1,0]},"rigid_body":{"kind":"Dynamic"},"collider_shape":{"Sphere":0.1}},
        {"id":3,"source":"Empty","transform":{},"joint":{"kind":"Hinge","a":1,"b":2,"anchor_a":[0,-1,0],"anchor_b":[0,0,0],"limits":[-0.5,0.5]}}
    ]}`
    tree,parse_error:=json.parse(transmute([]byte)source,spec=.JSON,parse_integers=true); testing.expect(t,parse_error==nil)
    snapshot,decode_error:=scene_document_decode(&owner,tree); json.destroy_value(tree); testing.expect(t,decode_error==.None)
    testing.expect(t,scene_snapshot_restore(&owner,&snapshot)==.None); scene_snapshot_destroy(&snapshot)
    collected,error:=physics_collect_joints(&owner); testing.expect(t,error==.None && len(collected)==1)
    joint:=collected[0].joint; joint_entity:=ecs.Entity_Id(collected[0].id); delete(collected)
    testing.expect(t,ecs.entity_exists(&owner.world,joint.a) && ecs.entity_exists(&owner.world,joint.b) && joint.has_limits && joint.limits[0] == -0.5)
    captured,capture_error:=scene_snapshot_capture(&owner); testing.expect(t,capture_error==.None)
    for row in captured.entities { if scene_row_has(row,"PhysicsJoint") { fields:=make(json.Object); testing.expect(t,scene_builtin_components_encode(&owner,row,&fields)==.None); object,_:=fields["joint"].(json.Object); a,a_valid:=object["a"].(json.Integer); b,b_valid:=object["b"].(json.Integer); testing.expect(t,a_valid && b_valid && a==1 && b==2); json.destroy_value(json.Value(fields)) } }
    testing.expect(t,scene_snapshot_restore(&owner,&captured)==.None); scene_snapshot_destroy(&captured)
    second,second_error:=physics_collect_joints(&owner); testing.expect(t,second_error==.None && len(second)==1 && second[0].joint.a!=joint.a && second[0].joint.b!=joint.b && !ecs.entity_exists(&owner.world,joint_entity))
    next:=second[0].joint; current_joint:=ecs.Entity_Id(second[0].id); delete(second)
    participant:=ecs.get_component_mut(&owner.world,next.b,Physics_Body); participant.body_type=.Fixed
    invalid,invalid_error:=physics_collect_joints(&owner); testing.expect(t,invalid_error==.Invalid_Operation && len(invalid)==0); delete(invalid)
    malformed,malformed_error:=scene_snapshot_capture(&owner); testing.expect(t,malformed_error==.None)
    before_count:=owner.world.live_count; testing.expect(t,scene_snapshot_restore(&owner,&malformed)==.Invalid_Operation); testing.expect(t,owner.world.live_count==before_count && ecs.entity_exists(&owner.world,current_joint)); scene_snapshot_destroy(&malformed)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(private="file")
physics_joints_native_acceptance :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner); testing.expect(t,scene_runtime_init(&owner,RUNTIME_LIBRARY)==.None)
    anchor:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,anchor,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&owner.world,anchor,physics_body(Physics_Shape{kind=.Sphere,radius=0.1},.Kinematic))
    bob:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,bob,Scene_Transform{km.transform(position={0,-1,0})}); ecs.add_component(&owner.world,bob,physics_body(Physics_Shape{kind=.Sphere,radius=0.1}))
    joint:=Physics_Joint{kind=.PointToPoint,a=anchor,b=bob,anchor_a={0,-1,0}}; ecs.add_component(&owner.world,bob,joint)
    testing.expect(t,execute_test_simulation(t,&owner,.Play)==.None)
    for kind in ([4]Physics_Joint_Kind{.PointToPoint,.Hinge,.Fixed,.Distance}) {
        joint.kind=kind; joint.anchor_a={0,-1,0}; joint.has_limits=kind==.Hinge; joint.limits={-0.5,0.5}
        if kind==.Distance { joint.anchor_a={}; joint.has_limits=true; joint.limits={0.5,1.5} }
        ecs.get_component_mut(&owner.world,bob,Physics_Joint)^=joint
        ecs.get_component_mut(&owner.world,bob,Scene_Transform).local.position={0,-1,0}; ecs.get_component_mut(&owner.world,bob,Physics_Body).linear_velocity={}
        for _ in 0..<120 { testing.expect(t,simulation_step(&owner,1.0/60)==.None) }
        position:=ecs.get_component_mut(&owner.world,bob,Scene_Transform).local.position
        testing.expect(t,position[1] > -1.3 && position[1] < -0.8 && abs(position[0])<0.05 && abs(position[2])<0.05)
    }
    ecs.remove_component(&owner.world,bob,Physics_Joint)
    before:=ecs.get_component_mut(&owner.world,bob,Scene_Transform).local.position[1]
    for _ in 0..<60 { testing.expect(t,simulation_step(&owner,1.0/60)==.None) }
    testing.expect(t,ecs.get_component_mut(&owner.world,bob,Scene_Transform).local.position[1]<before-2)
    testing.expect(t,execute_test_simulation(t,&owner,.Stop)==.None)
    restored,error:=physics_collect_joints(&owner); testing.expect(t,error==.None && len(restored)==1 && restored[0].joint.kind==.PointToPoint && restored[0].joint.a!=anchor && restored[0].joint.b!=bob); delete(restored)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when RUNTIME_LIBRARY!="" {
@(test)
test_physics_native_rapier_joint_constraints_sync_and_play_stop :: proc(t:^testing.T) { physics_joints_native_acceptance(t) }
}

@(test)
test_scene_full_u64_trigger_and_joint_document_wire_restore :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner)
    source:string=`(version:3,name:"Unsigned gameplay",next_entity_id:18446744073709551615,entities:[
        (id:9223372036854775808,source:Empty,transform:(),rigid_body:(kind:Kinematic),collider_shape:Sphere(0.1),trigger_volume:TriggerVolumeDescriptor,trigger_rules:[(event:"enter",other_entity:18446744073709551614,once:true,actions:[(action:"set_particles_active",target:(kind:"entity",entity:18446744073709551614),active:true)])]),
        (id:18446744073709551614,source:Empty,transform:(),rigid_body:(kind:Dynamic),collider_shape:Sphere(0.1),particle_emitter:()),
        (id:1,source:Empty,transform:(),joint:(kind:PointToPoint,a:9223372036854775808,b:18446744073709551614,anchor_a:[0,0,0],anchor_b:[0,0,0]))
    ])`
    tree,parse_error:=ron.parse(source); testing.expect(t,parse_error.kind==.None)
    snapshot,decode_error:=scene_document_decode(&owner,tree); json.destroy_value(tree); testing.expect(t,decode_error==.None)
    testing.expect(t,scene_snapshot_restore(&owner,&snapshot)==.None); scene_snapshot_destroy(&snapshot)
    captured,capture_error:=scene_snapshot_capture(&owner); testing.expect(t,capture_error==.None && captured.next_entity_id==max(u64))
    document,encode_error:=scene_document_encode(&owner,&captured,"Unsigned gameplay","scene.katla"); testing.expect(t,encode_error==.None)
    bytes,write_error:=ron.write(document); json.destroy_value(document); testing.expect(t,write_error.kind==.None)
    reread,read_error:=ron.parse(string(bytes)); delete(bytes); testing.expect(t,read_error.kind==.None)
    replacement,replacement_error:=scene_document_decode(&owner,reread); json.destroy_value(reread); testing.expect(t,replacement_error==.None)
    before,collect_error:=physics_collect_joints(&owner); testing.expect(t,collect_error==.None && len(before)==1); old_a,old_b:=before[0].joint.a,before[0].joint.b; delete(before)
    testing.expect(t,scene_snapshot_restore(&owner,&replacement)==.None); scene_snapshot_destroy(&replacement); scene_snapshot_destroy(&captured)
    after,after_error:=physics_collect_joints(&owner); testing.expect(t,after_error==.None && len(after)==1)
    if len(after)==1 { joint:=after[0].joint; testing.expect(t,joint.a!=old_a && joint.b!=old_b); rules,present:=ecs.get_component(&owner.world,joint.a,Trigger_Rules); testing.expect(t,present && rules.rules[0].other==joint.b && rules.rules[0].actions[0].target.entity==joint.b) }; delete(after)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
