#+test
package box3d

import "core:testing"

when NATIVE_LIBRARY!="" {
@(test)
test_native_mesh_floor_owned_hull_geometry_and_atomic_failed_replacement :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    vertices:=[4][3]f32{{-10,0,-10},{-10,0,10},{10,0,10},{10,0,-10}}
    indices:=[6]u32{0,1,2,0,2,3}
    ground:=test_body(1,.Fixed); ground.shape_kind=.Trimesh; ground.vertices=raw_data(vertices[:]); ground.vertex_count=4; ground.indices=raw_data(indices[:]); ground.index_count=6
    hull:=[8][3]f32{{-0.5,-0.5,-0.5},{-0.5,-0.5,0.5},{-0.5,0.5,-0.5},{-0.5,0.5,0.5},{0.5,-0.5,-0.5},{0.5,-0.5,0.5},{0.5,0.5,-0.5},{0.5,0.5,0.5}}
    cube:=test_body(max(u64),.Dynamic,position={0,3,0}); cube.shape_kind=.ConvexHull; cube.vertices=raw_data(hull[:]); cube.vertex_count=8; cube.density=2.5
    testing.expect_value(t,backend_sync(&b,{ground,cube}),Error.None)
    testing.expect(t,b.entries[1].body.vertices!=ground.vertices && b.entries[max(u64)].body.vertices!=cube.vertices)
    for _ in 0..<240 { step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step) }
    settled:=backend_step(&b,1.0/60); defer step_destroy(&settled)
    testing.expect(t,len(settled.poses)==2 && abs(settled.poses[1].position[1]-0.5)<0.04)
    prior:=b.entries[max(u64)].native
    bad_points:=[4][3]f32{{0,0,0},{1,0,0},{2,0,0},{3,0,0}}
    malformed:=cube; malformed.vertices=raw_data(bad_points[:]); malformed.vertex_count=4
    testing.expect_value(t,backend_sync(&b,{malformed}),Error.Native)
    testing.expect(t,len(b.entries)==2 && b.entries[max(u64)].native==prior)
    unsupported:=ground; unsupported.body_type=.Kinematic
    testing.expect_value(t,backend_sync(&b,{unsupported,cube}),Error.Invalid)
    testing.expect_value(t,len(b.entries),2)
    testing.expect_value(t,backend_sync(&b,{ground,cube}),Error.None)
    testing.expect(t,b.entries[max(u64)].native==prior)
    completed:Pose; b.pose(prior,&completed)
    naked:=cube; naked.shape_kind=.None; naked.vertices=nil; naked.vertex_count=0; naked.radius=0
    testing.expect_value(t,backend_sync(&b,{ground,naked}),Error.None)
    shapeless:=b.entries[max(u64)].native
    testing.expect(t,b.entries[max(u64)].body.vertices==nil && b.entries[max(u64)].body.vertex_count==0)
    pose:Pose; b.pose(shapeless,&pose); testing.expect(t,pose.position==completed.position && pose.linear_velocity==completed.linear_velocity)
    malformed.id=3
    testing.expect_value(t,backend_sync(&b,{ground,naked,malformed}),Error.Native)
    testing.expect(t,len(b.entries)==2 && b.entries[max(u64)].native==shapeless)
    testing.expect_value(t,backend_sync(&b,{ground,cube}),Error.None)
    b.pose(b.entries[max(u64)].native,&pose)
    testing.expect(t,pose.position==completed.position && pose.linear_velocity==completed.linear_velocity)
    testing.expect(t,b.entries[max(u64)].body.vertices!=nil && b.entries[max(u64)].body.vertices!=cube.vertices)
}
}

@(test)
test_mesh_preflight_rejects_missing_storage_and_unsupported_body_types :: proc(t:^testing.T) {
    body:=test_body(1,.Fixed); body.shape_kind=.Trimesh
    testing.expect(t,!body_valid(body))
    points:=[3][3]f32{{0,0,0},{1,0,0},{0,1,0}}; indices:=[3]u32{0,1,2}
    body.vertices=raw_data(points[:]); body.vertex_count=3; body.indices=raw_data(indices[:]); body.index_count=3
    testing.expect(t,body_valid(body)); body.body_type=.Dynamic; testing.expect(t,!body_valid(body))
    body.body_type=.Fixed; indices[2]=99; testing.expect(t,!body_valid(body))
}
