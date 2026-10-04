#+test
package app

import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:testing"

@(private="file")
Gesture_Gate_Test :: struct { owner:^Authoring,gesture:^Scene_Gesture,reject:bool,calls:int }
@(private="file")
gesture_gate_finish :: proc(state:rawptr)->editor.Scene_Error {
    gate:=cast(^Gesture_Gate_Test)state; gate.calls+=1
    if gate.reject { return .Invalid_Operation }
    if gate.gesture.active { return scene_gesture_finish(gate.owner,gate.gesture) }
    return .None
}

@(test)
test_authoring_gate_keeps_observe_readonly_and_orders_drag_before_other_mutation :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    a:=ecs.spawn(&owner.world,struct {transform:Scene_Transform}{{km.TRANSFORM_IDENTITY}})
    b:=ecs.spawn(&owner.world,struct {transform:Scene_Transform}{{km.TRANSFORM_IDENTITY}})
    gesture:Scene_Gesture; defer scene_gesture_destroy(&gesture)
    testing.expect_value(t,scene_gesture_begin(&owner,&gesture,{a}),editor.Scene_Error.None)
    op:=editor.Scene_Op{kind=.Set_Field,entity=a,component="SceneTransform",field="local",value=transmute([]byte)string(`{"position":[2,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}`)}
    testing.expect_value(t,scene_gesture_preview(&owner,&gesture,op),editor.Scene_Error.None)
    gate:=Gesture_Gate_Test{&owner,&gesture,true,0}
    owner.before_mutation_state=&gate; owner.before_mutation=gesture_gate_finish
    result,group:=execute_owned(&owner,&owner.world,&owner.registry,{kind=.Query_Entities})
    testing.expect(t,result.error==.None && gate.calls==0 && gesture.active)
    editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    op.entity=b
    result,group=execute_owned(&owner,&owner.world,&owner.registry,op)
    testing.expect(t,result.error==.Invalid_Operation && group.state==nil && gate.calls==1 && gesture.active)
    testing.expect_value(t,ecs.get_component_mut(&owner.world,b,Scene_Transform).local.position,km.Vec3{})
    editor.tool_result_destroy(&result)
    gate.reject=false
    result,group=execute_owned(&owner,&owner.world,&owner.registry,op)
    testing.expect(t,result.error==.None && !gesture.active && gate.calls==2)
    editor.agent_record_action(&owner.agent.session,op,&result,&group)
    testing.expect_value(t,len(owner.agent.session.actions),2)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None)
    testing.expect(t,ecs.get_component_mut(&owner.world,a,Scene_Transform).local.position==km.Vec3{2,0,0} && ecs.get_component_mut(&owner.world,b,Scene_Transform).local.position==km.Vec3{})
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None)
    testing.expect_value(t,ecs.get_component_mut(&owner.world,a,Scene_Transform).local.position,km.Vec3{})
}
