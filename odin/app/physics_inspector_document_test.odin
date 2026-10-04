#+test
#+build darwin, linux
//! Optional physics metadata survives durable scene publication and native preview restoration.
package app
import ecs "../ecs"
import editor "../editor"
import km "../math"
import resources "../resources"
import "core:testing"
import "core:mem"
import "core:os"
import "core:strings"

@(private="file")
physics_inspector_file :: proc(owner:^Authoring,load:bool)->editor.Scene_Error {
    result,group:=scene_file_execute(owner,{action=.Load if load else .Save,path="physics.katla",has_path=true})
    defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&group); return result.error
}
@(private="file")
physics_inspector_find :: proc(owner:^Authoring,name:string)->ecs.Entity_Id {
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    for id in ids { if label,present:=ecs.get_component(&owner.world,id,Scene_Name); present && label.name==name { return id } }
    return 0
}
@(private="file")
physics_inspector_presence_document :: proc(t:^testing.T,native:bool) {
    directory,error:=os.make_directory_temp("","katla-physics-metadata-*",context.allocator)
    testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_root:=strings.concatenate({directory,"/resources"}); defer delete(resource_root); testing.expect_value(t,os.make_directory(resource_root),os.Error(nil))
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    testing.expect_value(t,asset_resources_init(&owner,directory,resource_root),resources.Error.None)
    if native { testing.expect_value(t,physics_select_box3d(&owner,BOX3D_LIBRARY),editor.Scene_Error.None) }
    body_entity:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,body_entity,Scene_Name{strings.clone("Body")}); ecs.add_component(&owner.world,body_entity,Scene_Transform{km.TRANSFORM_IDENTITY})
    result,group:=scene_action_execute(&owner,{kind=.Add_Component,entity=body_entity,component="PhysicsBody"}); testing.expect_value(t,result.error,editor.Scene_Error.None); editor.agent_record_action(&owner.agent.session,{kind=.Add_Component,entity=body_entity,component="PhysicsBody"},&result,&group); editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    toggle:=editor.Scene_Op{kind=.Set_Field,entity=body_entity,component="PhysicsBody",field="has_collider",value=transmute([]byte)string("false")}
    result,group=scene_action_execute(&owner,toggle); testing.expect_value(t,result.error,editor.Scene_Error.None); editor.agent_record_action(&owner.agent.session,toggle,&result,&group); editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    metadata:=physics_body(Physics_Shape{kind=.None},.Fixed); metadata.has_rigid_body=false; metadata.has_material=true; metadata.has_filter=true; metadata.friction=.8; metadata.restitution=.3; metadata.density=2; metadata.layers=4; metadata.mask=8
    metadata_entity:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,metadata_entity,Scene_Name{strings.clone("Metadata")}); ecs.add_component(&owner.world,metadata_entity,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&owner.world,metadata_entity,metadata)
    testing.expect_value(t,physics_inspector_file(&owner,false),editor.Scene_Error.None)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None)
    restored,_:=ecs.get_component(&owner.world,body_entity,Physics_Body); testing.expect(t,restored.has_collider && restored.has_material && restored.has_filter && restored.shape.kind==.Box)
    testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None)
    disabled,_:=ecs.get_component(&owner.world,body_entity,Physics_Body); testing.expect(t,!disabled.has_collider && disabled.has_rigid_body && disabled.has_material && disabled.has_filter)
    roots:=ecs.get_resource_mut(&owner.world,Asset_Roots); bytes,read_error:=resources.read_text(&roots.project,"physics.katla"); testing.expect(t,read_error==.None && strings.contains(string(bytes),"physics_material:") && strings.contains(string(bytes),"collision_filter:") && !strings.contains(string(bytes),"collider_shape:")); delete(bytes)
    testing.expect_value(t,physics_inspector_file(&owner,true),editor.Scene_Error.None)
    testing.expect(t,!ecs.entity_exists(&owner.world,body_entity) && !ecs.entity_exists(&owner.world,metadata_entity))
    loaded_body:=physics_inspector_find(&owner,"Body"); loaded_metadata:=physics_inspector_find(&owner,"Metadata")
    body,present:=ecs.get_component(&owner.world,loaded_body,Physics_Body); testing.expect(t,present && body.has_rigid_body && !body.has_collider && body.has_material && body.has_filter && body.shape.kind==.None && body.friction==.5 && body.layers==max(u32))
    owned,has_owned:=ecs.get_component(&owner.world,loaded_metadata,Physics_Body); testing.expect(t,has_owned && !owned.has_rigid_body && !owned.has_collider && owned.has_material && owned.has_filter && owned.friction==.8 && owned.restitution==.3 && owned.density==2 && owned.layers==4 && owned.mask==8)
    if native {
        played,play_group:=simulation_execute(&owner,.Play); testing.expect_value(t,played.error,editor.Scene_Error.None); editor.tool_result_destroy(&played); editor.undo_group_destroy(&play_group)
        testing.expect_value(t,simulation_step(&owner,.1),editor.Scene_Error.None)
        stopped,stop_group:=simulation_execute(&owner,.Stop); testing.expect_value(t,stopped.error,editor.Scene_Error.None); editor.tool_result_destroy(&stopped); editor.undo_group_destroy(&stop_group)
        testing.expect(t,!ecs.entity_exists(&owner.world,loaded_body) && !ecs.entity_exists(&owner.world,loaded_metadata))
        current:=physics_inspector_find(&owner,"Metadata"); after,_:=ecs.get_component(&owner.world,current,Physics_Body); testing.expect(t,!after.has_rigid_body && !after.has_collider && after.has_material && after.has_filter && after.friction==owned.friction && after.restitution==owned.restitution && after.density==owned.density && after.layers==owned.layers && after.mask==owned.mask)
        body_after,_:=ecs.get_component(&owner.world,physics_inspector_find(&owner,"Body"),Physics_Body); testing.expect(t,body_after.has_rigid_body && !body_after.has_collider && body_after.has_material && body_after.has_filter && body_after.body_type==body.body_type && body_after.gravity_scale==body.gravity_scale && body_after.shape.kind==.None && body_after.friction==body.friction && body_after.mask==body.mask)
    }
    current_metadata:=physics_inspector_find(&owner,"Metadata"); ecs.get_component_mut(&owner.world,current_metadata,Physics_Body).sensor=true
    testing.expect_value(t,physics_inspector_file(&owner,false),editor.Scene_Error.Invalid_Field_Value)
}
@(private="file")
physics_inspector_presence_tracked :: proc(t:^testing.T,native:bool) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    physics_inspector_presence_document(t,native); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_physics_inspector_optional_metadata_save_load_and_collider_toggle_undo :: proc(t:^testing.T) { physics_inspector_presence_tracked(t,false) }
when BOX3D_LIBRARY!="" {
@(test)
test_physics_inspector_optional_metadata_native_play_stop :: proc(t:^testing.T) { physics_inspector_presence_tracked(t,true) }
}
