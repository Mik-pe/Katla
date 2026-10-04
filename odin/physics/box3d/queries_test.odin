#+test
package box3d
import "core:testing"
@(private="file")
native_raycast_nearest_sensors_inside_filters_and_motion :: proc(t:^testing.T) {
    backend:Backend; if !test_native_init(t,&backend) { return }; defer testing.expect_value(t,backend_destroy(&backend),Error.None)
    bodies:=[3]Body{test_body(max(u64),.Fixed,position={0,0,-5}),test_body(2,.Fixed,position={0,0,-2}),test_body(3,.Dynamic,position={4,0,0})}
    bodies[1].sensor=1; bodies[2].gravity_scale=0
    testing.expect_value(t,backend_sync(&backend,bodies[:]),Error.None)
    result:=backend_raycast(&backend,{0,0,0},{0,0,-7},10)
    testing.expect(t,result.error==.None && result.ray.hit==1 && result.ray.id==2 && abs(result.ray.distance-1.5)<0.001)
    solid:=backend_raycast(&backend,{0,0,0},{0,0,-1},10,include_sensors=false)
    testing.expect(t,solid.error==.None && solid.ray.id==max(u64) && abs(solid.ray.distance-4.5)<0.001 && solid.ray.normal[2]>0.99)
    inside:=backend_raycast(&backend,{0,0,-5},{0,0,-1},10,include_sensors=false)
    testing.expect(t,inside.error==.None && inside.ray.hit==1 && inside.ray.id==max(u64) && inside.ray.distance==0)
    filtered:=backend_raycast(&backend,{0,0,0},{0,0,-1},10,mask=0)
    testing.expect(t,filtered.error==.None && filtered.ray.hit==0)
    testing.expect_value(t,backend_raycast(&backend,{0,0,0},{0,0,0},10).error,Error.Invalid)
    testing.expect_value(t,backend_motion(&backend,3,.Set_Velocity,{1,0,0}),Error.None)
    testing.expect_value(t,backend_motion(&backend,3,.Impulse,{1,0,0}),Error.None)
    testing.expect_value(t,backend_motion(&backend,3,.Force,{1,0,0}),Error.None)
    result_step:=backend_step(&backend,0.1); testing.expect(t,result_step.error==.None)
    for pose in result_step.poses { if pose.id==3 { testing.expect(t,pose.linear_velocity[0]>2 && pose.position[0]>4.2) } }; step_destroy(&result_step)
    testing.expect_value(t,backend_sync(&backend,bodies[:1]),Error.None)
    missing:=backend_raycast(&backend,{0,0,0},{0,0,-1},3); testing.expect(t,missing.error==.None && missing.ray.hit==0)
    testing.expect_value(t,backend_motion(&backend,3,.Force,{1,0,0}),Error.Invalid)
}
when NATIVE_LIBRARY!="" {
@(test)
test_native_raycast_nearest_sensors_inside_filters_and_motion :: proc(t:^testing.T) { native_raycast_nearest_sensors_inside_filters_and_motion(t) }
}
