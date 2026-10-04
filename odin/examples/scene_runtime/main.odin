//! Actual Rapier, Luau, animation and snapshot acceptance on the application's scene owner.
package main
import app "../../app"
import scene "../../agent/scene"
import ecs "../../ecs"
import editor "../../editor"
import km "../../math"
import "core:os"
import "core:mem"
import "core:fmt"
import "core:strings"

apply_mode :: proc(owner:^app.Authoring,mode:scene.Simulation_Op) { result,undo:=app.simulation_execute(owner,mode); assert(result.error==.None,"actual preview transition failed"); editor.tool_result_destroy(&result); editor.undo_group_destroy(&undo) }
main :: proc() {
    assert(len(os.args)==4,"usage: scene_runtime <runtime-library> <project-root> <resource-root>")
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:app.Authoring; app.authoring_init(&owner)
    assert(app.authoring_services_init(&owner)==.None,"canonical application services failed")
    assert(app.asset_resources_init(&owner,os.args[2],os.args[3])==.None,"actual resource roots unavailable")
    assert(app.scene_runtime_init(&owner,os.args[1])==.None,"actual dependency runtime unavailable")
    clips:=make([]app.Animation_Clip,2); clips[0]={name=strings.clone("Walk"),duration=0.2}; clips[1]={name=strings.clone("Run"),duration=0.25}
    model:=app.Animation_Model{clips=clips}; actor:=ecs.create_entity(&owner.world)
    ecs.add_component(&owner.world,actor,model); ecs.add_component(&owner.world,actor,app.Scene_Name{strings.clone("Actor")}); ecs.add_component(&owner.world,actor,app.Scene_Transform{km.transform(position={3,0,0})}); ecs.add_component(&owner.world,actor,app.physics_body({kind=.Sphere,radius=0.25},.Kinematic))
    assert(app.animation_play(&owner.world,actor,"Walk",0,true,1)==.None)
    actions:=[2]scene.Event_Action{{kind=.Play_Animation,target={kind=.Other},clip="Run",fade_seconds=0.25,speed=1,looping=true},{kind=.Emit,name="prefab_activated"}}
    rules:=[1]scene.Trigger_Rule{{phase=.Enter,other=actor,has_other=true,once=true,actions=actions[:]}}
    result,undo:=app.trigger_execute(&owner,{action=.Create_Box,name="Entrance",position={0,0,0},half_extents={1,1,1},rules=rules[:]}); assert(result.error==.None && len(result.entities)==1); trigger:=result.entities[0]; editor.tool_result_destroy(&result); editor.undo_group_destroy(&undo)
    emitter:=app.particle_defaults(); emitter.active=false; emitter.emit_rate=0; ecs.add_component(&owner.world,trigger,app.Particle_Emitter{emitter})
    result,undo=app.behavior_execute(&owner,{action=.Set_Script,entity=trigger,path="scripts/prefab-effect.luau"}); assert(result.error==.None,"actual Luau attachment validation failed"); editor.tool_result_destroy(&result); editor.undo_group_destroy(&undo)
    apply_mode(&owner,.Play); assert(app.simulation_step(&owner,0.1)==.None)
    ecs.get_component_mut(&owner.world,actor,app.Scene_Transform).local.position={0,0,0}; assert(app.simulation_step(&owner,0.1)==.None)
    player:=ecs.get_component_mut(&owner.world,actor,app.Animation_Player); particles:=ecs.get_component_mut(&owner.world,trigger,app.Particle_Emitter)
    assert(player.blending && player.target_clip=="Run"); assert(particles.descriptor.active && len(particles.descriptor.burst_queue)==1 && particles.descriptor.burst_queue[0]==32,"actual Luau callback did not publish ordered particle commands")
    apply_mode(&owner,.Pause); frozen:=player.blend_time; assert(app.simulation_step(&owner,0.25)==.None && player.blend_time==frozen)
    apply_mode(&owner,.Resume); assert(app.simulation_step(&owner,0.25)==.None && player.clip=="Run" && !player.blending)
    apply_mode(&owner,.Stop); assert(!ecs.entity_exists(&owner.world,actor) && !ecs.entity_exists(&owner.world,trigger)); ids:=ecs.entity_ids(&owner.world)
    new_actor,new_trigger:ecs.Entity_Id; for id in ids { label,present:=ecs.get_component(&owner.world,id,app.Scene_Name); if present { if label.name=="Actor" { new_actor=id } else if label.name=="Entrance" { new_trigger=id } } }; delete(ids)
    restored:=ecs.get_component_mut(&owner.world,new_trigger,app.Trigger_Rules); assert(restored!=nil && restored.rules[0].other==new_actor && !restored.fired[0]); restored_emitter:=ecs.get_component_mut(&owner.world,new_trigger,app.Particle_Emitter); assert(restored_emitter!=nil && !restored_emitter.descriptor.active && len(restored_emitter.descriptor.burst_queue)==0)
    app.authoring_destroy(&owner); assert(len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0,"scene runtime ownership leaked")
    fmt.println("scene runtime PASS: actual Rapier enter, ordered trigger animation, Luau subscription, 32 queued particle emissions, pause/resume, fresh-ID restore and empty allocator")
}
