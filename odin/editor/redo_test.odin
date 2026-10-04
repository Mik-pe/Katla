#+test
package editor

import ecs "../ecs"
import "core:testing"
import "core:strings"

@(test)
test_session_redo_retargets_fresh_generations_and_invalidates_branch :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"Position",Editor_Test_Component{1,100,false})
    s:Agent_Session; agent_session_init(&s); defer agent_session_destroy(&s)
    a:=agent_execute(&s,&w,&reg,{kind=.Spawn,name="first"}); id:=a.result.entities[0]
    agent_execute(&s,&w,&reg,{kind=.Set_Field,entity=id,component="Position",field="health",value=transmute([]byte)string("42")})
    agent_execute(&s,&w,&reg,{kind=.Destroy,entity=id})
    for _ in 0..<3 { testing.expect_value(t,agent_undo_last(&s,&w,&reg),Scene_Error.None) }
    testing.expect(t,w.live_count==0 && len(s.actions)==0 && len(s.redo_actions)==3 && s.next_id==3)
    testing.expect_value(t,agent_redo_last(&s,&w,&reg),Scene_Error.None)
    fresh:=s.actions[0].result.entities[0]
    testing.expect(t,fresh!=id && ecs.entity_exists(&w,fresh) && s.actions[0].id==0)
    testing.expect_value(t,agent_redo_last(&s,&w,&reg),Scene_Error.None)
    value,_:=ecs.get_component(&w,fresh,Editor_Test_Component)
    testing.expect(t,value.health==42 && s.actions[1].operation.entity==fresh && s.actions[1].id==1)
    testing.expect_value(t,agent_redo_last(&s,&w,&reg),Scene_Error.None)
    testing.expect(t,w.live_count==0 && len(s.redo_actions)==0 && s.actions[2].id==2 && s.next_id==3)
    testing.expect_value(t,agent_undo_last(&s,&w,&reg),Scene_Error.None)
    fresh=s.actions[0].result.entities[0]
    testing.expect(t,ecs.entity_exists(&w,fresh))
    agent_execute(&s,&w,&reg,{kind=.Set_Field,entity=fresh,component="Position",field="health",value=transmute([]byte)string("73")})
    testing.expect(t,len(s.redo_actions)==0 && s.next_id==4 && s.actions[2].id==3)
    testing.expect_value(t,agent_redo_last(&s,&w,&reg),Scene_Error.None)
    value,_=ecs.get_component(&w,fresh,Editor_Test_Component)
    testing.expect_value(t,value.health,i32(73))
}

@(test)
test_session_redo_decode_failure_preserves_both_histories :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"Position",Editor_Test_Component{1,100,false})
    s:Agent_Session; agent_session_init(&s); defer agent_session_destroy(&s)
    id:=ecs.create_entity(&w); editor_add_default(&w,id,reg.entries["Position"])
    a:=agent_execute(&s,&w,&reg,{kind=.Set_Field,entity=id,component="Position",field="health",value=transmute([]byte)string("42")})
    command:=cast(^Entity_Command)a.undo.state
    testing.expect_value(t,agent_undo_last(&s,&w,&reg),Scene_Error.None)
    delete(command.after[0].data,command.allocator)
    command.after[0].data=transmute([]byte)strings.clone(`{"health":"invalid"}`,command.allocator)
    testing.expect_value(t,agent_redo_last(&s,&w,&reg),Scene_Error.Decode_Failed)
    value,_:=ecs.get_component(&w,id,Editor_Test_Component)
    testing.expect(t,value.health==100 && len(s.actions)==0 && len(s.redo_actions)==1 && s.redo_actions[0].id==0 && s.next_id==1)
}
