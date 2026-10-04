#+test
//! Source controller collision uses the real imported triangles and native Box3D contacts.
package app
import ecs "../ecs"
import editor "../editor"
import resources "../resources"
import km "../math"
import "core:testing"
import "core:mem"

@(private="file")
gltf_collision_acceptance :: proc(t:^testing.T,native:bool) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None); testing.expect_value(t,asset_resources_init(&owner,".","resources"),resources.Error.None)
    if native { testing.expect_value(t,physics_select_box3d(&owner,BOX3D_LIBRARY),editor.Scene_Error.None) }
    result,group:=scene_action_execute(&owner,{kind=.Spawn_Model,path="models/Box.glb",scale={1,1,1}}); testing.expect_value(t,result.error,editor.Scene_Error.None)
    if result.error==.None && len(result.entities)==2 {
        root,child:=result.entities[0],result.entities[1]
        ecs.add_component(&owner.world,root,physics_body({kind=.Trimesh},.Fixed))
        collected,error:=physics_collect(&owner); testing.expect_value(t,error,editor.Scene_Error.None); testing.expect_value(t,len(collected),1)
        if len(collected)==1 { testing.expect(t,len(collected[0].vertices)==24 && len(collected[0].indices)==36) }; physics_collected_destroy(&collected,owner.world.allocator)
        geometry,geometry_error:=scene_model_collision_geometry(ecs.get_component_mut(&owner.world,child,Scene_Model)); testing.expect_value(t,geometry_error,editor.Scene_Error.None); testing.expect(t,len(geometry.vertices)==24 && len(geometry.indices)==36); mesh_geometry_destroy(&geometry)
        if native {
            ball:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,ball,Scene_Transform{km.transform(position={0,2,0})}); ecs.add_component(&owner.world,ball,physics_body({kind=.Sphere,radius=.1}))
            played,play_history:=simulation_execute(&owner,.Play); testing.expect_value(t,played.error,editor.Scene_Error.None); editor.tool_result_destroy(&played); editor.undo_group_destroy(&play_history)
            for _ in 0..<120 { testing.expect_value(t,simulation_step(&owner,1.0/60),editor.Scene_Error.None) }
            testing.expect(t,abs(ecs.get_component_mut(&owner.world,ball,Scene_Transform).local.position[1]-.6)<.03)
            stopped,stop_history:=simulation_execute(&owner,.Stop); testing.expect_value(t,stopped.error,editor.Scene_Error.None); editor.tool_result_destroy(&stopped); editor.undo_group_destroy(&stop_history)
            testing.expect(t,!ecs.entity_exists(&owner.world,root) && !ecs.entity_exists(&owner.world,child) && !ecs.entity_exists(&owner.world,ball) && owner.world.live_count==3)
        }
    } else { testing.expect_value(t,len(result.entities),2) }
    editor.tool_result_destroy(&result); editor.undo_group_destroy(&group); authoring_destroy(&owner)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_gltf_controller_collision_collects_actual_bind_pose_triangle_geometry :: proc(t:^testing.T) { gltf_collision_acceptance(t,false) }
when BOX3D_LIBRARY!="" {
@(test)
test_gltf_controller_native_box3d_trimesh_contact_and_play_stop_shared_source :: proc(t:^testing.T) { gltf_collision_acceptance(t,true) }
}
