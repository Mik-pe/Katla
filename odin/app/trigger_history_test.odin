package app
import "core:testing"
import "core:mem"
import ecs "../ecs"
import editor "../editor"
import scene "../agent/scene"

@(private="file")
Trigger_Admission_Test :: struct { reject:bool,prepares,commits:int,observed_rules:int }
@(private="file")
trigger_admission_test_prepare :: proc(state:rawptr,owner:^Authoring,ids:[]ecs.Entity_Id,mode:Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
    witness:=cast(^Trigger_Admission_Test)state; witness.prepares+=1
    if len(ids)==1 { if rules:=ecs.get_component_mut(&owner.world,ids[0],Trigger_Rules); rules!=nil { witness.observed_rules=len(rules.rules) } }
    if witness.reject { return nil,.Invalid_Operation }; return witness,.None
}
@(private="file")
trigger_admission_test_finish :: proc(state,token:rawptr,commit:bool) { if commit { (cast(^Trigger_Admission_Test)state).commits+=1 } }

@(test)
test_trigger_create_and_rule_changes_prepare_before_publication_or_history :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); testing.expect(t,authoring_services_init(&owner)==.None)
    witness:=Trigger_Admission_Test{reject=true}; ecs.insert_resource(&owner.world,Scene_Participant{&witness,trigger_admission_test_prepare,trigger_admission_test_finish})
    key:=ecs.get_resource_mut(&owner.world,Scene_Identity).next_entity_id
    created,group:=trigger_execute(&owner,{action=.Create_Box,name="Rejected",half_extents={1,1,1}})
    testing.expect(t,created.error==.Invalid_Operation && group.state==nil && owner.world.live_count==0 && witness.prepares==1 && witness.commits==0)
    testing.expect(t,ecs.get_resource_mut(&owner.world,Scene_Identity).next_entity_id==key && len(owner.agent.session.actions)==0)
    editor.tool_result_destroy(&created); editor.undo_group_destroy(&group)
    witness.reject=false
    created,group=trigger_execute(&owner,{action=.Create_Box,name="Accepted",half_extents={1,2,3}})
    testing.expect(t,created.error==.None && group.state!=nil && witness.prepares==2 && witness.commits==1 && len(created.entities)==1)
    entity:=created.entities[0]; testing.expect(t,ecs.get_component_mut(&owner.world,entity,Scene_Key).value==key)
    editor.tool_result_destroy(&created)
    actions:=[1]scene.Event_Action{{kind=.Emit,name="door"}}; rules:=[1]scene.Trigger_Rule{{phase=.Enter,actions=actions[:]}}
    witness.reject=true
    changed,change:=trigger_execute(&owner,{action=.Set_Rules,entity=entity,rules=rules[:]})
    testing.expect(t,changed.error==.Invalid_Operation && change.state==nil && ecs.entity_exists(&owner.world,entity) && owner.world.live_count==1)
    testing.expect(t,len(ecs.get_component_mut(&owner.world,entity,Trigger_Rules).rules)==0 && witness.observed_rules==1 && witness.commits==1 && len(owner.agent.session.actions)==0)
    editor.tool_result_destroy(&changed); editor.undo_group_destroy(&change)
    witness.reject=false; testing.expect(t,editor.undo_group(&owner.world,&owner.registry,&group)==.None && !ecs.entity_exists(&owner.world,entity))
    testing.expect(t,editor.redo_group(&owner.world,&owner.registry,&group)==.None && group.entities[0]!=entity)
    restored:=group.entities[0]; testing.expect(t,ecs.entity_exists(&owner.world,restored) && ecs.get_component_mut(&owner.world,restored,Scene_Key).value==key)
    editor.undo_group_destroy(&group); authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(test)
test_trigger_rule_history_retains_live_animation_and_consumed_particle_work :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); testing.expect(t,authoring_services_init(&owner)==.None)
    created,creation:=trigger_execute(&owner,{action=.Create_Box,name="Animated",half_extents={1,1,1}}); testing.expect(t,created.error==.None)
    entity:=created.entities[0]; editor.tool_result_destroy(&created); editor.undo_group_destroy(&creation)
    model_entity:=attach_test_animation(&owner); source:=ecs.get_component_mut(&owner.world,model_entity,Animation_Model); source.clips[0].duration=.25
    model:Animation_Model; animation_model_clone(&model,source); ecs.add_component(&owner.world,entity,model)
    testing.expect(t,animation_play(&owner.world,entity,"Walk",0,true,1)==.None); ecs.get_component_mut(&owner.world,entity,Animation_Player).time=.0625
    ecs.add_component(&owner.world,entity,Particle_Emitter{particle_defaults()}); testing.expect(t,particle_burst(&owner.world,entity,11)==.None)
    actions:=[1]scene.Event_Action{{kind=.Emit,name="new"}}; rules:=[1]scene.Trigger_Rule{{phase=.Enter,actions=actions[:]}}
    changed,change:=trigger_execute(&owner,{action=.Set_Rules,entity=entity,rules=rules[:]}); testing.expect(t,changed.error==.None && change.state!=nil); editor.tool_result_destroy(&changed)
    emitter:=ecs.get_component_mut(&owner.world,entity,Particle_Emitter)
    testing.expect(t,len(emitter.descriptor.burst_queue)==1 && emitter.descriptor.burst_queue[0]==11)
    consumed:=particle_take_bursts(emitter); delete(consumed); particle_burst(&owner.world,entity,23)
    animation_update(&owner.world,.25)
    clock:=ecs.get_component_mut(&owner.world,entity,Animation_Player).time
    witness:=Trigger_Admission_Test{reject=true}; ecs.insert_resource(&owner.world,Scene_Participant{&witness,trigger_admission_test_prepare,trigger_admission_test_finish})
    testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&change),editor.Scene_Error.Invalid_Operation)
    testing.expect(t,ecs.get_component_mut(&owner.world,entity,Trigger_Rules).rules[0].actions[0].name=="new")
    witness.reject=false; testing.expect(t,editor.undo_group(&owner.world,&owner.registry,&change)==.None)
    player:=ecs.get_component_mut(&owner.world,entity,Animation_Player); emitter=ecs.get_component_mut(&owner.world,entity,Particle_Emitter)
    testing.expect(t,player.time==clock && player.loop_count==1 && len(player.events)==1 && player.events[0].clip=="Walk")
    testing.expect(t,len(emitter.descriptor.burst_queue)==1 && emitter.descriptor.burst_queue[0]==23 && len(ecs.get_component_mut(&owner.world,entity,Trigger_Rules).rules)==0)
    consumed=particle_take_bursts(emitter); delete(consumed); particle_burst(&owner.world,entity,47)
    animation_update(&owner.world,.125); clock=ecs.get_component_mut(&owner.world,entity,Animation_Player).time
    testing.expect(t,editor.redo_group(&owner.world,&owner.registry,&change)==.None)
    player=ecs.get_component_mut(&owner.world,entity,Animation_Player); emitter=ecs.get_component_mut(&owner.world,entity,Particle_Emitter)
    testing.expect(t,player.time==clock && player.loop_count==1 && len(player.events)==1)
    testing.expect(t,len(emitter.descriptor.burst_queue)==1 && emitter.descriptor.burst_queue[0]==47 && ecs.get_component_mut(&owner.world,entity,Trigger_Rules).rules[0].actions[0].name=="new")
    editor.undo_group_destroy(&change); authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(test)
test_trigger_delete_recreation_remaps_self_rules_without_replaying_bursts :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect(t,authoring_services_init(&owner)==.None)
    created,creation:=trigger_execute(&owner,{action=.Create_Box,name="Restored",half_extents={1,1,1}}); testing.expect(t,created.error==.None)
    entity:=created.entities[0]; editor.tool_result_destroy(&created); editor.undo_group_destroy(&creation)
    ecs.add_component(&owner.world,entity,Particle_Emitter{particle_defaults()})
    actions:=[1]scene.Event_Action{{kind=.Burst_Particles,target={kind=.Entity,entity=entity},count=5}}
    rules:=[1]scene.Trigger_Rule{{phase=.Enter,actions=actions[:]}}
    configured,configuration:=trigger_execute(&owner,{action=.Set_Rules,entity=entity,rules=rules[:]}); testing.expect(t,configured.error==.None); editor.tool_result_destroy(&configured); editor.undo_group_destroy(&configuration)
    testing.expect(t,particle_burst(&owner.world,entity,17)==.None)
    removed,removal:=scene_action_execute(&owner,{kind=.Destroy,entity=entity}); testing.expect(t,removed.error==.None && !ecs.entity_exists(&owner.world,entity)); editor.tool_result_destroy(&removed)
    testing.expect(t,editor.undo_group(&owner.world,&owner.registry,&removal)==.None)
    restored:=removal.entities[0]; testing.expect(t,restored!=entity && ecs.entity_exists(&owner.world,restored))
    emitter:=ecs.get_component_mut(&owner.world,restored,Particle_Emitter)
    testing.expect(t,emitter!=nil && len(emitter.descriptor.burst_queue)==0)
    restored_rules:=ecs.get_component_mut(&owner.world,restored,Trigger_Rules)
    testing.expect(t,restored_rules.rules[0].actions[0].target.entity==restored)
    testing.expect(t,editor.redo_group(&owner.world,&owner.registry,&removal)==.None && !ecs.entity_exists(&owner.world,restored))
    editor.undo_group_destroy(&removal)
}
