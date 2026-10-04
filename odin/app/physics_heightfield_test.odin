#+test
package app
import "core:testing"
import "core:mem"
import "core:encoding/json"
import ecs "../ecs"
import editor "../editor"
import km "../math"
import box3d "../physics/box3d"

@(test)
test_heightfield_document_owned_snapshot_roundtrip_and_rejection :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner)
    source:string=`{"version":3,"name":"Terrain","next_entity_id":2,"entities":[{"id":1,"source":"Empty","transform":{},"collider_shape":{"Heightfield":{"rows":2,"cols":3,"heights":[0,1,2,3,4,5]}}}]}`
    tree,parse_error:=json.parse(transmute([]byte)source,spec=.JSON,parse_integers=true); testing.expect(t,parse_error==nil)
    snapshot,decode_error:=scene_document_decode(&owner,tree); json.destroy_value(tree); testing.expect_value(t,decode_error,editor.Scene_Error.None)
    testing.expect_value(t,scene_snapshot_restore(&owner,&snapshot),editor.Scene_Error.None); scene_snapshot_destroy(&snapshot)
    ids:=ecs.entity_ids(&owner.world); testing.expect_value(t,len(ids),1); entity:=ids[0]; delete(ids)
    live:=ecs.get_component_mut(&owner.world,entity,Physics_Body); testing.expect(t,live!=nil && live.shape.rows==2 && live.shape.cols==3 && live.shape.heights[5]==5)
    captured,capture_error:=scene_snapshot_capture(&owner); testing.expect_value(t,capture_error,editor.Scene_Error.None)
    live.shape.heights[0]=99
    fields:=make(json.Object); testing.expect_value(t,scene_builtin_components_encode(&owner,captured.entities[0],&fields),editor.Scene_Error.None)
    collider,_:=fields["collider_shape"].(json.Object); field,_:=collider["Heightfield"].(json.Object); heights,_:=field["heights"].(json.Array)
    first,valid:=recipe_number(heights[0]); testing.expect(t,valid && first==0)
    rebuilt:=Scene_Entity{key=1,components=make([dynamic]Scene_Component,owner.world.allocator)}
    testing.expect_value(t,scene_row_component(&owner,&rebuilt,"SceneTransform",Scene_Transform{km.TRANSFORM_IDENTITY}),editor.Scene_Error.None)
    testing.expect_value(t,scene_builtin_components_decode(&owner,&rebuilt,fields),editor.Scene_Error.None); json.destroy_value(json.Value(fields))
    restored:=Scene_Snapshot{next_entity_id=captured.next_entity_id,entities=make([dynamic]Scene_Entity,owner.world.allocator),allocator=owner.world.allocator}; append(&restored.entities,rebuilt)
    testing.expect_value(t,scene_snapshot_restore(&owner,&restored),editor.Scene_Error.None); scene_snapshot_destroy(&restored); scene_snapshot_destroy(&captured)
    ids=ecs.entity_ids(&owner.world); live=ecs.get_component_mut(&owner.world,ids[0],Physics_Body); testing.expect(t,ids[0]!=entity && live.shape.heights[0]==0); delete(ids)
    for document in ([]string{`{"Heightfield":{"rows":1,"cols":2,"heights":[0,1]}}`,`{"Heightfield":{"rows":2,"cols":2,"heights":[0,1]}}`,`{"Heightfield":{"rows":4294967295,"cols":4294967295,"heights":[]}}`,`{"Heightfield":{"rows":2,"cols":2,"heights":[0,1,2,"bad"]}}`}) {
        invalid,parse_invalid:=json.parse(transmute([]byte)document,spec=.JSON,parse_integers=true); testing.expect(t,parse_invalid==nil)
        rejected,is_valid:=scene_gameplay_shape(invalid); testing.expect(t,!is_valid); delete(rejected.heights); json.destroy_value(invalid)
    }
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(private="file")
native_heightfield_hierarchy_queries_and_preview :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); register_test_scene_runtime(&owner)
    testing.expect_value(t,physics_select_box3d(&owner,BOX3D_LIBRARY),editor.Scene_Error.None)
    heights:=[6]f32{0,1,2,0,1,2}
    parent:=ecs.spawn(&owner.world,struct { transform:Scene_Transform }{Scene_Transform{km.Transform{position={10,2,-3},rotation=km.QUAT_IDENTITY,scale={1,2,1}}}})
    height_shape,height_shape_valid:=physics_heightfield(2,3,heights[:]); testing.expect(t,height_shape_valid)
    terrain:=ecs.spawn(&owner.world,struct { transform:Scene_Transform,parent:Scene_Parent,body:Physics_Body }{Scene_Transform{km.Transform{rotation={0,0.70710678,0,0.70710678},scale={1,1,1}}},Scene_Parent{parent},physics_body(height_shape,.Fixed)})
    heights[0]=99
    owned,_:=ecs.get_component(&owner.world,terrain,Physics_Body); testing.expect(t,owned.shape.heights[0]==0)
    testing.expect_value(t,physics_box3d_sync(&owner),editor.Scene_Error.None)
    ray:=physics_raycast(&owner,{10,20,-3},{0,-2,0},30)
    testing.expect(t,ray.error==.None && ray.hit && ray.entity==terrain && abs(ray.point[1]-4)<0.001)
    query:=physics_query_shape({kind=.Sphere,radius=0.5},{10,10,-3})
    cast_result:=physics_shape_cast(&owner,query,{0,-2,0},10)
    testing.expect(t,cast_result.error==.None && cast_result.hit && cast_result.entity==terrain && abs(cast_result.point[1]-4.83333)<0.02 && abs(cast_result.distance-2.58333)<0.01)
    query.origin={10,4,-3}; overlap,error:=physics_shape_overlaps(&owner,query); defer delete(overlap)
    testing.expect(t,error==.None && len(overlap)==1 && overlap[0]==terrain)
    rejected:=query; rejected.scale={0,1,1}; testing.expect_value(t,physics_shape_cast(&owner,rejected,{0,-1,0},10).error,editor.Scene_Error.Invalid_Field_Value)
    backend:=ecs.get_resource_mut(&owner.world,box3d.Backend); native:=backend.entries[u64(terrain)].native
    shape:=ecs.get_component_mut(&owner.world,terrain,Physics_Body); shape.body_type=.Kinematic
    testing.expect_value(t,physics_box3d_sync(&owner),editor.Scene_Error.None); testing.expect(t,backend.entries[u64(terrain)].native!=native); shape.body_type=.Fixed
    ball:=ecs.spawn(&owner.world,struct { transform:Scene_Transform,body:Physics_Body }{Scene_Transform{km.Transform{position={10,10,-3},rotation=km.QUAT_IDENTITY,scale={1,1,1}}},physics_body({kind=.Sphere,radius=0.5})})
    testing.expect_value(t,execute_test_simulation(t,&owner,.Play),editor.Scene_Error.None)
    for _ in 0..<120 { result:=physics_step(&owner,1.0/60); testing.expect_value(t,result.error,editor.Scene_Error.None); physics_step_result_destroy(&result) }
    contacts,contact_error:=physics_contacts(&owner); defer delete(contacts)
    testing.expect(t,contact_error==.None)
    testing.expect_value(t,execute_test_simulation(t,&owner,.Stop),editor.Scene_Error.None); testing.expect(t,!ecs.entity_exists(&owner.world,terrain) && !ecs.entity_exists(&owner.world,ball))
    ids:=ecs.entity_ids(&owner.world); defer delete(ids); found:=false
    for id in ids { if body,present:=ecs.get_component(&owner.world,id,Physics_Body); present && body.shape.kind==.Heightfield { found=true; testing.expect(t,len(body.shape.heights)==6 && body.shape.heights[0]==0 && body.shape.heights[5]==2) } }
    testing.expect(t,found)
}
when BOX3D_LIBRARY!="" {
@(test)
test_native_heightfield_hierarchy_queries_and_preview :: proc(t:^testing.T) { native_heightfield_hierarchy_queries_and_preview(t) }
}

@(private="file")
native_moving_concave_scene_and_preview :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner)
    testing.expect_value(t,physics_select_box3d(&owner,BOX3D_LIBRARY),editor.Scene_Error.None)
    heights:=[4]f32{}
    shape,valid:=physics_heightfield(2,2,heights[:]); testing.expect(t,valid)
    field_body:=physics_body(shape,.Dynamic); field_body.linear_velocity={0.5,0,0}; field_body.ccd=true
    field:=ecs.spawn(&owner.world,struct { transform:Scene_Transform,body:Physics_Body }{Scene_Transform{km.transform(scale={4,1,4})},field_body})
    geometry,geometry_error:=mesh_cube({1,1,1},owner.world.allocator); testing.expect_value(t,geometry_error,Mesh_Error.None)
    mesh_body:=physics_body({kind=.Trimesh},.Dynamic); mesh_body.density=2; mesh_body.ccd=true
    mesh:=ecs.spawn(&owner.world,struct { transform:Scene_Transform,body:Physics_Body,geometry:Scene_Mesh }{Scene_Transform{km.transform(position={0,3,0})},mesh_body,Scene_Mesh{geometry=geometry}})
    testing.expect_value(t,execute_test_simulation(t,&owner,.Play),editor.Scene_Error.None)
    testing.expect_value(t,physics_prepare(&owner),editor.Scene_Error.None)
    testing.expect_value(t,physics_apply_motion(&owner,mesh,.Impulse,{2,0,0}),editor.Scene_Error.None)
    testing.expect(t,abs(ecs.get_component_mut(&owner.world,mesh,Physics_Body).linear_velocity[0]-1)<0.001)
    testing.expect_value(t,physics_apply_motion(&owner,mesh,.Set_Velocity,{}),editor.Scene_Error.None)
    for _ in 0..<120 { testing.expect_value(t,simulation_step(&owner,1.0/60),editor.Scene_Error.None) }
    ground:=ecs.get_component_mut(&owner.world,field,Scene_Transform); settled:=ecs.get_component_mut(&owner.world,mesh,Scene_Transform)
    testing.expect(t,abs(ground.local.position[0]-1)<0.002 && abs(ground.local.position[1])<0.001 && settled.local.position[1]>0.49 && settled.local.position[1]<0.8)
    world,world_error:=scene_world_matrix(&owner,mesh); testing.expect_value(t,world_error,editor.Scene_Error.None)
    bottom:=max(f32); source_mesh,_:=ecs.get_component(&owner.world,mesh,Scene_Mesh)
    for vertex in source_mesh.geometry.vertices { bottom=min(bottom,km.transform_point(world,vertex.position)[1]) }; testing.expect(t,abs(bottom)<0.03)
    contacts,error:=physics_contacts(&owner); testing.expect(t,error==.None && len(contacts)>0); delete(contacts)
    testing.expect_value(t,execute_test_simulation(t,&owner,.Stop),editor.Scene_Error.None); testing.expect(t,!ecs.entity_exists(&owner.world,field) && !ecs.entity_exists(&owner.world,mesh))
    ids:=ecs.entity_ids(&owner.world); found_field,found_mesh:=false,false
    for id in ids {
        body,has_body:=ecs.get_component(&owner.world,id,Physics_Body); if !has_body { continue }
        transform,_:=ecs.get_component(&owner.world,id,Scene_Transform)
        if body.shape.kind==.Heightfield { found_field=true; testing.expect(t,transform.local.position==km.VEC3_ZERO && len(body.shape.heights)==4 && body.linear_velocity==km.Vec3{0.5,0,0}) }
        else if body.shape.kind==.Trimesh { found_mesh=true; testing.expect(t,transform.local.position==km.Vec3{0,3,0} && body.density==2 && body.linear_velocity==km.VEC3_ZERO) }
    }; delete(ids); testing.expect(t,found_field && found_mesh)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when BOX3D_LIBRARY!="" {
@(test)
test_native_moving_concave_scene_and_preview :: proc(t:^testing.T) { native_moving_concave_scene_and_preview(t) }
}
