#+test
//! Collider removal and exact restoration include owned height streams, trigger state and external constraints.
package app
import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:testing"
import "core:mem"

@(private="file")
Collider_Test_Participant :: struct { reject:bool }
@(private="file")
collider_test_prepare :: proc(state:rawptr,owner:^Authoring,ids:[]ecs.Entity_Id,mode:Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) { return nil,.Invalid_Operation if (cast(^Collider_Test_Participant)state).reject else .None }
@(private="file")
collider_test_finish :: proc(state,token:rawptr,committed:bool) {}
@(private="file")
collider_history_acceptance :: proc(t:^testing.T,native:bool) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    if native { testing.expect_value(t,physics_select_box3d(&owner,BOX3D_LIBRARY),editor.Scene_Error.None) }
    a:=ecs.create_entity(&owner.world); b:=ecs.create_entity(&owner.world); constraint:=ecs.create_entity(&owner.world)
    for id in ([3]ecs.Entity_Id{a,b,constraint}) { ecs.add_component(&owner.world,id,Scene_Transform{km.TRANSFORM_IDENTITY}) }
    heights:=[4]f32{.1,.2,.3,.4}; shape,valid:=physics_heightfield(2,2,heights[:]); testing.expect(t,valid)
    body:=physics_body(shape,.Kinematic,true); body.has_material=true; body.has_filter=true; body.friction=.9; body.restitution=.4; body.density=2.5; body.layers=4; body.mask=8; body.linear_velocity={1,2,3}; body.gravity_scale=.25
    ecs.add_component(&owner.world,a,body); ecs.add_component(&owner.world,b,physics_body({kind=.Sphere,radius=.5},.Dynamic))
    ecs.add_component(&owner.world,constraint,Physics_Joint{kind=.PointToPoint,a=a,b=b,anchor_a={.1,.2,.3},anchor_b={.3,.4,.5}})
    volume:=Trigger_Volume{overlapping=make([dynamic]ecs.Entity_Id,owner.world.allocator)}; append(&volume.overlapping,b); ecs.add_component(&owner.world,a,volume); ecs.add_component(&owner.world,a,Trigger_Rules{})
    if native { testing.expect_value(t,physics_prepare(&owner),editor.Scene_Error.None) }
    participant:=Collider_Test_Participant{reject=true}; ecs.insert_resource(&owner.world,Scene_Participant{&participant,collider_test_prepare,collider_test_finish})
    op:=editor.Scene_Op{kind=.Set_Field,entity=a,component="PhysicsBody",field="has_collider",value=transmute([]byte)string("false")}
    result,group:=scene_action_execute(&owner,op); testing.expect(t,result.error==.Invalid_Operation && group.state==nil); editor.tool_result_destroy(&result)
    before:=ecs.get_component_mut(&owner.world,a,Physics_Body); testing.expect(t,before.has_collider && before.shape.kind==.Heightfield && before.shape.heights[3]==.4 && ecs.get_component_mut(&owner.world,constraint,Physics_Joint)!=nil)
    participant.reject=false; result,group=scene_action_execute(&owner,op); testing.expect_value(t,result.error,editor.Scene_Error.None)
    after:=ecs.get_component_mut(&owner.world,a,Physics_Body); testing.expect(t,!after.has_collider && after.has_rigid_body && !after.has_material && !after.has_filter && !after.sensor && after.shape.kind==.None && len(after.shape.heights)==0 && after.linear_velocity==body.linear_velocity && after.gravity_scale==body.gravity_scale)
    testing.expect(t,ecs.get_component_mut(&owner.world,a,Trigger_Volume)==nil && ecs.get_component_mut(&owner.world,a,Trigger_Rules)==nil && ecs.get_component_mut(&owner.world,constraint,Physics_Joint)==nil)
    if native { testing.expect_value(t,physics_prepare(&owner),editor.Scene_Error.None) }
    testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.None)
    restored:=ecs.get_component_mut(&owner.world,a,Physics_Body); testing.expect(t,restored.has_collider && restored.has_filter && restored.has_material && restored.sensor && restored.shape.kind==.Heightfield && restored.shape.rows==2 && restored.shape.cols==2 && restored.shape.heights[0]==.1 && restored.shape.heights[3]==.4 && restored.friction==.9 && restored.restitution==.4 && restored.density==2.5 && restored.layers==4 && restored.mask==8)
    testing.expect(t,ecs.get_component_mut(&owner.world,a,Trigger_Volume).overlapping[0]==b && ecs.get_component_mut(&owner.world,a,Trigger_Rules)!=nil && ecs.get_component_mut(&owner.world,constraint,Physics_Joint).a==a)
    if native { testing.expect_value(t,physics_prepare(&owner),editor.Scene_Error.None) }
    testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.None)
    if native { testing.expect_value(t,physics_prepare(&owner),editor.Scene_Error.None) }
    editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    detached:=ecs.get_component_mut(&owner.world,a,Physics_Body); detached.has_material=true; detached.friction=.7; detached.has_filter=true; detached.layers=16
    op.value=transmute([]byte)string("true")
    result,group=scene_action_execute(&owner,op); testing.expect_value(t,result.error,editor.Scene_Error.None)
    attached:=ecs.get_component_mut(&owner.world,a,Physics_Body); testing.expect(t,attached.has_collider && attached.shape.kind==.Box && attached.shape.half_extents==[3]f32{.5,.5,.5} && attached.has_material && attached.friction==.7 && attached.has_filter && attached.layers==16)
    testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.None)
    detached=ecs.get_component_mut(&owner.world,a,Physics_Body); testing.expect(t,!detached.has_collider && detached.shape.kind==.None && detached.has_material && detached.friction==.7 && detached.has_filter && detached.layers==16)
    editor.tool_result_destroy(&result); editor.undo_group_destroy(&group); authoring_destroy(&owner)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_collider_removal_owned_heightfield_trigger_constraint_undo_and_native_rejection :: proc(t:^testing.T) { collider_history_acceptance(t,false) }
when BOX3D_LIBRARY!="" {
@(test)
test_collider_removal_box3d_heightfield_constraint_lifetime_and_exact_undo :: proc(t:^testing.T) { collider_history_acceptance(t,true) }
}
