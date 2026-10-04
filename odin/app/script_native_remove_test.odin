#+test
package app
import "core:testing"
import "core:mem"
import "core:os"
import "core:strings"
import ecs "../ecs"
import scene "../agent/scene"
import editor "../editor"
import km "../math"
import box3d "../physics/box3d"

@(private="file")
native_script_remove_retains_authored_rule_and_restores :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-script-remove-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }
    defer { os.remove_all(directory); delete(directory) }
    resources_path:=strings.concatenate({directory,"/resources"}); defer delete(resources_path)
    scripts_path:=strings.concatenate({resources_path,"/scripts"}); defer delete(scripts_path)
    testing.expect(t,os.make_directory(resources_path)==nil && os.make_directory(scripts_path)==nil)
    source_path:=strings.concatenate({scripts_path,"/remove.luau"}); defer delete(source_path)
    testing.expect(t,os.write_entire_file(source_path,`function on_spawn(entity,world)
        local victim=world:find_entity("Victim"); assert(world:entity_exists(victim))
        world:destroy_entity(victim)
    end`)==nil)
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner)
    testing.expect(t,asset_resources_init(&owner,directory,resources_path)==.None && physics_select_box3d(&owner,BOX3D_LIBRARY)==.None && script_native_init(&owner,LUAU_APP_LIBRARY)==.None)
    victim:=ecs.create_entity(&owner.world); partner:=ecs.create_entity(&owner.world)
    body:=physics_body({kind=.Sphere,radius=.25}); body.gravity_scale=0
    for id in ([2]ecs.Entity_Id{victim,partner}) { ecs.add_component(&owner.world,id,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&owner.world,id,body) }
    ecs.add_component(&owner.world,victim,Scene_Name{strings.clone("Victim")}); ecs.add_component(&owner.world,partner,Scene_Name{strings.clone("Partner")})
    emitter:=particle_defaults(); emitter.emit_rate=0; ecs.add_component(&owner.world,victim,Particle_Emitter{emitter})
    constraint:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,constraint,Scene_Name{strings.clone("Constraint")}); ecs.add_component(&owner.world,constraint,Physics_Joint{kind=.Fixed,a=victim,b=partner})
    controller:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,controller,Scene_Name{strings.clone("Controller")}); ecs.add_component(&owner.world,controller,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&owner.world,controller,Script_Component{path=strings.clone("scripts/remove.luau")})
    actions:=[1]scene.Event_Action{{kind=.Burst_Particles,target={kind=.Entity,entity=victim},count=1}}
    rules:=[1]scene.Trigger_Rule{{phase=.Exit,has_other=true,other=victim,actions=actions[:]}}
    result,undo:=trigger_execute(&owner,{action=.Create_Box,name="Sensor",position={0,0,0},half_extents={1,1,1},rules=rules[:]})
    testing.expect(t,result.error==.None); trigger:=result.entities[0]; editor.tool_result_destroy(&result); editor.undo_group_destroy(&undo)
    testing.expect(t,execute_test_simulation(t,&owner,.Play)==.None && simulation_step(&owner,.01)==.None)
    testing.expect(t,!ecs.entity_exists(&owner.world,victim))
    testing.expect(t,ecs.get_component_mut(&owner.world,constraint,Physics_Joint)==nil)
    backend:=ecs.get_resource_mut(&owner.world,box3d.Backend); testing.expect(t,len(backend.joints)==0)
    stored:=ecs.get_component_mut(&owner.world,trigger,Trigger_Rules)
    testing.expect(t,stored.rules[0].other==victim && stored.rules[0].actions[0].target.entity==victim)
    testing.expect(t,simulation_step(&owner,.01)==.None)
    testing.expect(t,len(stored.last_errors)==1)
    testing.expect(t,execute_test_simulation(t,&owner,.Stop)==.None)
    ids:=ecs.entity_ids(&owner.world); restored_victim,restored_partner,restored_sensor,restored_constraint:ecs.Entity_Id
    for id in ids { name,present:=ecs.get_component(&owner.world,id,Scene_Name); if !present { continue }; switch name.name {
    case "Victim": restored_victim=id
    case "Partner": restored_partner=id
    case "Sensor": restored_sensor=id
    case "Constraint": restored_constraint=id
    } }; delete(ids)
    testing.expect(t,restored_victim!=victim && ecs.entity_exists(&owner.world,restored_victim))
    restored:=ecs.get_component_mut(&owner.world,restored_sensor,Trigger_Rules); testing.expect(t,restored!=nil && len(restored.last_errors)==0 && restored.rules[0].other==restored_victim && restored.rules[0].actions[0].target.entity==restored_victim)
    joint:=ecs.get_component_mut(&owner.world,restored_constraint,Physics_Joint); testing.expect(t,joint!=nil && joint.a==restored_victim && joint.b==restored_partner)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when LUAU_APP_LIBRARY!="" && BOX3D_LIBRARY!="" {
    @(test)
    test_script_native_remove_retains_rules_and_stop_restores_fresh_references :: proc(t:^testing.T) { native_script_remove_retains_authored_rule_and_restores(t) }
}
