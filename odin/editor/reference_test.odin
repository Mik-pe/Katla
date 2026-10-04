#+test
package editor

import ecs "../ecs"
import "core:testing"

Reference_Test_Component :: struct { target:ecs.Entity_Id, children:[2]ecs.Entity_Id }

@(test)
test_references_restore_live_and_earlier_snapshots :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"References",Reference_Test_Component{})
    session:Agent_Session; agent_session_init(&session); defer agent_session_destroy(&session)
    target:=ecs.create_entity(&w)
    owner:=ecs.create_entity(&w)
    ecs.add_component(&w,owner,Reference_Test_Component{target,{target,owner}})
    action:=agent_execute(&session,&w,&reg,{kind=.Remove_Component,entity=owner,component="References"})
    testing.expect_value(t,action.result.error,Scene_Error.None)
    action=agent_execute(&session,&w,&reg,{kind=.Destroy,entity=target})
    testing.expect_value(t,action.result.error,Scene_Error.None)
    testing.expect_value(t,agent_undo_last(&session,&w,&reg),Scene_Error.None)
    current:=session.actions[0].operation.entity
    testing.expect_value(t,current,owner)
    ids:=ecs.entity_ids(&w); defer delete(ids)
    fresh:ecs.Entity_Id
    for id in ids { if id!=owner { fresh=id } }
    testing.expect(t,fresh!=target && ecs.entity_exists(&w,fresh))
    testing.expect_value(t,agent_undo_last(&session,&w,&reg),Scene_Error.None)
    references,ok:=ecs.get_component(&w,owner,Reference_Test_Component)
    testing.expect(t,ok && references.target==fresh && references.children==[2]ecs.Entity_Id{fresh,owner})
    destroy,group:=scene_execute(&w,&reg,{kind=.Destroy,entity=fresh}); defer tool_result_destroy(&destroy); defer undo_group_destroy(&group)
    testing.expect_value(t,undo_group(&w,&reg,&group),Scene_Error.None)
    restored,_:=ecs.get_component(&w,owner,Reference_Test_Component)
    testing.expect(t,restored.target!=fresh && restored.target==group.entities[0])
}

@(test)
test_reference_maps_reject_outside_scene_including_zero_identity :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"References",Reference_Test_Component{})
    mapping:=make(map[ecs.Entity_Id]ecs.Entity_Id); defer delete(mapping)
    value:=Reference_Test_Component{0,{0,0}}
    testing.expect(t,!component_map_references(reg.entries["References"],&value,{mapping,true}))
    mapping[0]=42
    testing.expect(t,component_map_references(reg.entries["References"],&value,{mapping,true}))
    testing.expect_value(t,value,Reference_Test_Component{42,{42,42}})
}

@(test)
test_repeated_undo_redo_remaps_command_self_references :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"References",Reference_Test_Component{})
    entity:=ecs.create_entity(&w); ecs.add_component(&w,entity,Reference_Test_Component{entity,{entity,entity}})
    result,command:=scene_execute(&w,&reg,{kind=.Destroy,entity=entity}); defer tool_result_destroy(&result); defer undo_group_destroy(&command)
    for _ in 0..<4 {
        testing.expect_value(t,undo_group(&w,&reg,&command),Scene_Error.None)
        restored:=command.entities[0]; component,ok:=ecs.get_component(&w,restored,Reference_Test_Component)
        testing.expect(t,ok && component.target==restored && component.children==[2]ecs.Entity_Id{restored,restored})
        testing.expect_value(t,redo_group(&w,&reg,&command),Scene_Error.None)
        testing.expect(t,!ecs.entity_exists(&w,restored))
    }
}

Tagged_Reference_Test :: struct { reference:u64 `inspect:"entity_ref"` }
@(test)
test_explicit_entity_reference_field_tags_map_u64_values :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"Tagged",Tagged_Reference_Test{})
    mapping:=make(map[ecs.Entity_Id]ecs.Entity_Id); defer delete(mapping); mapping[900]=1000
    value:=Tagged_Reference_Test{900}
    testing.expect(t,component_map_references(reg.entries["Tagged"],&value,{mapping,true}))
    testing.expect_value(t,value.reference,u64(1000))
}
