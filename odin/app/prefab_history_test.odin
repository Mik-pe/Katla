#+test
#+build darwin, linux
package app

import ecs "../ecs"
import editor "../editor"
import scene "../agent/scene"
import km "../math"
import "core:testing"
import "core:os"
import "core:strings"

@(private="file")
Prefab_History_Native :: struct { reject:bool,commits:int }
@(private="file")
prefab_history_prepare :: proc(state:rawptr,owner:^Authoring,ids:[]ecs.Entity_Id,mode:Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
    native:=cast(^Prefab_History_Native)state
    if native.reject { return nil,.Invalid_Operation }; return native,.None
}
@(private="file")
prefab_history_finish :: proc(state,token:rawptr,committed:bool) { if committed { (cast(^Prefab_History_Native)state).commits+=1 } }

@(test)
test_prefab_history_consumed_bursts_never_replay_and_removal_prunes_incoming_references :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-prefab-history-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource:=strings.concatenate({directory,"/resources"}); defer delete(resource); testing.expect(t,os.make_directory(resource)==nil)
    path:=strings.concatenate({directory,"/emitter.katprefab"}); defer delete(path)
    document:string=`(version:1,root:1,scene:(version:3,name:"Emitter",next_entity_id:2,entities:[(id:1,transform:(),particle_emitter:(active:true,burst_queue:[9]),rigid_body:(kind:Kinematic),collider_shape:Sphere(0.1))]))`
    testing.expect(t,os.write_entire_file(path,document)==nil)
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect(t,authoring_services_init(&owner)==.None && asset_resources_init(&owner,directory,resource)==.None)
    native:=Prefab_History_Native{}; ecs.insert_resource(&owner.world,Scene_Participant{&native,prefab_history_prepare,prefab_history_finish})
    inserted,create_group:=asset_authoring_execute(&owner,{action=.Instantiate,path=path,rotation={0,0,0,1},scale={1,1,1}}); testing.expect_value(t,inserted.error,editor.Scene_Error.None); if inserted.error!=.None { editor.tool_result_destroy(&inserted); return }
    testing.expect_value(t,native.commits,1); root:=inserted.entities[0]; editor.tool_result_destroy(&inserted); defer editor.undo_group_destroy(&create_group)
    emitter:=ecs.get_component_mut(&owner.world,root,Particle_Emitter); testing.expect(t,len(emitter.descriptor.burst_queue)==1 && emitter.descriptor.burst_queue[0]==9)
    consumed:=particle_take_bursts(emitter); testing.expect(t,len(consumed)==1 && consumed[0]==9); delete(consumed)
    testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&create_group),editor.Scene_Error.None)
    testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&create_group),editor.Scene_Error.None)
    root=create_group.entities[0]; emitter=ecs.get_component_mut(&owner.world,root,Particle_Emitter); testing.expect(t,len(emitter.descriptor.burst_queue)==0)
    survivor:=ecs.spawn(&owner.world,struct {transform:Scene_Transform,body:Physics_Body}{{km.TRANSFORM_IDENTITY},physics_body(Physics_Shape{kind=.Sphere,radius=0.1},.Dynamic)})
    joint_entity:=ecs.spawn(&owner.world,struct {transform:Scene_Transform,joint:Physics_Joint}{{km.TRANSFORM_IDENTITY},{kind=.PointToPoint,a=root,b=survivor}})
    actions:=[2]scene.Event_Action{{kind=.Burst_Particles,target={.Entity,root},count=4},{kind=.Emit,name="keep"}}
    rules:=[1]scene.Trigger_Rule{{phase=.Enter,actions=actions[:]}}
    trigger_result,trigger_group:=trigger_execute(&owner,{action=.Create_Box,name="Outside",half_extents={1,1,1},rules=rules[:]}); testing.expect_value(t,trigger_result.error,editor.Scene_Error.None); if trigger_result.error!=.None { editor.tool_result_destroy(&trigger_result); editor.undo_group_destroy(&trigger_group); return }
    trigger:=trigger_result.entities[0]; editor.tool_result_destroy(&trigger_result); editor.undo_group_destroy(&trigger_group)
    testing.expect_value(t,particle_burst(&owner.world,root,5),editor.Scene_Error.None)
    native.reject=true
    rejected,rejected_group:=asset_authoring_execute(&owner,{action=.Remove,root_entity=root}); defer editor.tool_result_destroy(&rejected); defer editor.undo_group_destroy(&rejected_group)
    testing.expect(t,rejected.error==.Invalid_Operation && rejected_group.state==nil && ecs.entity_exists(&owner.world,root) && ecs.get_component_mut(&owner.world,joint_entity,Physics_Joint)!=nil && len(ecs.get_component_mut(&owner.world,trigger,Trigger_Rules).rules[0].actions)==2 && len(ecs.get_component_mut(&owner.world,root,Particle_Emitter).descriptor.burst_queue)==1)
    native.reject=false
    removed,remove_group:=asset_authoring_execute(&owner,{action=.Remove,root_entity=root}); defer editor.tool_result_destroy(&removed); defer editor.undo_group_destroy(&remove_group); testing.expect_value(t,removed.error,editor.Scene_Error.None)
    testing.expect(t,!ecs.entity_exists(&owner.world,root) && ecs.get_component_mut(&owner.world,joint_entity,Physics_Joint)==nil && len(ecs.get_component_mut(&owner.world,trigger,Trigger_Rules).rules[0].actions)==1)
    testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&remove_group),editor.Scene_Error.None)
    restored:=ecs.get_component_mut(&owner.world,joint_entity,Physics_Joint); testing.expect(t,restored!=nil && restored.a!=root && ecs.entity_exists(&owner.world,restored.a)); if restored==nil { return }
    emitter=ecs.get_component_mut(&owner.world,restored.a,Particle_Emitter); testing.expect(t,emitter!=nil && len(emitter.descriptor.burst_queue)==0)
    restored_rules:=ecs.get_component_mut(&owner.world,trigger,Trigger_Rules); testing.expect(t,len(restored_rules.rules[0].actions)==2 && restored_rules.rules[0].actions[0].target.entity==restored.a)
    testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&remove_group),editor.Scene_Error.None)
    testing.expect(t,ecs.get_component_mut(&owner.world,joint_entity,Physics_Joint)==nil && len(ecs.get_component_mut(&owner.world,trigger,Trigger_Rules).rules[0].actions)==1)
}
