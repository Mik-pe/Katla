#+test
package editor

import ecs "../ecs"
import "core:testing"
import "core:strings"
import "core:mem"
import "core:thread"




Editor_Test_Component :: struct { x:f32 `min:"-10" max:"10" speed:"0.5" display_name:"X coordinate"`, health:i32, hidden:bool `inspect:"skip"` }
@(test)
test_editor_metadata_and_field_json :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"Position",Editor_Test_Component{1,100,false})
    fields:=editor_fields(&reg,"Position")
    testing.expect(t,len(fields)==3 && fields[0].constraints.has_min && fields[0].constraints.min == -10 && fields[2].constraints.skip)
    id:=ecs.create_entity(&w); editor_add_default(&w,id,reg.entries["Position"])
    testing.expect_value(t,editor_set_field(&w,&reg,id,"Position","x",transmute([]byte)string("4.5")),Scene_Error.None)
    value,ok:=ecs.get_component(&w,id,Editor_Test_Component); testing.expect(t,ok && value.x==4.5 && value.health==100)
    testing.expect_value(t,editor_set_field(&w,&reg,id,"Position","hidden",transmute([]byte)string("true")),Scene_Error.Field_Not_Found)
    testing.expect_value(t,editor_set_field(&w,&reg,id,"Position","x",transmute([]byte)string("\"wrong\"")),Scene_Error.Invalid_Field_Value)
}
@(test)
test_scene_set_field_undo_redo :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"Position",Editor_Test_Component{1,100,false})
    id:=ecs.create_entity(&w); editor_add_default(&w,id,reg.entries["Position"])
    result,group:=scene_execute(&w,&reg,Scene_Op{kind=.Set_Field,entity=id,component="Position",field="health",value=transmute([]byte)string("42")})
    defer tool_result_destroy(&result); defer undo_group_destroy(&group)
    testing.expect_value(t,result.error,Scene_Error.None)
    testing.expect_value(t,undo_group(&w,&reg,&group),Scene_Error.None)
    a,_:=ecs.get_component(&w,id,Editor_Test_Component); testing.expect_value(t,a.health,i32(100))
    testing.expect_value(t,redo_group(&w,&reg,&group),Scene_Error.None)
    b,_:=ecs.get_component(&w,id,Editor_Test_Component); testing.expect_value(t,b.health,i32(42))
}
@(test)
test_scene_destroy_undo_uses_fresh_generation :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"Position",Editor_Test_Component{1,100,false})
    id:=ecs.create_entity(&w); editor_add_default(&w,id,reg.entries["Position"])
    result,group:=scene_execute(&w,&reg,Scene_Op{kind=.Destroy,entity=id})
    defer tool_result_destroy(&result); defer undo_group_destroy(&group)
    testing.expect_value(t,undo_group(&w,&reg,&group),Scene_Error.None)
    testing.expect(t,id!=group.entity && !ecs.entity_exists(&w,id) && ecs.entity_exists(&w,group.entity))
    value,ok:=ecs.get_component(&w,group.entity,Editor_Test_Component); testing.expect(t,ok && value.health==100)
    testing.expect_value(t,redo_group(&w,&reg,&group),Scene_Error.None)
    testing.expect(t,!ecs.entity_exists(&w,group.entity))
}
@(test)
test_scene_spawn_duplicate_add_remove_query :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"Position",Editor_Test_Component{1,100,false})
    a,spawn_group:=scene_execute(&w,&reg,Scene_Op{kind=.Spawn,position={7,0,0}})
    defer tool_result_destroy(&a); defer undo_group_destroy(&spawn_group)
    id:=a.entities[0]
    value,_:=ecs.get_component(&w,id,Editor_Test_Component); testing.expect_value(t,value.x,f32(7))
    b,dup_group:=scene_execute(&w,&reg,Scene_Op{kind=.Duplicate,entity=id})
    defer tool_result_destroy(&b); defer undo_group_destroy(&dup_group)
    testing.expect(t,b.error==.None && b.entities[0]!=id)
    c,remove_group:=scene_execute(&w,&reg,Scene_Op{kind=.Remove_Component,entity=id,component="Position"})
    defer tool_result_destroy(&c); defer undo_group_destroy(&remove_group)
    q,empty:=scene_execute(&w,&reg,Scene_Op{kind=.Query_Entities,component="Position"})
    defer tool_result_destroy(&q); defer undo_group_destroy(&empty)
    testing.expect_value(t,len(q.entities),1)
    testing.expect_value(t,undo_group(&w,&reg,&remove_group),Scene_Error.None)
    testing.expect(t,ecs.validate(&w))
}
Owned_Name :: struct { name:string }
test_name_destroy :: proc(p:rawptr) { value:=cast(^Owned_Name)p; delete(value.name); value.name="" }
test_name_clone :: proc(dst,src:rawptr) { (^Owned_Name)(dst)^=Owned_Name{strings.clone((^Owned_Name)(src).name)} }
@(test)
test_editor_owned_strings_and_failed_decode_release :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    context.allocator=mem.tracking_allocator(&tracker)
    w:ecs.World; ecs.world_init(&w)
    reg:Component_Registry; editor_registry_init(&reg)
    editor_register(&w,&reg,"Name",Owned_Name{strings.clone("first")},ecs.Value_Ops{test_name_destroy,test_name_clone})
    id:=ecs.create_entity(&w); editor_add_default(&w,id,reg.entries["Name"])
    testing.expect_value(t,editor_set_field(&w,&reg,id,"Name","name",transmute([]byte)string("\"second\"")),Scene_Error.None)
    value,ok:=ecs.get_component(&w,id,Owned_Name); testing.expect(t,ok && value.name=="second")
    editor_registry_destroy(&reg); ecs.world_destroy(&w)
    testing.expect_value(t,len(tracker.allocation_map),0)
}
@(private="file")
test_agent_producer :: proc(t:^thread.Thread) {
    h:=cast(^Agent_Harness)t.data
    for _ in 0..<12 { agent_submit(h,Scene_Op{kind=.Spawn,position={3,0,0}}) }
    agent_finish(h)
}
@(test)
test_agent_background_mailbox_and_undo :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"Position",Editor_Test_Component{})
    h:Agent_Harness; agent_harness_init(&h); defer agent_harness_destroy(&h)
    producer:=thread.create(test_agent_producer); producer.data=&h
    thread.start(producer); thread.join(producer); thread.destroy(producer)
    h.session.paused=true; testing.expect_value(t,agent_tick(&h,&w,&reg),0)
    h.session.paused=false
    testing.expect_value(t,agent_tick(&h,&w,&reg),10)
    testing.expect_value(t,agent_tick(&h,&w,&reg),2)
    testing.expect(t,h.session.finished && w.live_count==12)
    for _ in 0..<12 { response,ok:=agent_take_result(&h); testing.expect(t,ok && response.result.error==.None); tool_result_destroy(&response.result) }
    testing.expect_value(t,agent_undo_all(&h.session,&w,&reg),Scene_Error.None)
    testing.expect(t,w.live_count==0 && len(h.session.actions)==0)
}

@(test)
test_agent_read_and_failed_actions_have_no_undo_mutation :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"Position",Editor_Test_Component{})
    id:=ecs.create_entity(&w); editor_add_default(&w,id,reg.entries["Position"])
    session:Agent_Session; agent_session_init(&session); defer agent_session_destroy(&session)
    agent_execute(&session,&w,&reg,Scene_Op{kind=.Query_Entities})
    agent_execute(&session,&w,&reg,Scene_Op{kind=.Set_Field,entity=id,component="missing",field="x",value=transmute([]byte)string("3")})
    testing.expect_value(t,agent_undo_all(&session,&w,&reg),Scene_Error.None)
    testing.expect(t,ecs.entity_exists(&w,id) && w.live_count==1 && ecs.validate(&w))
}
Sync_Agent_State :: struct { steps,results:int }
@(test)
test_agent_sync_observation_and_results :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    session:Agent_Session; agent_session_init(&session); defer agent_session_destroy(&session)
    state:Sync_Agent_State
    decide:=proc(s:^Sync_Agent_State,o:Observation)->(Scene_Op,bool) {
        assert(o.entity_count==s.steps)
        if s.steps==2 { return {},false }
        s.steps+=1; return Scene_Op{kind=.Spawn},true
    }
    on_result:=proc(s:^Sync_Agent_State,a:^Agent_Action) { assert(a.result.error==.None); s.results+=1 }
    agent_run_sync(&session,&w,&reg,&state,decide,on_result)
    testing.expect(t,state.steps==2 && state.results==2 && session.finished && w.live_count==2)
}

@(test)
test_results_and_undo_free_with_captured_allocator :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator)
    defer mem.tracking_allocator_destroy(&tracker)
    allocator:=mem.tracking_allocator(&tracker)
    w:ecs.World; ecs.world_init(&w,allocator=allocator)
    reg:Component_Registry; editor_registry_init(&reg,allocator)
    editor_register(&w,&reg,"Position",Editor_Test_Component{})
    id:=ecs.create_entity(&w); editor_add_default(&w,id,reg.entries["Position"])
    result,group:=scene_execute(&w,&reg,Scene_Op{kind=.Set_Field,entity=id,component="Position",field="health",value=transmute([]byte)string("42")})
    attrs,no_undo:=scene_execute(&w,&reg,Scene_Op{kind=.Get_Attributes,entity=id,component="Position"})
    tool_result_destroy(&attrs); undo_group_destroy(&no_undo)
    tool_result_destroy(&result); undo_group_destroy(&group)
    editor_registry_destroy(&reg); ecs.world_destroy(&w)
    testing.expect_value(t,len(tracker.allocation_map),0)
}
