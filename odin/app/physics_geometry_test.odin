#+test
package app
import "core:testing"
import "core:mem"
import ecs "../ecs"
import km "../math"

@(test)
test_physics_mesh_affine_bake_preserves_source_and_releases_rejected_batch :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner)
    parent:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,parent,Scene_Transform{km.transform(position={4,2,1},scale={-2,3,1})})
    geometry,mesh_error:=mesh_cube({1,1,1},owner.world.allocator); testing.expect(t,mesh_error==.None)
    entity:=ecs.create_entity(&owner.world); local:=km.transform(position={1,0,0},rotation=km.quat_axis_angle(km.VEC3_Y,0.5)); ecs.add_component(&owner.world,entity,Scene_Transform{local}); ecs.add_component(&owner.world,entity,Scene_Parent{parent}); ecs.add_component(&owner.world,entity,Scene_Mesh{geometry=geometry}); ecs.add_component(&owner.world,entity,physics_body({kind=.Trimesh},.Fixed))
    collected,collect_error:=physics_collect(&owner); testing.expect(t,collect_error==.None && len(collected)==1)
    world,world_error:=scene_world_matrix(&owner,entity); testing.expect(t,world_error==.None)
    rigid:=km.mat4_trs(collected[0].position,km.Quat(collected[0].rotation),km.VEC3_ONE)
    for vertex,i in geometry.vertices { expected:=km.transform_point(world,vertex.position); actual:=km.transform_point(rigid,collected[0].vertices[i]); testing.expect(t,km.length(expected-actual)<0.00001) }
    testing.expect(t,collected[0].indices[0]==geometry.indices[0] && collected[0].indices[1]==geometry.indices[2] && collected[0].indices[2]==geometry.indices[1])
    pose:=[1]Physics_Resolved_Pose{{u64(entity),collected[0].position,collected[0].rotation,{0,0,0}}}; testing.expect(t,physics_commit_poses(&owner,pose[:])==.None); testing.expect(t,ecs.get_component_mut(&owner.world,entity,Scene_Transform).local==local)
    physics_collected_destroy(&collected,owner.world.allocator)
    broken:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,broken,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&owner.world,broken,physics_body({kind=.ConvexHull},.Fixed))
    invalid,invalid_error:=physics_collect(&owner); testing.expect(t,invalid==nil && invalid_error==.Component_Not_Found); physics_collected_destroy(&invalid,owner.world.allocator)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(test)
test_physics_dynamic_mesh_rejects_unrepresentable_hierarchy_and_bad_indices :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); register_test_scene_runtime(&owner)
    geometry,mesh_error:=mesh_cube({1,1,1},owner.world.allocator); testing.expect(t,mesh_error==.None)
    entity:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,entity,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&owner.world,entity,Scene_Mesh{geometry=geometry}); ecs.add_component(&owner.world,entity,physics_body({kind=.ConvexHull}))
    parent:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,parent,Scene_Transform{km.transform(scale={1,2,1})}); ecs.add_component(&owner.world,entity,Scene_Parent{parent})
    collected,err:=physics_collect(&owner); testing.expect(t,collected==nil && err==.Invalid_Operation); physics_collected_destroy(&collected,owner.world.allocator)
    ecs.remove_component(&owner.world,entity,Scene_Parent); mesh:=ecs.get_component_mut(&owner.world,entity,Scene_Mesh); mesh.geometry.indices[0]=u32(len(mesh.geometry.vertices))
    collected,err=physics_collect(&owner); testing.expect(t,collected==nil && err==.Invalid_Field_Value); physics_collected_destroy(&collected,owner.world.allocator)
}
