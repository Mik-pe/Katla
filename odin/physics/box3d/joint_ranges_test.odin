#+test
package box3d
import "core:testing"
import "core:math"

@(private="file")
range_bodies :: proc()->[2]Body {
    result:=[2]Body{test_body(1,.Kinematic),test_body(max(u64),.Dynamic,position={0,-1,0})}
    result[0].radius=0.1; result[1].radius=0.1; return result
}

@(private="file")
native_hinge_periodic_intervals_cross_branch_and_outside_principal_angle :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    ranges:=[6][2]f32{{3,4},{4,5},{-5,-4},{-4,-3},{-.999*f32(math.PI),.999*f32(math.PI)},{0,0}}
    for limits in ranges {
        testing.expect_value(t,backend_reset(&b),Error.None)
        center:=(f64(limits[0])+f64(limits[1]))*0.5; half:=(f64(limits[1])-f64(limits[0]))*0.5
        initial:=center+half+0.15
        bodies:=range_bodies(); bodies[1].gravity_scale=0; bodies[1].mask=0
        bodies[1].rotation={0,f32(math.sin(initial*0.5)),0,f32(math.cos(initial*0.5))}
        joint:=Joint{id=11,a=1,b=max(u64),kind=.Hinge,anchor_a={0,-1,0},has_limits=1,limits=limits}
        testing.expect_value(t,backend_sync(&b,bodies[:],{joint}),Error.None)
        for _ in 0..<240 { step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step) }
        pose:Pose; b.pose(b.entries[max(u64)].native,&pose)
        angle:=2*math.atan2(f64(pose.rotation[1]),f64(pose.rotation[3]))
        offset:=math.atan2(math.sin(angle-center),math.cos(angle-center))
        testing.expect(t,abs(offset)<=half+0.025)
        testing.expect(t,abs(pose.rotation[0])<0.01 && abs(pose.rotation[2])<0.01)
    }
}
@(private="file")
native_hinge_full_turn_limits_remain_free_and_near_pi_bounds_are_unclamped :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    ranges:=[4][2]f32{{-4,4},{-f32(math.PI),f32(math.PI)},{4,11},{-.999*f32(math.PI),.999*f32(math.PI)}}
    for limits in ranges {
        testing.expect_value(t,backend_reset(&b),Error.None)
        initial:=f32(0.996*math.PI)
        bodies:=range_bodies(); bodies[1].gravity_scale=0; bodies[1].mask=0
        bodies[1].rotation={0,math.sin(initial*0.5),0,math.cos(initial*0.5)}
        joint:=Joint{id=11,a=1,b=max(u64),kind=.Hinge,anchor_a={0,-1,0},has_limits=1,limits=limits}
        testing.expect_value(t,backend_sync(&b,bodies[:],{joint}),Error.None)
        for _ in 0..<30 { step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step) }
        pose:Pose; b.pose(b.entries[max(u64)].native,&pose)
        angle:=2*math.atan2(pose.rotation[1],pose.rotation[3])
        testing.expect(t,abs(angle-initial)<0.001)
    }
}
@(private="file")
native_distance_zero_and_subslop_rest_lengths_reach_exact_equilibrium :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    for rest in ([3]f32{0,0.001,0.004}) {
        testing.expect_value(t,backend_reset(&b),Error.None)
        bodies:=range_bodies(); bodies[1].position={1,0,0}; bodies[1].gravity_scale=0; bodies[1].mask=0; bodies[1].radius=0.5
        joint:=Joint{id=11,a=1,b=max(u64),kind=.Distance,has_limits=1,limits={rest,rest}}
        testing.expect_value(t,backend_sync(&b,bodies[:],{joint}),Error.None)
        // Keep the weak spring awake without adding a force or replacing integration.
        for _ in 0..<1200 {
            testing.expect_value(t,backend_motion(&b,max(u64),.Force,{}),Error.None)
            step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step)
        }
        pose:Pose; b.pose(b.entries[max(u64)].native,&pose)
        testing.expect(t,abs(abs(pose.position[0])-rest)<0.0002)
        testing.expect(t,abs(pose.position[1])+abs(pose.position[2])<0.0001)
    }
}
@(private="file")
native_hinge_extreme_finite_point_intervals :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    for target in ([3]f32{1e30,-max(f32),max(f32)}) {
        testing.expect_value(t,backend_reset(&b),Error.None)
        bodies:=range_bodies(); bodies[1].gravity_scale=0; bodies[1].mask=0
        joint:=Joint{id=11,a=1,b=max(u64),kind=.Hinge,anchor_a={0,-1,0},has_limits=1,limits={target,target}}
        testing.expect_value(t,backend_sync(&b,bodies[:],{joint}),Error.None)
        for _ in 0..<240 { step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step) }
        pose:Pose; b.pose(b.entries[max(u64)].native,&pose)
        expected_y:=math.sin(f64(target)*0.5); expected_w:=math.cos(f64(target)*0.5)
        alignment:=abs(f64(pose.rotation[1])*expected_y+f64(pose.rotation[3])*expected_w)
        testing.expect(t,alignment>0.9999 && abs(pose.rotation[0])<0.01 && abs(pose.rotation[2])<0.01)
    }
}

when NATIVE_LIBRARY!="" {
@(test)
test_native_hinge_extreme_finite_point_intervals_reduce_phase_without_overflow :: proc(t:^testing.T) { native_hinge_extreme_finite_point_intervals(t) }
@(test)
test_native_hinge_periodic_intervals_cross_branch_and_outside_principal_angle :: proc(t:^testing.T) { native_hinge_periodic_intervals_cross_branch_and_outside_principal_angle(t) }
@(test)
test_native_hinge_full_turn_limits_remain_free_and_near_pi_bounds_are_unclamped :: proc(t:^testing.T) { native_hinge_full_turn_limits_remain_free_and_near_pi_bounds_are_unclamped(t) }
@(test)
test_native_distance_zero_and_subslop_rest_lengths_reach_exact_equilibrium :: proc(t:^testing.T) { native_distance_zero_and_subslop_rest_lengths_reach_exact_equilibrium(t) }
}
