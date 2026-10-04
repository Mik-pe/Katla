#+test
//! Box3D app composition owns generational joint references through genuine preview and restore.
package app
import "core:testing"
import "core:mem"
import ecs "../ecs"
import km "../math"

@(private="file")
box3d_joints_app_acceptance :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner); testing.expect(t,physics_select_box3d(&owner,BOX3D_LIBRARY)==.None)
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
    ecs.remove_component(&owner.world,bob,Physics_Joint); before:=ecs.get_component_mut(&owner.world,bob,Scene_Transform).local.position[1]
    for _ in 0..<60 { testing.expect(t,simulation_step(&owner,1.0/60)==.None) }
    testing.expect(t,ecs.get_component_mut(&owner.world,bob,Scene_Transform).local.position[1]<before-2)
    testing.expect(t,execute_test_simulation(t,&owner,.Stop)==.None)
    restored,error:=physics_collect_joints(&owner); testing.expect(t,error==.None && len(restored)==1 && restored[0].joint.kind==.PointToPoint && restored[0].joint.a!=anchor && restored[0].joint.b!=bob); delete(restored)
    testing.expect(t,execute_test_simulation(t,&owner,.Play)==.None && simulation_step(&owner,0.1)==.None && execute_test_simulation(t,&owner,.Stop)==.None)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when BOX3D_LIBRARY!="" {
@(test)
test_physics_native_box3d_all_joint_variants_and_preview_restore :: proc(t:^testing.T) { box3d_joints_app_acceptance(t) }
}
