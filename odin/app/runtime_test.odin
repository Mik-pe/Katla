#+test
package app

import "core:mem"
import "core:os"
import "core:strings"
import scene "../agent/scene"
import editor "../editor"
import "core:testing"
import ecs "../ecs"
import km "../math"

RUNTIME_LIBRARY :: #config(SCENE_RUNTIME_LIBRARY,"")

@(test)
test_runtime_missing_owner_refuses_authored_scripts_and_physics :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app); register_test_scene_runtime(&app)
    entity:=ecs.create_entity(&app.world); ecs.add_component(&app.world,entity,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&app.world,entity,physics_body(Physics_Shape{kind=.Sphere,radius=0.5}))
    testing.expect(t,execute_test_simulation(t,&app,.Play)==.Invalid_Operation && app.mode==.Editing)
    testing.expect(t,scene_runtime_init(&app,"/unavailable/scene-runtime.so")==.Application_Owned)
}
@(private="file")
native_runtime_acceptance :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-scene-runtime-*",context.allocator); testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_path:=strings.concatenate({directory,"/resources"}); defer delete(resource_path); script_path:=strings.concatenate({resource_path,"/scripts"}); defer delete(script_path)
    testing.expect(t,os.make_directory(resource_path)==nil && os.make_directory(script_path)==nil)
    file:=strings.concatenate({script_path,"/events.luau"}); defer delete(file)
    source:string=`function on_spawn(entity,world)
        world:on_event("activated",function(name,data,current)
            assert(data.trigger == entity)
            current:set_particles_active(entity,true)
            current:burst_particles(entity,32)
        end)
    end`
    testing.expect(t,os.write_entire_file(file,source)==nil)
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    app:Authoring; authoring_init(&app); register_test_scene_runtime(&app)
    testing.expect(t,asset_resources_init(&app,directory,resource_path)==.None); testing.expect(t,scene_runtime_init(&app,RUNTIME_LIBRARY)==.None)
    actor:=attach_test_animation(&app); ecs.add_component(&app.world,actor,Scene_Name{strings.clone("Actor")}); ecs.add_component(&app.world,actor,Scene_Transform{km.transform(position={3,0,0})}); ecs.add_component(&app.world,actor,physics_body(Physics_Shape{kind=.Sphere,radius=0.25},.Kinematic)); testing.expect(t,animation_play(&app.world,actor,"Walk",0,true,1)==.None)
    actions:=[2]scene.Event_Action{ {kind=.Play_Animation,target={kind=.Other},clip="Run",fade_seconds=0.25,speed=1,looping=true}, {kind=.Emit,name="activated"} }
    rules:=[1]scene.Trigger_Rule{ {phase=.Enter,has_other=true,other=actor,once=true,actions=actions[:]} }
    result,undo:=trigger_execute(&app,scene.Trigger_Op{action=.Create_Box,name="Entrance",position={0,0,0},half_extents={1,1,1},rules=rules[:]}); testing.expect(t,result.error==.None && len(result.entities)==1); trigger:=result.entities[0]; editor.tool_result_destroy(&result); editor.undo_group_destroy(&undo)
    emitter:=particle_defaults(); emitter.emit_rate=0; emitter.active=false; ecs.add_component(&app.world,trigger,Particle_Emitter{emitter})
    script_result,script_undo:=behavior_execute(&app,scene.Behavior_Op{action=.Set_Script,entity=trigger,path="scripts/events.luau"}); testing.expect(t,script_result.error==.None); editor.tool_result_destroy(&script_result); editor.undo_group_destroy(&script_undo)
    testing.expect(t,execute_test_simulation(t,&app,.Play)==.None)
    testing.expect(t,simulation_step(&app,0.1)==.None); testing.expect(t,len(ecs.get_component_mut(&app.world,trigger,Particle_Emitter).descriptor.burst_queue)==0)
    ecs.get_component_mut(&app.world,actor,Scene_Transform).local.position={0,0,0}
    testing.expect(t,simulation_step(&app,0.1)==.None)
    player:=ecs.get_component_mut(&app.world,actor,Animation_Player); testing.expect(t,player.blending && player.target_clip=="Run")
    particles:=ecs.get_component_mut(&app.world,trigger,Particle_Emitter); testing.expect(t,particles.descriptor.active && len(particles.descriptor.burst_queue)==1 && particles.descriptor.burst_queue[0]==32)
    trigger_rules:=ecs.get_component_mut(&app.world,trigger,Trigger_Rules); testing.expect(t,trigger_rules.fired[0] && len(trigger_rules.last_errors)==0)
    testing.expect(t,execute_test_simulation(t,&app,.Pause)==.None); frozen:=player.blend_time; testing.expect(t,simulation_step(&app,0.25)==.None && player.blend_time==frozen)
    testing.expect(t,execute_test_simulation(t,&app,.Resume)==.None); testing.expect(t,simulation_step(&app,0.25)==.None && player.clip=="Run" && !player.blending)
    ecs.get_component_mut(&app.world,actor,Scene_Transform).local.position={3,0,0}; testing.expect(t,simulation_step(&app,0.1)==.None); ecs.get_component_mut(&app.world,actor,Scene_Transform).local.position={0,0,0}; testing.expect(t,simulation_step(&app,0.1)==.None); testing.expect(t,len(particles.descriptor.burst_queue)==1)
    testing.expect(t,execute_test_simulation(t,&app,.Stop)==.None); testing.expect(t,!ecs.entity_exists(&app.world,actor) && !ecs.entity_exists(&app.world,trigger))
    ids:=ecs.entity_ids(&app.world); restored_actor,restored_trigger:ecs.Entity_Id
    for entity in ids { name,present:=ecs.get_component(&app.world,entity,Scene_Name); if present { if name.name=="Actor" { restored_actor=entity } else if name.name=="Entrance" { restored_trigger=entity } } }; delete(ids)
    restored:=ecs.get_component_mut(&app.world,restored_trigger,Trigger_Rules); testing.expect(t,restored!=nil && restored.rules[0].other==restored_actor && !restored.fired[0])
    restored_particles:=ecs.get_component_mut(&app.world,restored_trigger,Particle_Emitter); testing.expect(t,restored_particles!=nil && !restored_particles.descriptor.active && len(restored_particles.descriptor.burst_queue)==0)
    testing.expect(t,execute_test_simulation(t,&app,.Play)==.None); testing.expect(t,simulation_step(&app,0.1)==.None); ecs.get_component_mut(&app.world,restored_actor,Scene_Transform).local.position={0,0,0}; testing.expect(t,simulation_step(&app,0.1)==.None); testing.expect(t,len(restored_particles.descriptor.burst_queue)==1)
    authoring_destroy(&app); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when RUNTIME_LIBRARY!="" {
@(test)
test_runtime_native_rapier_trigger_luau_animation_and_stop :: proc(t:^testing.T) { native_runtime_acceptance(t) }
}
@(test)
test_physics_completed_pose_batch_rejection_preserves_every_target :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app); register_test_scene_runtime(&app)
    first:=ecs.create_entity(&app.world); second:=ecs.create_entity(&app.world)
    for entity in ([2]ecs.Entity_Id{first,second}) { ecs.add_component(&app.world,entity,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&app.world,entity,physics_body(Physics_Shape{kind=.Sphere,radius=0.5})) }
    poses:=[2]Physics_Resolved_Pose{ {u64(first),{1,2,3},{0,0,0,1},{1,0,0}}, {u64(second),{4,5,6},{0,0,0,0},{0,0,0}} }
    testing.expect(t,physics_commit_poses(&app,poses[:])==.Invalid_Field_Value); testing.expect(t,ecs.get_component_mut(&app.world,first,Scene_Transform).local.position==km.VEC3_ZERO)
    poses[1].rotation={0,0,0,1}; ecs.remove_component(&app.world,second,Physics_Body); testing.expect(t,physics_commit_poses(&app,poses[:])==.Component_Not_Found); testing.expect(t,ecs.get_component_mut(&app.world,first,Scene_Transform).local.position==km.VEC3_ZERO)
}
