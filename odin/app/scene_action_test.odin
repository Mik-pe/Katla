#+test
package app

import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:testing"
import resources "../resources"

@(private="file")
Action_Test_Native :: struct { reject:bool,commits:int }
@(private="file")
action_test_prepare :: proc(state:rawptr,owner:^Authoring,ids:[]ecs.Entity_Id,mode:Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
    native:=cast(^Action_Test_Native)state
    if native.reject { return nil,.Invalid_Operation }
    return native,.None
}
@(private="file")
action_test_finish :: proc(state,token:rawptr,commit:bool) { if commit { (cast(^Action_Test_Native)state).commits+=1 } }

@(test)
test_scene_action_batch_admission_reparent_and_history_preserve_live_generations :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    a:=ecs.spawn(&owner.world,struct {transform:Scene_Transform}{{km.TRANSFORM_IDENTITY}})
    b:=ecs.spawn(&owner.world,struct {transform:Scene_Transform}{{km.TRANSFORM_IDENTITY}})
    native:=Action_Test_Native{reject=true}; ecs.insert_resource(&owner.world,Scene_Participant{&native,action_test_prepare,action_test_finish})
    op:=editor.Scene_Op{kind=.Set_Field,component="SceneTransform",field="local",value=transmute([]byte)string(`{"position":[2,3,4],"rotation":[0,0,0,1],"scale":[1,1,1]}`)}
    result,group:=scene_action_execute(&owner,op,{a,b}); testing.expect_value(t,result.error,editor.Scene_Error.Invalid_Operation)
    testing.expect(t,group.state==nil && ecs.get_component_mut(&owner.world,a,Scene_Transform).local.position=={} && ecs.get_component_mut(&owner.world,b,Scene_Transform).local.position=={})
    editor.tool_result_destroy(&result); native.reject=false
    result,group=scene_action_execute(&owner,op,{a,b}); testing.expect_value(t,result.error,editor.Scene_Error.None)
    testing.expect_value(t,native.commits,1); editor.agent_record_action(&owner.agent.session,op,&result,&group)
    native.reject=true; testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.Invalid_Operation)
    testing.expect(t,ecs.entity_exists(&owner.world,a) && ecs.get_component_mut(&owner.world,a,Scene_Transform).local.position==km.Vec3{2,3,4})
    native.reject=false; testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None)
    testing.expect(t,ecs.entity_exists(&owner.world,a) && ecs.entity_exists(&owner.world,b) && ecs.get_component_mut(&owner.world,a,Scene_Transform).local.position=={})
    result,group=scene_action_execute(&owner,{kind=.Set_Parent,entity=a,parent=b,has_parent=true}); testing.expect_value(t,result.error,editor.Scene_Error.None); editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    result,group=scene_action_execute(&owner,{kind=.Set_Parent,entity=b,parent=a,has_parent=true}); testing.expect_value(t,result.error,editor.Scene_Error.Invalid_Operation); editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    _,parent_present:=ecs.get_component(&owner.world,b,Scene_Parent); testing.expect(t,!parent_present)
}

@(test)
test_scene_duplicate_subtree_keeps_internal_references_and_atomic_destroy_undo :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    result,group:=scene_action_execute(&owner,{kind=.Spawn,name="Root",shape="sphere",scale={1,1,1}})
    testing.expect_value(t,result.error,editor.Scene_Error.None); if result.error!=.None { editor.tool_result_destroy(&result); return }
    root:=result.entities[0]; editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    child:=ecs.spawn(&owner.world,struct {transform:Scene_Transform,parent:Scene_Parent}{{km.TRANSFORM_IDENTITY},{root}})
    result,group=scene_action_execute(&owner,{kind=.Duplicate,entity=root,position_offset={3,0,0},has_position_offset=true},{root,child})
    testing.expect_value(t,result.error,editor.Scene_Error.None); testing.expect_value(t,len(result.entities),2)
    if result.error!=.None { editor.tool_result_destroy(&result); return }
    copy_root,copy_child:=result.entities[0],result.entities[1]
    testing.expect_value(t,ecs.get_component_mut(&owner.world,copy_child,Scene_Parent).entity,copy_root)
    testing.expect_value(t,ecs.get_component_mut(&owner.world,copy_root,Scene_Transform).local.position,km.Vec3{3,0,0})
    testing.expect_value(t,ecs.get_component_mut(&owner.world,copy_child,Scene_Transform).local.position,km.Vec3{})
    editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    result,group=scene_action_execute(&owner,{kind=.Destroy,entity=copy_root})
    testing.expect(t,result.error==.None && len(result.entities)==2 && !ecs.entity_exists(&owner.world,copy_child)); editor.agent_record_action(&owner.agent.session,{kind=.Destroy,entity=copy_root},&result,&group)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None)
    testing.expect(t,!ecs.entity_exists(&owner.world,copy_root) && !ecs.entity_exists(&owner.world,copy_child) && owner.world.live_count==4)
    testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None); testing.expect_value(t,owner.world.live_count,2)
}

@(test)
test_scene_gesture_distinct_positions_single_history_cancel_and_native_rejection :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    a:=ecs.spawn(&owner.world,struct {transform:Scene_Transform}{{km.TRANSFORM_IDENTITY}})
    b:=ecs.spawn(&owner.world,struct {transform:Scene_Transform}{{km.Transform{position={5,0,0},rotation=km.QUAT_IDENTITY,scale={1,1,1}}}})
    native:=Action_Test_Native{}; ecs.insert_resource(&owner.world,Scene_Participant{&native,action_test_prepare,action_test_finish})
    gesture:Scene_Gesture; defer scene_gesture_destroy(&gesture)
    testing.expect_value(t,scene_gesture_begin(&owner,&gesture,{a,b}),editor.Scene_Error.None)
    operations:=[2]editor.Scene_Op{
        {kind=.Set_Field,entity=a,component="SceneTransform",field="local",value=transmute([]byte)string(`{"position":[1,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}`)},
        {kind=.Set_Field,entity=b,component="SceneTransform",field="local",value=transmute([]byte)string(`{"position":[6,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}`)},
    }
    testing.expect_value(t,scene_gesture_preview_values(&owner,&gesture,operations[:]),editor.Scene_Error.None)
    testing.expect_value(t,len(owner.agent.session.actions),0)
    native.reject=true; testing.expect_value(t,scene_gesture_cancel(&owner,&gesture),editor.Scene_Error.Invalid_Operation)
    testing.expect(t,gesture.active && ecs.get_component_mut(&owner.world,b,Scene_Transform).local.position==km.Vec3{6,0,0})
    native.reject=false; testing.expect_value(t,scene_gesture_finish(&owner,&gesture),editor.Scene_Error.None)
    testing.expect_value(t,len(owner.agent.session.actions),1)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None)
    testing.expect_value(t,ecs.get_component_mut(&owner.world,b,Scene_Transform).local.position,km.Vec3{5,0,0})
    testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None)
    testing.expect_value(t,ecs.get_component_mut(&owner.world,b,Scene_Transform).local.position,km.Vec3{6,0,0})
    testing.expect_value(t,scene_gesture_begin(&owner,&gesture,{a,b}),editor.Scene_Error.None)
    operations[0].value=transmute([]byte)string(`{"position":[8,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}`)
    testing.expect_value(t,scene_gesture_preview_values(&owner,&gesture,operations[:]),editor.Scene_Error.None)
    testing.expect_value(t,scene_gesture_cancel(&owner,&gesture),editor.Scene_Error.None)
    testing.expect_value(t,ecs.get_component_mut(&owner.world,a,Scene_Transform).local.position,km.Vec3{1,0,0})
}

@(test)
test_scene_gesture_readonly_reply_keeps_drag_active_and_undo_skips_observation :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    entity:=ecs.spawn(&owner.world,struct {transform:Scene_Transform}{{km.TRANSFORM_IDENTITY}})
    gesture:Scene_Gesture; defer scene_gesture_destroy(&gesture)
    testing.expect_value(t,scene_gesture_begin(&owner,&gesture,{entity}),editor.Scene_Error.None)
    op:=editor.Scene_Op{kind=.Set_Field,component="SceneTransform",field="local",value=transmute([]byte)string(`{"position":[1,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}`)}
    testing.expect_value(t,scene_gesture_preview(&owner,&gesture,op),editor.Scene_Error.None)
    previous_id:=owner.agent.session.next_id
    query:=editor.Scene_Op{kind=.Query_Entities}
    result,group:=scene_action_execute(&owner,query)
    testing.expect_value(t,result.error,editor.Scene_Error.None)
    editor.agent_record_action(&owner.agent.session,query,&result,&group)
    testing.expect(t,owner.agent.session.next_id>previous_id && !editor.agent_can_undo(&owner.agent.session))
    op.value=transmute([]byte)string(`{"position":[2,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}`)
    testing.expect_value(t,scene_gesture_preview(&owner,&gesture,op),editor.Scene_Error.None)
    testing.expect_value(t,scene_gesture_finish(&owner,&gesture),editor.Scene_Error.None)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None)
    testing.expect_value(t,ecs.get_component_mut(&owner.world,entity,Scene_Transform).local.position,km.Vec3{})
    testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None)
    testing.expect_value(t,ecs.get_component_mut(&owner.world,entity,Scene_Transform).local.position,km.Vec3{2,0,0})
}

@(test)
test_scene_drop_batch_actual_prefab_model_native_admission_and_missing_asset_rollback :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    testing.expect_value(t,asset_resources_init(&owner,".","resources"),resources.Error.None)
    native:=Action_Test_Native{reject=true}; ecs.insert_resource(&owner.world,Scene_Participant{&native,action_test_prepare,action_test_finish})
    operations:=[2]editor.Scene_Op{
        {kind=.Application,tool_name="prefab",value=transmute([]byte)string(`{"action":"instantiate","path":"resources/prefabs/chair.katprefab","position":[2,0,0]}`)},
        {kind=.Spawn_Model,path="models/Box.glb",scale={1,1,1}},
    }
    key:=ecs.get_resource_mut(&owner.world,Scene_Identity).next_entity_id
    result,group:=scene_action_execute_batch(&owner,operations[:])
    testing.expect(t,result.error==.Invalid_Operation && group.state==nil && owner.world.live_count==0 && ecs.get_resource_mut(&owner.world,Scene_Identity).next_entity_id==key); editor.tool_result_destroy(&result)
    native.reject=false
    operations[1].path="models/missing.glb"
    result,group=scene_action_execute_batch(&owner,operations[:]); testing.expect(t,result.error!=.None && group.state==nil && owner.world.live_count==0); editor.tool_result_destroy(&result)
    operations[1].path="models/Box.glb"
    result,group=scene_action_execute_batch(&owner,operations[:]); testing.expect_value(t,result.error,editor.Scene_Error.None)
    testing.expect(t,native.commits==1 && len(result.entities)==5 && owner.world.live_count==5)
    editor.agent_record_action(&owner.agent.session,{kind=.Application,tool_name="asset_drop"},&result,&group)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None); testing.expect_value(t,owner.world.live_count,0)
    testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None); testing.expect_value(t,owner.world.live_count,5)
}
