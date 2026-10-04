#+test
package app

import scene "../agent/scene"
import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:testing"

@(private="file")
Reference_Test_Native :: struct { reject:bool }
@(private="file")
reference_test_prepare :: proc(state:rawptr,owner:^Authoring,ids:[]ecs.Entity_Id,mode:Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
    return nil,.Invalid_Operation if (cast(^Reference_Test_Native)state).reject else .None
}
@(private="file")
reference_test_finish :: proc(state,token:rawptr,commit:bool) {}

@(test)
test_authored_delete_prunes_rules_and_overlaps_atomically_and_undo_maps_fresh_target :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner)
    testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    result,group:=scene_action_execute(&owner,{kind=.Spawn,name="Emitter",shape="cube",scale={1,1,1}})
    testing.expect_value(t,result.error,editor.Scene_Error.None)
    if result.error!=.None { editor.tool_result_destroy(&result); return }
    target:=result.entities[0]; editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    ecs.add_component(&owner.world,target,Particle_Emitter{descriptor=particle_defaults()})
    survivor:=ecs.spawn(&owner.world,struct {transform:Scene_Transform}{{km.TRANSFORM_IDENTITY}})
    emit:=[1]scene.Event_Action{{kind=.Emit,name="filtered"}}
    mixed:=[2]scene.Event_Action{{kind=.Burst_Particles,target={.Entity,target},count=4},{kind=.Emit,name="kept"}}
    burst:=[1]scene.Event_Action{{kind=.Burst_Particles,target={.Entity,target},count=1}}
    rules:=[3]scene.Trigger_Rule{{phase=.Enter,other=target,has_other=true,actions=emit[:]},{phase=.Enter,actions=mixed[:]},{phase=.Exit,actions=burst[:]}}
    result,group=trigger_execute(&owner,{action=.Create_Box,name="Trigger",half_extents={1,1,1},rules=rules[:]})
    testing.expect_value(t,result.error,editor.Scene_Error.None)
    if result.error!=.None { editor.tool_result_destroy(&result); editor.undo_group_destroy(&group); return }
    trigger:=result.entities[0]; editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    volume:=ecs.get_component_mut(&owner.world,trigger,Trigger_Volume)
    volume.overlapping=make([dynamic]ecs.Entity_Id,owner.world.allocator); append(&volume.overlapping,target,survivor)
    native:=Reference_Test_Native{reject=true}; ecs.insert_resource(&owner.world,Scene_Participant{&native,reference_test_prepare,reference_test_finish})
    result,group=scene_action_execute(&owner,{kind=.Destroy,entity=target})
    testing.expect(t,result.error==.Invalid_Operation && group.state==nil && ecs.entity_exists(&owner.world,target))
    testing.expect_value(t,len(ecs.get_component_mut(&owner.world,trigger,Trigger_Rules).rules),3)
    testing.expect_value(t,len(ecs.get_component_mut(&owner.world,trigger,Trigger_Volume).overlapping),2)
    editor.tool_result_destroy(&result)
    native.reject=false
    result,group=scene_action_execute(&owner,{kind=.Destroy,entity=target})
    testing.expect_value(t,result.error,editor.Scene_Error.None)
    testing.expect(t,!ecs.entity_exists(&owner.world,target))
    kept:=ecs.get_component_mut(&owner.world,trigger,Trigger_Rules)
    testing.expect(t,len(kept.rules)==1 && len(kept.rules[0].actions)==1 && kept.rules[0].actions[0].name=="kept")
    current_volume:=ecs.get_component_mut(&owner.world,trigger,Trigger_Volume)
    testing.expect(t,len(current_volume.overlapping)==1 && current_volume.overlapping[0]==survivor)
    editor.agent_record_action(&owner.agent.session,{kind=.Destroy,entity=target},&result,&group)
    snapshot,error:=scene_snapshot_capture(&owner)
    testing.expect_value(t,error,editor.Scene_Error.None); scene_snapshot_destroy(&snapshot)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None)
    restored:=ecs.get_component_mut(&owner.world,trigger,Trigger_Rules)
    testing.expect_value(t,len(restored.rules),3)
    fresh:=restored.rules[0].other
    testing.expect(t,fresh!=target && ecs.entity_exists(&owner.world,fresh) && restored.rules[1].actions[0].target.entity==fresh)
    restored_volume:=ecs.get_component_mut(&owner.world,trigger,Trigger_Volume)
    testing.expect(t,len(restored_volume.overlapping)==2 && restored_volume.overlapping[0]==fresh)
    snapshot,error=scene_snapshot_capture(&owner)
    testing.expect_value(t,error,editor.Scene_Error.None); scene_snapshot_destroy(&snapshot)
    testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None)
    testing.expect(t,!ecs.entity_exists(&owner.world,fresh) && len(ecs.get_component_mut(&owner.world,trigger,Trigger_Rules).rules)==1)
}
