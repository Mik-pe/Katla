#+test
//! Native constraints prove complete-batch ownership, physical spring coefficients and hinge frames.
package box3d
import "core:testing"
import "core:math"

@(thread_local) failed_publish_count:int
@(thread_local) original_publish:proc "c"(rawptr)->i32
@(private="file")
fail_second_publish :: proc "c"(value:rawptr)->i32 { failed_publish_count+=1; if failed_publish_count==2 { return 0 }; return original_publish(value) }
@(private="file")
joint_bodies :: proc()->[2]Body {
    result:=[2]Body{test_body(1,.Kinematic),test_body(max(u64),.Dynamic,position={0,-1,0})}
    result[0].radius=0.1; result[1].radius=0.1; return result
}
@(private="file")
native_joint_variants :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    for kind in ([4]Joint_Kind{.PointToPoint,.Hinge,.Fixed,.Distance}) {
        testing.expect_value(t,backend_reset(&b),Error.None)
        bodies:=joint_bodies(); joint:=Joint{id=max(u64),a=1,b=max(u64),kind=kind,anchor_a={0,-1,0}}
        if kind==.Hinge { joint.has_limits=1; joint.limits={-0.5,0.5} }
        if kind==.Distance { joint.anchor_a={}; joint.has_limits=1; joint.limits={0.5,1.5} }
        testing.expect_value(t,backend_sync(&b,bodies[:],{joint}),Error.None)
        native:=b.joints[joint.id].native
        for _ in 0..<180 {
            testing.expect_value(t,backend_sync(&b,bodies[:],{joint}),Error.None)
            testing.expect(t,b.joints[joint.id].native==native)
            step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step)
        }
        result:=backend_step(&b,1.0/60); testing.expect(t,len(result.poses)==2 && result.poses[1].position[1] > -1.3 && result.poses[1].position[1] < -0.8); step_destroy(&result)
        testing.expect_value(t,backend_sync(&b,bodies[:]),Error.None); testing.expect_value(t,len(b.joints),0)
        for _ in 0..<60 { step:=backend_step(&b,1.0/60); step_destroy(&step) }
        released:=backend_step(&b,1.0/60); testing.expect(t,released.poses[1].position[1] < -3); step_destroy(&released)
    }
}
@(private="file")
native_hinge_axis_and_limits :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    for axis in 0..<3 {
        testing.expect_value(t,backend_reset(&b),Error.None)
        bodies:=joint_bodies(); bodies[1].gravity_scale=0
        bodies[1].rotation={0,0,0,math.cos(f32(0.3))}; bodies[1].rotation[axis]=math.sin(f32(0.3))
        joint:=Joint{id=9,a=1,b=max(u64),kind=.Hinge,anchor_a={0,-1,0},has_limits=1,limits={-0.2,0.2}}
        testing.expect_value(t,backend_sync(&b,bodies[:],{joint}),Error.None)
        for _ in 0..<180 { step:=backend_step(&b,1.0/60); step_destroy(&step) }
        result:=backend_step(&b,1.0/60); q:=result.poses[1].rotation
        testing.expect(t,abs(q[0])<0.02 && abs(q[2])<0.02)
        if axis==1 { testing.expect(t,abs(q[1])<0.13 && abs(q[1])>0.05) } else { testing.expect(t,abs(q[1])<0.02) }
        step_destroy(&result)
    }
}
@(private="file")
native_spring_physical_coefficients :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    extensions:[2]f32
    for density,i in ([2]f32{1,8}) {
        testing.expect_value(t,backend_reset(&b),Error.None); bodies:=joint_bodies(); bodies[1].density=density; bodies[1].gravity_scale=4
        joint:=Joint{id=8,a=1,b=max(u64),kind=.Distance,has_limits=1,limits={0.5,1.5}}
        testing.expect_value(t,backend_sync(&b,bodies[:],{joint}),Error.None)
        for _ in 0..<360 { step:=backend_step(&b,1.0/60); step_destroy(&step) }
        result:=backend_step(&b,1.0/60); extensions[i]= -result.poses[1].position[1]-1; step_destroy(&result)
        expected:=f32(4.0/3.0*math.PI)*0.001*density*9.81*4
        testing.expect(t,abs(extensions[i]-expected)<0.02)
    }
    testing.expect(t,extensions[1]>extensions[0]*6 && extensions[1]<extensions[0]*10)
}
@(private="file")
native_joint_whole_batch_rejection :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    bodies:=joint_bodies(); joint:=Joint{id=7,a=1,b=max(u64),kind=.Fixed,anchor_a={0,-1,0}}
    testing.expect_value(t,backend_sync(&b,bodies[:],{joint}),Error.None)
    old_a,old_b,old_joint:=b.entries[1].native,b.entries[max(u64)].native,b.joints[7].native
    changed:=bodies; changed[0].position={4,0,0}; invalid:=joint; invalid.b=123
    testing.expect_value(t,backend_sync(&b,changed[:],{invalid}),Error.Invalid)
    invalid=joint; invalid.kind=.Hinge; invalid.has_limits=1; invalid.limits={4,-4}
    testing.expect_value(t,backend_sync(&b,changed[:],{invalid}),Error.Invalid)
    invalid=joint; invalid.kind=.Distance; invalid.has_limits=1; invalid.limits={math.nan_f32(),1}
    testing.expect_value(t,backend_sync(&b,changed[:],{invalid}),Error.Invalid)
    testing.expect(t,b.entries[1].native==old_a && b.entries[max(u64)].native==old_b && b.joints[7].native==old_joint)
    original_publish=b.publish_joint; b.publish_joint=fail_second_publish; failed_publish_count=0
    fresh:=joint; fresh.id=8; more:=joint; more.id=9
    testing.expect_value(t,backend_sync(&b,changed[:],{joint,fresh,more}),Error.Native)
    b.publish_joint=original_publish
    testing.expect(t,len(b.entries)==2 && len(b.joints)==1 && b.entries[1].native==old_a && b.entries[max(u64)].native==old_b && b.joints[7].native==old_joint && b.valid_joint(old_joint)!=0)
    result:=backend_step(&b,1.0/60); testing.expect(t,result.error==.None && result.poses[0].position[0]==0 && abs(result.poses[1].position[1]+1)<0.02); step_destroy(&result)
    changed=bodies; changed[1].radius=0.15
    testing.expect_value(t,backend_sync(&b,changed[:],{joint}),Error.None)
    testing.expect(t,b.entries[max(u64)].native!=old_b && b.joints[7].native!=old_joint)
    testing.expect_value(t,backend_sync(&b,changed[:1]),Error.None); testing.expect(t,len(b.joints)==0 && len(b.entries)==1)
    testing.expect_value(t,backend_reset(&b),Error.None); testing.expect(t,len(b.joints)==0 && len(b.entries)==0)
}
when NATIVE_LIBRARY!="" {
@(test)
test_native_joint_variants_preserve_handles_and_release_real_motion :: proc(t:^testing.T) { native_joint_variants(t) }
@(test)
test_native_joint_hinge_uses_y_axis_and_enforces_native_limits :: proc(t:^testing.T) { native_hinge_axis_and_limits(t) }
@(test)
test_native_joint_distance_preserves_physical_stiffness_and_damping :: proc(t:^testing.T) { native_spring_physical_coefficients(t) }
@(test)
test_native_joint_whole_batch_preflight_and_publish_failure_rollback :: proc(t:^testing.T) { native_joint_whole_batch_rejection(t) }
}

when NATIVE_LIBRARY!="" {
@(test)
test_native_joint_owner_replacement_preserves_completed_angular_motion :: proc(t:^testing.T) {
    reference,replaced:Backend
    if !test_native_init(t,&reference) { return }; defer testing.expect_value(t,backend_destroy(&reference),Error.None)
    if !test_native_init(t,&replaced) { return }; defer testing.expect_value(t,backend_destroy(&replaced),Error.None)
    bodies:=joint_bodies(); bodies[1].position={0.3,-1,0}; bodies[1].gravity_scale=0
    joint:=Joint{id=7,a=1,b=max(u64),kind=.PointToPoint,anchor_b={0,1,0}}
    for owner in ([2]^Backend{&reference,&replaced}) {
        testing.expect_value(t,backend_sync(owner,bodies[:],{joint}),Error.None)
        for _ in 0..<12 { step:=backend_step(owner,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step) }
        testing.expect_value(t,backend_sync(owner,bodies[:]),Error.None)
    }
    old:=replaced.entries[max(u64)].native; changed:=bodies; changed[1].radius=0.11
    testing.expect_value(t,backend_sync(&replaced,changed[:]),Error.None)
    testing.expect(t,replaced.entries[max(u64)].native!=old)
    before:Pose; reference.pose(reference.entries[max(u64)].native,&before)
    for _ in 0..<12 {
        actual:=backend_step(&replaced,1.0/120); expected:=backend_step(&reference,1.0/120)
        for rotation,i in actual.poses[1].rotation { testing.expect(t,abs(rotation-expected.poses[1].rotation[i])<1e-5) }
        step_destroy(&actual); step_destroy(&expected)
    }
    after:Pose; reference.pose(reference.entries[max(u64)].native,&after)
    testing.expect(t,abs(before.rotation[2]-after.rotation[2])>0.001)
}
@(test)
test_native_joint_cycles_restore_dependency_heap :: proc(t:^testing.T) {
    observer:Backend; if !test_native_init(t,&observer) { return }; defer testing.expect_value(t,backend_destroy(&observer),Error.None)
    serial :: #config(ODIN_TEST_THREADS,0)==1
    baseline:=observer.native_bytes()
    for _ in 0..<8 {
        b:Backend; if !test_native_init(t,&b) { return }
        bodies:=joint_bodies(); joint:=Joint{id=max(u64),a=1,b=max(u64),kind=.Distance,has_limits=1,limits={0.5,1.5}}
        testing.expect_value(t,backend_sync(&b,bodies[:],{joint}),Error.None)
        step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step)
        testing.expect_value(t,backend_reset(&b),Error.None)
        testing.expect_value(t,backend_sync(&b,bodies[:],{joint}),Error.None)
        testing.expect_value(t,backend_destroy(&b),Error.None)
        if serial { testing.expect_value(t,observer.native_bytes(),baseline) }
    }
}
}
