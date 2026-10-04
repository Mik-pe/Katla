#+test
package box3d
import "core:testing"
import "core:math"

when NATIVE_LIBRARY!="" {
@(test)
test_native_heightfield_centered_row_major_rotation_contact_and_replacement :: proc(t:^testing.T) {
    observer:Backend; if !test_native_init(t,&observer) { return }; defer testing.expect_value(t,backend_destroy(&observer),Error.None)
    baseline:=observer.native_bytes(); testing.expect(t,baseline>0)
    b:Backend; if !test_native_init(t,&b) { return }
    defer { testing.expect_value(t,backend_destroy(&b),Error.None); when #config(ODIN_TEST_THREADS,0)==1 { testing.expect_value(t,observer.native_bytes(),baseline) } }
    heights:=[6]f32{0,1,2,0,1,2}
    field:=test_body(max(u64),.Fixed,position={10,2,-3}); field.shape_kind=.Heightfield; field.radius=0
    field.heights=raw_data(heights[:]); field.rows=2; field.cols=3; field.height_scale={1.5,2,2}; field.rotation={0,0.70710678,0,0.70710678}
    testing.expect_value(t,backend_sync(&b,{field}),Error.None)
    original:=b.entries[field.id].native
    heights[1]=99
    testing.expect_value(t,b.entries[field.id].body.heights[1],f32(1)); heights[1]=1
    testing.expect_value(t,backend_sync(&b,{field}),Error.None); testing.expect_value(t,b.entries[field.id].native,original)
    ray:=backend_raycast(&b,{10,20,-3},{0,-1,0},30)
    testing.expect(t,ray.error==.None && ray.ray.hit==1 && ray.ray.id==max(u64) && abs(ray.ray.point[1]-4)<0.001)
    left:=backend_raycast(&b,{10,20,-1.501},{0,-1,0},30)
    testing.expect(t,left.error==.None && left.ray.hit==1 && abs(left.ray.point[1]-2.00133)<0.001)
    outside:=backend_raycast(&b,{10,20,-1.4},{0,-1,0},30); testing.expect(t,outside.error==.None && outside.ray.hit==0)
    pose,error:=backend_pose(&b,field.id); testing.expect(t,error==.None && math.abs(pose.position[0]-10)<0.0001 && abs(pose.position[2]+3)<0.0001)
    changed:=field; changed.rows=1; testing.expect_value(t,backend_sync(&b,{changed}),Error.Invalid); testing.expect_value(t,b.entries[field.id].native,original)
    changed=field; changed.body_type=.Dynamic; testing.expect_value(t,backend_sync(&b,{changed}),Error.None); testing.expect(t,b.entries[field.id].native!=original)
    testing.expect_value(t,backend_sync(&b,{field}),Error.None)
    for &height in heights { height=3 }; testing.expect_value(t,backend_sync(&b,{field}),Error.None); testing.expect(t,b.entries[field.id].native!=original)
    ray=backend_raycast(&b,{10,20,-3},{0,-1,0},30); testing.expect(t,ray.error==.None && ray.ray.hit==1 && abs(ray.ray.point[1]-8)<0.001)
    ball:=test_body(2,.Dynamic,position={10,11,-3}); testing.expect_value(t,backend_sync(&b,{field,ball}),Error.None)
    for _ in 0..<240 { step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step) }
    settled,settled_error:=backend_pose(&b,2); testing.expect(t,settled_error==.None && abs(settled.position[1]-8.5)<0.03)
    contacts,contact_error:=backend_contacts(&b); defer delete(contacts)
    testing.expect(t,contact_error==.None && len(contacts)>0)
    for contact in contacts { testing.expect(t,contact.a==2 && contact.b==max(u64) && abs(contact.point[1]-8)<0.03 && contact.normal[1]<-0.99 && contact.normal_impulse>0 && abs(contact.separation)<0.03) }
    testing.expect_value(t,backend_reset(&b),Error.None); testing.expect_value(t,len(b.entries),0)
    empty_contacts,empty_error:=backend_contacts(&b); defer delete(empty_contacts); testing.expect(t,empty_error==.None && len(empty_contacts)==0 && len(contacts)>0)
}
@(test)
test_native_exact_shape_casts_overlap_filters_concave_and_large_hull :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    target:=test_body(max(u64),.Fixed,position={0,0,-5}); target.shape_kind=.Box; target.half_extents={0.5,0.5,0.5}
    sensor:=target; sensor.id=7; sensor.sensor=1; sensor.position={0,0,-2}
    testing.expect_value(t,backend_sync(&b,{target,sensor}),Error.None)
    query:=test_body(0,.Fixed); query.radius=0.5
    for kind in ([3]Shape_Kind{.Sphere,.Box,.Capsule}) {
        query.shape_kind=kind; query.half_extents={0.5,0.5,0.5}; query.half_height=0.5
        cast_result:=backend_shape_cast(&b,query,{0,0,-7},10,include_sensors=false)
        testing.expect(t,cast_result.error==.None && cast_result.ray.hit==1 && cast_result.ray.id==max(u64) && abs(cast_result.ray.distance-4.0/7)<0.011 && abs(cast_result.ray.point[2]+4)<0.011 && cast_result.ray.normal[2]>0.99)
    }
    query.shape_kind=.Sphere
    closest:=backend_shape_cast(&b,query,{0,0,-1},10); testing.expect(t,closest.error==.None && closest.ray.id==7 && abs(closest.ray.distance-1)<0.011)
    miss:=backend_shape_cast(&b,query,{0,0,-1},10,mask=0); testing.expect(t,miss.error==.None && miss.ray.hit==0)
    query.position={0,0,-5}; inside:=backend_shape_cast(&b,query,{1,0,0},10,include_sensors=false); testing.expect(t,inside.error==.None && inside.ray.id==max(u64) && inside.ray.distance==0)
    ids,error:=backend_shape_overlaps(&b,query,include_sensors=false); defer delete(ids); testing.expect(t,error==.None && len(ids)==1 && ids[0]==max(u64))
    vertices:=[6][3]f32{{-2,-2,0},{-1,-2,0},{-2,2,0},{1,-2,0},{2,-2,0},{2,2,0}}; indices:=[6]u32{0,1,2,3,4,5}
    query.position={}; query.shape_kind=.Trimesh; query.vertices=raw_data(vertices[:]); query.vertex_count=6; query.indices=raw_data(indices[:]); query.index_count=6
    gap:=backend_shape_cast(&b,query,{0,0,-1},10,include_sensors=false); testing.expect(t,gap.error==.None && gap.ray.hit==0)
    query.position={1.3,0,-5}; mesh_ids,mesh_error:=backend_shape_overlaps(&b,query,include_sensors=false); defer delete(mesh_ids); testing.expect(t,mesh_error==.None && len(mesh_ids)==1)
    points:[80][3]f32
    for &point,i in points { angle:=2*math.PI*f32(i%40)/40; point={math.cos(angle)*2,f32(i/40)*4-2,math.sin(angle)*2} }
    query.shape_kind=.ConvexHull; query.position={0,0,-5}; query.vertices=raw_data(points[:]); query.vertex_count=80; query.indices=nil; query.index_count=0
    hull_ids,hull_error:=backend_shape_overlaps(&b,query,include_sensors=false); defer delete(hull_ids); testing.expect(t,hull_error==.None && len(hull_ids)==1 && hull_ids[0]==max(u64))
    hull_body:=query; hull_body.id=42; hull_body.position={4,0,0}; testing.expect_value(t,backend_sync(&b,{target,hull_body}),Error.None)
    edges,edge_error:=backend_hull_edges(&b,42); defer delete(edges); testing.expect(t,edge_error==.None && len(edges)==120)
    for edge in edges { testing.expect(t,edge.start!=edge.end && edge.start[0]>=1.999 && edge.start[0]<=6.001 && edge.end[0]>=1.999 && edge.end[0]<=6.001) }
    testing.expect_value(t,backend_sync(&b,{target}),Error.None); testing.expect(t,len(edges)==120)
    query.position={0,0,0}; hull_cast:=backend_shape_cast(&b,query,{0,0,-1},10,include_sensors=false)
    testing.expect(t,hull_cast.error==.None && hull_cast.ray.hit==1 && abs(hull_cast.ray.distance-2.5)<0.025)
}
}

@(test)
test_heightfield_preflight_and_owned_geometry_without_native_library :: proc(t:^testing.T) {
    heights:=[4]f32{0,1,2,3}; field:=test_body(1,.Fixed); field.shape_kind=.Heightfield; field.heights=raw_data(heights[:]); field.rows=2; field.cols=2; field.height_scale={2,1,2}
    testing.expect(t,body_valid(field)); owned:=body_clone(field,context.allocator); defer body_destroy(owned,context.allocator)
    heights[0]=9; testing.expect(t,!geometry_equal(field,owned) && owned.heights[0]==0)
    heights[0]=math.sqrt(f32(-1)); testing.expect(t,!body_valid(field)); heights[0]=0
    field.cols=max(u32); testing.expect(t,!body_valid(field)); field.cols=2
    field.height_scale[2]=0; testing.expect(t,!body_valid(field)); field.height_scale[2]=2
    field.shape_kind=.Sphere; testing.expect(t,!body_valid(field))
}
when NATIVE_LIBRARY!="" {
@(test)
test_native_moving_heightfield_exact_surfaces_contacts_and_ccd :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    heights:=[4]f32{}
    field:=test_body(1,.Dynamic); field.shape_kind=.Heightfield; field.rows=2; field.cols=2; field.heights=raw_data(heights[:]); field.height_scale={2,1,2}; field.linear_velocity={1,0,0}; field.ccd=1
    ball:=test_body(2,.Dynamic,position={0,2,0}); ball.radius=0.25
    testing.expect_value(t,backend_sync(&b,{field,ball}),Error.None)
    for _ in 0..<60 { step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step) }
    ground,ground_error:=backend_pose(&b,1); settled,settled_error:=backend_pose(&b,2)
    testing.expect(t,ground_error==.None && settled_error==.None && abs(ground.position[0]-1)<0.001 && abs(ground.position[1])<0.001 && abs(settled.position[1]-0.25)<0.03)
    contacts,error:=backend_contacts(&b); defer delete(contacts); testing.expect(t,error==.None && len(contacts)>0)
    ray:=backend_raycast(&b,{1,2,0},{0,-1,0},5,mask=max(u32)); testing.expect(t,ray.error==.None && ray.ray.hit==1)
}
}
when NATIVE_LIBRARY!="" {
@(test)
test_native_moving_mesh_closed_volume_density_contacts_and_concavity :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    vertices:=[8][3]f32{{-0.5,-0.5,-0.5},{0.5,-0.5,-0.5},{0.5,0.5,-0.5},{-0.5,0.5,-0.5},{-0.5,-0.5,0.5},{0.5,-0.5,0.5},{0.5,0.5,0.5},{-0.5,0.5,0.5}}
    indices:=[36]u32{0,2,1,0,3,2,4,5,6,4,6,7,0,1,5,0,5,4,3,7,6,3,6,2,0,4,7,0,7,3,1,2,6,1,6,5}
    mesh:=test_body(2,.Dynamic,position={0,3,0}); mesh.shape_kind=.Trimesh; mesh.vertices=raw_data(vertices[:]); mesh.vertex_count=8; mesh.indices=raw_data(indices[:]); mesh.index_count=36; mesh.density=2; mesh.ccd=1
    ground:=test_body(1,.Fixed,position={0,-0.5,0}); ground.shape_kind=.Box; ground.half_extents={5,0.5,5}
    testing.expect_value(t,backend_sync(&b,{mesh,ground}),Error.None)
    testing.expect_value(t,backend_motion(&b,2,.Impulse,{2,0,0}),Error.None)
    moved,moved_error:=backend_pose(&b,2); testing.expect(t,moved_error==.None && abs(moved.linear_velocity[0]-1)<0.001)
    mesh.density=4; testing.expect_value(t,backend_sync(&b,{mesh,ground}),Error.None)
    testing.expect_value(t,backend_motion(&b,2,.Impulse,{2,0,0}),Error.None)
    moved,moved_error=backend_pose(&b,2); testing.expect(t,moved_error==.None && abs(moved.linear_velocity[0]-1.5)<0.001)
    testing.expect_value(t,backend_motion(&b,2,.Set_Velocity,{}),Error.None)
    for _ in 0..<240 { step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step) }
    moved,moved_error=backend_pose(&b,2); testing.expect(t,moved_error==.None && abs(moved.position[1]-0.5)<0.04)
    contacts,error:=backend_contacts(&b); defer delete(contacts); testing.expect(t,error==.None && len(contacts)>0)
    gap_vertices:=[6][3]f32{{-2,-1,0},{-1,-1,0},{-2,1,0},{1,-1,0},{2,-1,0},{2,1,0}}; gap_indices:=[6]u32{0,1,2,3,4,5}
    gap:=test_body(3,.Kinematic,position={0,0,-3}); gap.shape_kind=.Trimesh; gap.vertices=raw_data(gap_vertices[:]); gap.vertex_count=6; gap.indices=raw_data(gap_indices[:]); gap.index_count=6
    testing.expect_value(t,backend_sync(&b,{gap}),Error.None)
    miss:=backend_raycast(&b,{0,0,0},{0,0,-1},10); testing.expect(t,miss.error==.None && miss.ray.hit==0)
    hit:=backend_raycast(&b,{-1.8,0,0},{0,0,-1},10); testing.expect(t,hit.error==.None && hit.ray.hit==1 && hit.ray.id==3 && abs(hit.ray.point[2]+3)<0.001)
    triangle_miss:=backend_raycast(&b,{-1.1,0.9,0},{0,0,-1},10); testing.expect(t,triangle_miss.error==.None && triangle_miss.ray.hit==0)
    query:=test_body(0,.Fixed); query.radius=0.1
    cast_result:=backend_shape_cast(&b,query,{0,0,-1},10); testing.expect(t,cast_result.error==.None && cast_result.ray.hit==0)
}

@(test)
test_native_moving_triangle_sensor_deduplication_and_fast_ccd :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    heights:=[4]f32{}
    field:=test_body(1,.Kinematic,true); field.shape_kind=.Heightfield; field.rows=2; field.cols=2; field.heights=raw_data(heights[:]); field.height_scale={4,1,4}
    visitor:=test_body(max(u64),.Fixed,position={0,0,0}); visitor.radius=0.5
    testing.expect_value(t,backend_sync(&b,{field,visitor}),Error.None)
    first:=backend_step(&b,1.0/60); defer step_destroy(&first)
    testing.expect(t,first.error==.None && len(first.overlaps)==1 && len(first.events)==1 && first.events[0].pair.other==max(u64))
    copied,error:=backend_trigger_overlaps(&b,1); defer delete(copied); testing.expect(t,error==.None && len(copied)==1 && copied[0]==max(u64))
    field.position={0,3,0}; testing.expect_value(t,backend_sync(&b,{field,visitor}),Error.None)
    outside:=backend_step(&b,1.0/60); defer step_destroy(&outside)
    testing.expect(t,outside.error==.None && len(outside.overlaps)==0 && len(outside.events)==1 && outside.events[0].phase==.Exit)
    field.sensor=0; field.position={}; field.body_type=.Dynamic
    bullet:=test_body(2,.Dynamic,position={0,3,0}); bullet.radius=0.1; bullet.ccd=1; bullet.linear_velocity={0,-200,0}
    testing.expect_value(t,backend_sync(&b,{field,bullet}),Error.None)
    for _ in 0..<4 { step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step) }
    result,result_error:=backend_pose(&b,2); testing.expect(t,result_error==.None && result.position[1]>0.08 && result.position[1]<0.13 && abs(result.linear_velocity[1])<0.05)
    contacts,contact_error:=backend_contacts(&b); defer delete(contacts); testing.expect(t,contact_error==.None && len(contacts)>0)
}
}
