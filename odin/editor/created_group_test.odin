#+test
package editor

import ecs "../ecs"
import "core:testing"

@(test)
test_created_subtree_redo_maps_cross_references_and_preflights_all_rows :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"References",Reference_Test_Component{})
    a,b:=ecs.create_entity(&w),ecs.create_entity(&w)
    ecs.add_component(&w,a,Reference_Test_Component{b,{a,b}}); ecs.add_component(&w,b,Reference_Test_Component{a,{b,a}})
    command:=created_entities_group(&w,&reg,{a,b}); defer undo_group_destroy(&command)
    for _ in 0..<4 {
        testing.expect_value(t,undo_group(&w,&reg,&command),Scene_Error.None)
        entry:=reg.entries["References"]; delete_key(&reg.entries,"References")
        testing.expect_value(t,redo_group(&w,&reg,&command),Scene_Error.Component_Not_Found)
        ids:=ecs.entity_ids(&w); testing.expect_value(t,len(ids),0); delete(ids); reg.entries["References"]=entry
        testing.expect_value(t,redo_group(&w,&reg,&command),Scene_Error.None)
        fresh_a,fresh_b:=command.entities[0],command.entities[1]
        first,ok_first:=ecs.get_component(&w,fresh_a,Reference_Test_Component); second,ok_second:=ecs.get_component(&w,fresh_b,Reference_Test_Component)
        testing.expect(t,ok_first && ok_second && fresh_a!=a && fresh_b!=b)
        testing.expect_value(t,first,Reference_Test_Component{fresh_b,{fresh_a,fresh_b}})
        testing.expect_value(t,second,Reference_Test_Component{fresh_a,{fresh_b,fresh_a}})
    }
}
