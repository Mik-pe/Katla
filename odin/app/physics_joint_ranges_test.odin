#+test
//! Full periodic limits reach native constraints and survive real Play/Stop reference replacement.
package app
import "core:testing"
import "core:mem"
import "core:math"
import ecs "../ecs"
import km "../math"
import box3d "../physics/box3d"

@(private="file")
joint_ranges_app_acceptance :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner); testing.expect(t,physics_select_box3d(&owner,BOX3D_LIBRARY)==.None)
    anchor:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,anchor,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&owner.world,anchor,physics_body(Physics_Shape{kind=.Sphere,radius=0.1},.Kinematic))
    bob:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,bob,Scene_Transform{km.transform(position={0,-1,0})})
    body:=physics_body(Physics_Shape{kind=.Sphere,radius=0.1}); body.gravity_scale=0; body.mask=0; ecs.add_component(&owner.world,bob,body)
    joint:=Physics_Joint{kind=.Hinge,a=anchor,b=bob,anchor_a={0,-1,0},has_limits=true,limits={3,4}}; ecs.add_component(&owner.world,bob,joint)
    testing.expect(t,execute_test_simulation(t,&owner,.Play)==.None)
    for limits in ([4][2]f32{{3,4},{4,5},{-5,-4},{-4,4}}) {
        joint.limits=limits; ecs.get_component_mut(&owner.world,bob,Physics_Joint)^=joint
        for _ in 0..<180 { testing.expect(t,simulation_step(&owner,1.0/60)==.None) }
        pose,error:=box3d.backend_pose(ecs.get_resource_mut(&owner.world,box3d.Backend),u64(bob)); testing.expect(t,error==.None)
        center:=f64(limits[0]+limits[1])*0.5; half:=f64(limits[1]-limits[0])*0.5
        angle:=2*math.atan2(f64(pose.rotation[1]),f64(pose.rotation[3])); offset:=math.atan2(math.sin(angle-center),math.cos(angle-center))
        testing.expect(t,half>=math.PI || abs(offset)<half+0.025)
    }
    joint.kind=.Distance; joint.anchor_a={}; joint.limits={0,0}; ecs.get_component_mut(&owner.world,bob,Physics_Joint)^=joint
    for _ in 0..<60 { testing.expect(t,simulation_step(&owner,1.0/60)==.None) }
    testing.expect(t,execute_test_simulation(t,&owner,.Stop)==.None)
    restored,error:=physics_collect_joints(&owner); testing.expect(t,error==.None && len(restored)==1)
    if len(restored)==1 {
        original:=restored[0].joint; testing.expect(t,original.kind==.Hinge && original.limits==[2]f32{3,4} && original.a!=anchor && original.b!=bob)
    }
    delete(restored)
    testing.expect(t,execute_test_simulation(t,&owner,.Play)==.None && simulation_step(&owner,0.1)==.None && execute_test_simulation(t,&owner,.Stop)==.None)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when BOX3D_LIBRARY!="" {
@(test)
test_physics_native_box3d_full_joint_ranges_and_preview_restore :: proc(t:^testing.T) { joint_ranges_app_acceptance(t) }
}
