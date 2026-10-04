package app

import "core:testing"
import "core:mem"
import "core:encoding/json"
import ecs "../ecs"
import editor "../editor"
import scene "../agent/scene"

@(private="package")
execute_test_simulation :: proc(t:^testing.T,app:^Authoring,op:scene.Simulation_Op)->editor.Scene_Error {
    result,undo:=simulation_execute(app,op); defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&undo); return result.error
}
@(test)
test_preview_pause_resume_stop_restore_exact_authored_state :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    app:Authoring; authoring_init(&app); animation_register(&app.world,&app.registry); simulation_init(&app)
    id:=attach_test_animation(&app); testing.expect(t,animation_play(&app.world,id,"Walk",0,true,1)==.None)
    protected:=ecs.create_entity(&app.world); ecs.add_component(&app.world,protected,Editor_Hidden{})
    testing.expect(t,execute_test_simulation(t,&app,.Play)==.None && app.mode==.Playing)
    testing.expect(t,execute_test_simulation(t,&app,.Play)==.None); testing.expect(t,simulation_step(&app,0.25)==.None)
    player:=ecs.get_component_mut(&app.world,id,Animation_Player); time:=player.time
    testing.expect(t,execute_test_simulation(t,&app,.Pause)==.None && app.mode==.Paused); testing.expect(t,simulation_step(&app,1)==.None && player.time==time)
    testing.expect(t,execute_test_simulation(t,&app,.Resume)==.None && app.mode==.Playing); testing.expect(t,animation_play(&app.world,id,"Run",0,false,1)==.None)
    extra:=ecs.create_entity(&app.world); testing.expect(t,execute_test_simulation(t,&app,.Stop)==.None && app.mode==.Editing)
    testing.expect(t,!ecs.entity_exists(&app.world,id) && !ecs.entity_exists(&app.world,extra) && ecs.entity_exists(&app.world,protected))
    ids:=ecs.entity_ids(&app.world); found:=false
    for entity in ids { restored:=ecs.get_component_mut(&app.world,entity,Animation_Player); if restored!=nil { found=true; testing.expect(t,restored.clip=="Walk" && restored.time==0 && restored.playing) } }; delete(ids); testing.expect(t,found)
    testing.expect(t,execute_test_simulation(t,&app,.Stop)==.None); authoring_destroy(&app); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_preview_failed_restore_retains_recoverable_snapshot :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app); animation_register(&app.world,&app.registry); simulation_init(&app); id:=attach_test_animation(&app)
    testing.expect(t,execute_test_simulation(t,&app,.Play)==.None)
    runtime:=ecs.get_resource_mut(&app.world,Simulation_Runtime); component:=&runtime.snapshot.entities[0].components[0]
    original:=component.data; component.data=transmute([]byte)string(`{"invalid":`)
    testing.expect(t,execute_test_simulation(t,&app,.Stop)==.Decode_Failed); testing.expect(t,app.mode==.Playing && runtime.captured && ecs.entity_exists(&app.world,id))
    component.data=original; testing.expect(t,execute_test_simulation(t,&app,.Stop)==.None)
}

@(test)
test_preview_play_response_resets_previous_session_counters :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app); register_test_scene_runtime(&app)
    runtime:=ecs.get_resource_mut(&app.world,Simulation_Runtime); runtime.elapsed_seconds=12; runtime.steps=48
    result,undo:=simulation_execute(&app,.Play); defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&undo)
    testing.expect(t,result.error==.None && runtime.elapsed_seconds==0 && runtime.steps==0)
    response:struct {mode:string,changed,runtime_ids_replaced:bool,elapsed_seconds:f64,steps:u64}; err:=json.unmarshal(result.data,&response); defer delete(response.mode)
    testing.expect(t,err==nil && response.mode=="playing" && response.elapsed_seconds==0 && response.steps==0)
}
