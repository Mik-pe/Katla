#+test
package app
import "core:testing"
import ecs "../ecs"
import km "../math"
@(private="file")
native_box_queries_resolve_hierarchy_and_motion :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); register_test_scene_runtime(&owner)
    testing.expect(t,physics_select_box3d(&owner,BOX3D_LIBRARY)==.None)
    parent:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,parent,Scene_Transform{km.transform(position={0,0,-3})})
    solid:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,solid,Scene_Transform{km.transform(position={0,0,-2})}); ecs.add_component(&owner.world,solid,Scene_Parent{parent}); ecs.add_component(&owner.world,solid,physics_body(Physics_Shape{kind=.Box,half_extents={1,1,1}},.Fixed))
    sensor:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,sensor,Scene_Transform{km.transform(position={0,0,-2})}); body:=physics_body(Physics_Shape{kind=.Sphere,radius=0.25},.Fixed); body.sensor=true; ecs.add_component(&owner.world,sensor,body)
    moving_entity:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,moving_entity,Scene_Transform{km.transform(position={5,0,0})}); moving:=physics_body(Physics_Shape{kind=.Sphere,radius=0.5}); moving.gravity_scale=0; ecs.add_component(&owner.world,moving_entity,moving)
    cube,geometry_error:=mesh_cube({1,1,1},owner.world.allocator); testing.expect(t,geometry_error==.None)
    hull:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,hull,Scene_Transform{km.transform(position={8,0,0})}); ecs.add_component(&owner.world,hull,Scene_Mesh{geometry=cube}); ecs.add_component(&owner.world,hull,physics_body({kind=.ConvexHull},.Fixed))
    testing.expect(t,physics_prepare(&owner)==.None)
    edges,edge_error:=physics_collider_edges(&owner,hull); defer delete(edges); testing.expect(t,edge_error==.None && len(edges)==12)
    points:=make([][3]f32,len(cube.vertices),owner.world.allocator); defer delete(points); for vertex,i in cube.vertices { points[i]=vertex.position }
    query:=physics_query_shape({kind=.ConvexHull},{8,0,3},vertices=points,indices=cube.indices)
    cast_result:=physics_shape_cast(&owner,query,{0,0,-1},10); testing.expect(t,cast_result.error==.None && cast_result.hit && cast_result.entity==hull && abs(cast_result.distance-2)<0.011)
    query.origin={8,0,0}; overlaps,overlap_error:=physics_shape_overlaps(&owner,query); defer delete(overlaps); testing.expect(t,overlap_error==.None && len(overlaps)==1 && overlaps[0]==hull)
    hit:=physics_raycast(&owner,{0,0,0},{0,0,-1},10,include_sensors=false)
    testing.expect(t,hit.error==.None && hit.hit && hit.entity==solid && abs(hit.distance-4)<0.001)
    visitor:=physics_raycast(&owner,{0,0,0},{0,0,-1},10)
    testing.expect(t,visitor.error==.None && visitor.hit && visitor.entity==sensor && abs(visitor.distance-1.75)<0.001)
    testing.expect(t,physics_apply_motion(&owner,moving_entity,.Set_Velocity,{2,0,0})==.None)
    testing.expect(t,physics_apply_motion(&owner,moving_entity,.Impulse,{1,0,0})==.None)
    result:=physics_step(&owner,0.1); defer physics_step_result_destroy(&result)
    testing.expect(t,result.error==.None && ecs.get_component_mut(&owner.world,moving_entity,Scene_Transform).local.position[0]>5.3)
    testing.expect(t,ecs.get_component_mut(&owner.world,moving_entity,Physics_Body).linear_velocity[0]>3)
    testing.expect(t,physics_reset(&owner)==.None)
    cleared:=physics_raycast(&owner,{0,0,0},{0,0,-1},10); testing.expect(t,cleared.error==.None && !cleared.hit)
}
when BOX3D_LIBRARY!="" {
@(test)
test_physics_native_box_queries_resolve_hierarchy_and_motion :: proc(t:^testing.T) { native_box_queries_resolve_hierarchy_and_motion(t) }
}
