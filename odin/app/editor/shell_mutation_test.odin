package editor_app

import app ".."
import ecs "../../ecs"
import editor "../../editor"
import km "../../math"
import "core:testing"

@(test)
test_agent_mutation_finishes_preview_before_second_command_and_readonly_preserves_capture :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner)
    entity:=ecs.create_entity(&owner.world)
    ecs.add_component(&owner.world,entity,app.Scene_Transform{local={rotation=km.QUAT_IDENTITY,scale={1,1,1}}})
    state:State; state_init(&state,&owner); defer state_destroy(&state)
    shell:=Shell{state=&state}
    owner.before_mutation_state=&shell; owner.before_mutation=shell_before_mutation
    defer { owner.before_mutation=nil; app.scene_gesture_destroy(&shell.field_gesture) }
    testing.expect_value(t,app.scene_gesture_begin(&owner,&shell.field_gesture,{entity}),editor.Scene_Error.None)
    op:=editor.Scene_Op{kind=.Set_Field,entity=entity,component="SceneTransform",field="local",value=transmute([]byte)string(`{"position":[3,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}`)}
    testing.expect_value(t,app.scene_gesture_preview(&owner,&shell.field_gesture,op),editor.Scene_Error.None)
    result,undo:=app.scene_action_execute(&owner,{kind=.Query_Entities})
    testing.expect(t,result.error==.None && shell.field_gesture.active && !editor.agent_can_undo(&owner.agent.session)); editor.tool_result_destroy(&result); editor.undo_group_destroy(&undo)
    op.value=transmute([]byte)string(`{"position":[7,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}`)
    result,undo=app.scene_action_execute(&owner,op)
    testing.expect(t,result.error==.None && !shell.field_gesture.active)
    editor.agent_record_action(&owner.agent.session,op,&result,&undo)
    testing.expect_value(t,app.authoring_undo_last(&owner),editor.Scene_Error.None)
    transform,_:=ecs.get_component(&owner.world,entity,app.Scene_Transform); testing.expect_value(t,transform.local.position[0],f32(3))
    testing.expect_value(t,app.authoring_undo_last(&owner),editor.Scene_Error.None)
    transform,_=ecs.get_component(&owner.world,entity,app.Scene_Transform); testing.expect_value(t,transform.local.position[0],f32(0))
    testing.expect_value(t,app.authoring_redo_last(&owner),editor.Scene_Error.None)
    testing.expect_value(t,app.authoring_redo_last(&owner),editor.Scene_Error.None)
    transform,_=ecs.get_component(&owner.world,entity,app.Scene_Transform); testing.expect_value(t,transform.local.position[0],f32(7))
}
