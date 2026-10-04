#+test
//! Authored participant presence drives genuine optional native colliders and exact restore.
package app
import "core:testing"
import "core:mem"
import ecs "../ecs"
import km "../math"

@(test)
test_physics_metadata_remains_owned_without_native_participation :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); register_test_scene_runtime(&owner)
    entity:=ecs.create_entity(&owner.world)
    metadata:=physics_body(Physics_Shape{kind=.None},.Fixed); metadata.has_rigid_body=false; metadata.has_material=true; metadata.has_filter=true; metadata.friction=0.8; metadata.layers=4; metadata.mask=8
    ecs.add_component(&owner.world,entity,metadata)
    collected,error:=physics_collect(&owner); defer physics_collected_destroy(&collected,owner.world.allocator)
    testing.expect(t,error==.None && len(collected)==0)
    testing.expect(t,execute_test_simulation(t,&owner,.Play)==.None)
    testing.expect(t,simulation_step(&owner,0.1)==.None)
    testing.expect(t,execute_test_simulation(t,&owner,.Stop)==.None)
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    found:=false; for id in ids { value,present:=ecs.get_component(&owner.world,id,Physics_Body); if present { found=!value.has_rigid_body && !value.has_collider && value.has_material && value.has_filter && value.friction==0.8 && value.layers==4 && value.mask==8 } }
    testing.expect(t,found && !ecs.entity_exists(&owner.world,entity))
}
@(private="file")
physics_optional_native_acceptance :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner)
    testing.expect(t,physics_select_box3d(&owner,BOX3D_LIBRARY)==.None)
    floor_body:=physics_body(Physics_Shape{kind=.Box,half_extents={5,0.1,5}},.Dynamic); floor_body.has_rigid_body=false
    floor:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,floor,Scene_Transform{km.transform(position={0,-0.1,0})}); ecs.add_component(&owner.world,floor,floor_body)
    body:=physics_body(Physics_Shape{kind=.None}); body.linear_velocity={0.1,0,0}
    ball:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,ball,Scene_Transform{km.transform(position={0,2,0})}); ecs.add_component(&owner.world,ball,body)
    initial,error:=physics_collect(&owner); testing.expect(t,error==.None && len(initial)==2)
    for resolved in initial { if resolved.id==u64(floor) { testing.expect(t,resolved.body.body_type==.Fixed) } else { testing.expect(t,resolved.body.shape.kind==.None && len(resolved.vertices)==0 && len(resolved.indices)==0) } }; physics_collected_destroy(&initial,owner.world.allocator)
    testing.expect(t,execute_test_simulation(t,&owner,.Play)==.None)
    for _ in 0..<5 { result:=physics_step(&owner,1.0/60); testing.expect(t,result.error==.None && len(result.events)==0); physics_step_result_destroy(&result) }
    ecs.get_component_mut(&owner.world,ball,Scene_Transform).local.position={0,2,0}
    current:=ecs.get_component_mut(&owner.world,ball,Physics_Body); current.linear_velocity={}; current.has_collider=true; current.shape={kind=.Sphere,radius=0.25}
    for _ in 0..<180 { testing.expect(t,simulation_step(&owner,1.0/60)==.None) }
    y:=ecs.get_component_mut(&owner.world,ball,Scene_Transform).local.position[1]; testing.expect(t,y>0.15 && y<0.35)
    current=ecs.get_component_mut(&owner.world,ball,Physics_Body); current.has_collider=false; current.shape={kind=.None}
    testing.expect(t,simulation_step(&owner,1.0/60)==.None)
    testing.expect(t,execute_test_simulation(t,&owner,.Stop)==.None)
    ids:=ecs.entity_ids(&owner.world); found_body,found_floor:=false,false
    for id in ids { restored,present:=ecs.get_component(&owner.world,id,Physics_Body); if present { if restored.has_rigid_body { found_body=!restored.has_collider && restored.shape.kind==.None && restored.linear_velocity[0]==0.1 } else { found_floor=restored.has_collider && restored.body_type==.Dynamic } } }; delete(ids)
    testing.expect(t,found_body && found_floor && !ecs.entity_exists(&owner.world,ball) && !ecs.entity_exists(&owner.world,floor))
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when BOX3D_LIBRARY!="" {
@(test)
test_physics_native_box3d_optional_collider_presence_and_restore :: proc(t:^testing.T) { physics_optional_native_acceptance(t) }
}
