#+test
package box3d

import "core:testing"

@(test)
test_none_shape_preflight_requires_no_geometry :: proc(t:^testing.T) {
    body:=test_body(max(u64),.Dynamic,true,{1,3,2}); body.shape_kind=.None; body.radius=0; body.half_height=0; body.half_extents={}
    testing.expect(t,body_valid(body))
    points:=[3][3]f32{{0,0,0},{1,0,0},{0,1,0}}
    body.vertices=raw_data(points[:]); body.vertex_count=3; testing.expect(t,!body_valid(body))
    body.vertices=nil; body.vertex_count=0; body.indices=cast([^]u32)raw_data(points[:]); testing.expect(t,!body_valid(body))
    body.indices=nil; body.shape_kind=Shape_Kind(6); testing.expect(t,!body_valid(body))
    body.shape_kind=.None; body.rotation={0,0,0,0}; testing.expect(t,!body_valid(body))
}

when NATIVE_LIBRARY!="" {
@(test)
test_native_none_body_motion_and_null_shape_updates :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    moving:=test_body(max(u64),.Dynamic,true,{1,3,2}); moving.shape_kind=.None; moving.radius=0; moving.linear_velocity={2,0,0}; moving.rotation={0,0.70710677,0,0.70710677}
    fixed:=moving; fixed.id=7; fixed.body_type=.Fixed; fixed.linear_velocity={}
    kinematic:=moving; kinematic.id=8; kinematic.body_type=.Kinematic; kinematic.linear_velocity={1,0,0}
    sensor:=test_body(9,.Fixed,true,{1,3,2}); sensor.radius=5
    bodies:=[4]Body{moving,fixed,kinematic,sensor}
    testing.expect_value(t,backend_sync(&b,bodies[:]),Error.None)
    original:=b.entries[max(u64)].native; testing.expect(t,original!=nil)
    for _ in 0..<60 {
        step:=backend_step(&b,1.0/60); testing.expect(t,step.error==.None && len(step.poses)==4 && len(step.overlaps)==0 && len(step.events)==0); step_destroy(&step)
    }
    pose:Pose; b.pose(original,&pose)
    testing.expect(t,abs(pose.position[0]-3)<1e-4 && pose.position[1]==3 && pose.position[2]==2 && pose.linear_velocity==moving.linear_velocity)
    before:=pose
    bodies[0].density=4; bodies[0].friction=0.8; bodies[0].restitution=0.3; bodies[0].layers=2; bodies[0].mask=0; bodies[0].ccd=1; bodies[0].sensor=0; bodies[0].radius=9; bodies[0].half_extents={3,4,5}
    testing.expect_value(t,backend_sync(&b,bodies[:]),Error.None)
    testing.expect(t,b.entries[max(u64)].native==original)
    b.pose(original,&pose); testing.expect(t,pose.position==before.position && pose.rotation==before.rotation && pose.linear_velocity==before.linear_velocity)
    bodies[0].position={10,20,30}; bodies[0].linear_velocity={3,0,0}
    testing.expect_value(t,backend_sync(&b,bodies[:]),Error.None)
    b.pose(original,&pose); testing.expect(t,pose.position==bodies[0].position && pose.linear_velocity==bodies[0].linear_velocity)
    step:=backend_step(&b,1.0/60); defer step_destroy(&step)
    testing.expect(t,step.error==.None && len(step.overlaps)==0)
    for p in step.poses { if p.id==7 { testing.expect(t,p.position==fixed.position) }; if p.id==8 { testing.expect(t,p.position[0]>2 && p.position[1]==3) } }
}

@(test)
test_native_collider_none_transitions_keep_pose_and_remove_sensor_membership :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    sensor:=test_body(1,.Fixed,true); visitor:=test_body(2,.Fixed,true,{0.5,0,0})
    testing.expect_value(t,backend_sync(&b,{sensor,visitor}),Error.None)
    initial:=backend_step(&b,1.0/60); testing.expect(t,initial.error==.None && len(initial.overlaps)==2 && len(initial.events)==2); step_destroy(&initial)
    sensor.shape_kind=.None; sensor.radius=0
    testing.expect_value(t,backend_sync(&b,{sensor,visitor}),Error.None)
    removed:=backend_step(&b,1.0/60); testing.expect(t,removed.error==.None && len(removed.poses)==2 && len(removed.overlaps)==0 && len(removed.events)==2)
    for event in removed.events { testing.expect_value(t,event.phase,Phase.Exit) }; step_destroy(&removed)
    stable:=b.entries[1].native; sensor.density=3; sensor.layers=4; sensor.mask=0
    testing.expect_value(t,backend_sync(&b,{sensor,visitor}),Error.None); testing.expect(t,b.entries[1].native==stable)
    sensor.shape_kind=.Sphere; sensor.radius=0.5; sensor.layers=max(u32); sensor.mask=max(u32)
    testing.expect_value(t,backend_sync(&b,{sensor,visitor}),Error.None)
    restored:=backend_step(&b,1.0/60); testing.expect(t,restored.error==.None && len(restored.overlaps)==2 && len(restored.events)==2)
    for event in restored.events { testing.expect_value(t,event.phase,Phase.Enter) }; step_destroy(&restored)
    moving:=test_body(max(u64),.Dynamic,position={3,6,2}); moving.gravity_scale=0; moving.linear_velocity={2,0,0}
    testing.expect_value(t,backend_sync(&b,{moving}),Error.None)
    for _ in 0..<12 { step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step) }
    before:Pose; b.pose(b.entries[moving.id].native,&before)
    for _ in 0..<8 {
        moving.shape_kind=.None; moving.radius=0
        testing.expect_value(t,backend_sync(&b,{moving}),Error.None)
        naked:=b.entries[moving.id].native; pose:Pose; b.pose(naked,&pose); testing.expect(t,pose.position==before.position && pose.linear_velocity==before.linear_velocity)
        moving.friction+=0.01; moving.density+=0.25; testing.expect_value(t,backend_sync(&b,{moving}),Error.None)
        b.pose(naked,&pose); testing.expect(t,b.entries[moving.id].native==naked && pose.position==before.position && pose.linear_velocity==before.linear_velocity)
        moving.shape_kind=.Sphere; moving.radius=0.5
        testing.expect_value(t,backend_sync(&b,{moving}),Error.None)
        b.pose(b.entries[moving.id].native,&pose); testing.expect(t,pose.position==before.position && pose.linear_velocity==before.linear_velocity)
        step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step)
        b.pose(b.entries[moving.id].native,&before)
    }
    testing.expect(t,before.position[0]>3.6 && before.position[1]==6)
    testing.expect_value(t,backend_reset(&b),Error.None)
    empty:=backend_step(&b,1.0/60); testing.expect(t,empty.error==.None && len(empty.poses)==0 && len(empty.overlaps)==0); step_destroy(&empty)
}

@(test)
test_native_none_body_wrappers_release_dependency_heap :: proc(t:^testing.T) {
    observer:Backend; if !test_native_init(t,&observer) { return }; defer testing.expect_value(t,backend_destroy(&observer),Error.None)
    bytes:=observer.native_bytes
    // This process-wide counter is asserted only in the serialized native acceptance run.
    serial :: #config(ODIN_TEST_THREADS,0)==1
    baseline:=bytes()
    for cycle in 0..<8 {
        b:Backend; if !test_native_init(t,&b) { return }
        body:=test_body(u64(cycle),.Dynamic,true,{0,2,0}); body.shape_kind=.None; body.radius=0
        testing.expect_value(t,backend_sync(&b,{body}),Error.None)
        step:=backend_step(&b,1.0/60); testing.expect(t,step.error==.None && len(step.poses)==1 && len(step.overlaps)==0); step_destroy(&step)
        testing.expect_value(t,backend_destroy(&b),Error.None)
        if serial { testing.expect_value(t,bytes(),baseline) }
    }
}
}

LEGACY_LIBRARY :: #config(BOX3D_LEGACY_LIBRARY,"")
when LEGACY_LIBRARY!="" {
@(test)
test_legacy_library_abi_rejects_before_creating_body_owner :: proc(t:^testing.T) {
    b:Backend
    testing.expect_value(t,backend_init(&b,LEGACY_LIBRARY),Error.ABI)
    testing.expect(t,b.instance==nil && b.library==nil && len(b.entries)==0)
}
}
