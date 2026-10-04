#+test
package app
import "core:testing"
import "core:mem"
import "core:strings"
import asset "../agent/assets"
import ecs "../ecs"
import editor "../editor"
import km "../math"

@(private="file")
native_chair_mesh_acceptance :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner)
    testing.expect(t,asset_resources_init(&owner,".","resources")==.None)
    testing.expect(t,physics_select_box3d(&owner,BOX3D_LIBRARY)==.None)
    result,undo:=asset_authoring_execute(&owner,asset.Prefab_Request{action=.Instantiate,path="resources/prefabs/chair.katprefab",name="Native chair",position={4,0,2},rotation={0,0,0,1},scale={2,1,1}})
    testing.expect(t,result.error==.None && len(result.entities)==3); if result.error!=.None || len(result.entities)!=3 { editor.tool_result_destroy(&result); editor.undo_group_destroy(&undo); authoring_destroy(&owner); return }
    chair:=result.entities[0]; editor.tool_result_destroy(&result); editor.undo_group_destroy(&undo)
    frame:ecs.Entity_Id; ids:=ecs.entity_ids(&owner.world)
    for id in ids { if name,present:=ecs.get_component(&owner.world,id,Scene_Name); present && name.name=="Frame" { frame=id } }; delete(ids)
    mesh,present:=ecs.get_component(&owner.world,frame,Scene_Mesh); testing.expect(t,present && len(mesh.geometry.vertices)==144 && len(mesh.geometry.indices)==216)
    body:=physics_body({kind=.Sphere,radius=0.1}); body.ccd=true
    ball:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,ball,Scene_Name{strings.clone("Falling sphere")}); ecs.add_component(&owner.world,ball,Scene_Transform{km.transform(position={4,2,2})}); ecs.add_component(&owner.world,ball,body)
    traveler:=ecs.create_entity(&owner.world); across:=physics_body({kind=.Sphere,radius=0.04}); across.gravity_scale=0; across.linear_velocity={-1,0,0}; across.ccd=true
    ecs.add_component(&owner.world,traveler,Scene_Name{strings.clone("Under seat")}); ecs.add_component(&owner.world,traveler,Scene_Transform{km.transform(position={5.1,0.2,2})}); ecs.add_component(&owner.world,traveler,across)
    testing.expect(t,execute_test_simulation(t,&owner,.Play)==.None)
    for _ in 0..<120 { testing.expect(t,simulation_step(&owner,1.0/60)==.None) }
    fallen:=ecs.get_component_mut(&owner.world,ball,Scene_Transform); crossed:=ecs.get_component_mut(&owner.world,traveler,Scene_Transform)
    testing.expect(t,abs(fallen.local.position[1]-0.6)<0.025)
    testing.expect(t,crossed.local.position[0]<3.15 && abs(crossed.local.position[1]-0.2)<0.001 && abs(crossed.local.position[2]-2)<0.001)
    testing.expect(t,execute_test_simulation(t,&owner,.Stop)==.None && !ecs.entity_exists(&owner.world,chair) && !ecs.entity_exists(&owner.world,frame) && !ecs.entity_exists(&owner.world,ball))
    ids=ecs.entity_ids(&owner.world); found_frame,found_ball:=false,false
    for id in ids {
        if name,has_name:=ecs.get_component(&owner.world,id,Scene_Name); has_name {
            if name.name=="Frame" { parent,has_parent:=ecs.get_component(&owner.world,id,Scene_Parent); restored,has_mesh:=ecs.get_component(&owner.world,id,Scene_Mesh); found_frame=has_parent && ecs.entity_exists(&owner.world,parent.entity) && has_mesh && len(restored.geometry.vertices)==144 }
            else if name.name=="Falling sphere" { transform,has_transform:=ecs.get_component(&owner.world,id,Scene_Transform); found_ball=has_transform && transform.local.position==km.Vec3{4,2,2} }
        }
    }; delete(ids); testing.expect(t,found_frame && found_ball)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when BOX3D_LIBRARY!="" {
@(test)
test_native_chair_mesh_box3d_contact_hole_and_play_stop_restore :: proc(t:^testing.T) { native_chair_mesh_acceptance(t) }
}
