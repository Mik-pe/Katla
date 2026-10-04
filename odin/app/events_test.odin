package app
import "core:testing"
import "core:mem"
import "core:encoding/json"
import ecs "../ecs"
import editor "../editor"
import scene "../agent/scene"
import km "../math"

@(test)
test_trigger_atomic_validation_ordered_failures_once_and_reference_restore :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    app:Authoring; authoring_init(&app); register_test_scene_runtime(&app)
    actor:=attach_test_animation(&app); ecs.add_component(&app.world,actor,Scene_Transform{km.TRANSFORM_IDENTITY})
    empty_result,creation:=trigger_execute(&app,scene.Trigger_Op{action=.Create_Box,name="Trigger",position={0,0,0},half_extents={1,1,1}}); testing.expect(t,empty_result.error==.None); trigger:=empty_result.entities[0]; editor.tool_result_destroy(&empty_result)
    bad_actions:=[1]scene.Event_Action{{kind=.Play_Animation,target={kind=.Entity,entity=actor},clip="missing",fade_seconds=0.25,speed=1,looping=true}}; bad_rules:=[1]scene.Trigger_Rule{{phase=.Enter,actions=bad_actions[:]}}
    invalid,invalid_undo:=trigger_execute(&app,scene.Trigger_Op{action=.Set_Rules,entity=trigger,rules=bad_rules[:]}); testing.expect(t,invalid.error==.Invalid_Operation && invalid_undo.state==nil); editor.tool_result_destroy(&invalid); editor.undo_group_destroy(&invalid_undo); testing.expect(t,len(ecs.get_component_mut(&app.world,trigger,Trigger_Rules).rules)==0)
    actions:=[2]scene.Event_Action{{kind=.Play_Animation,target={kind=.Other},clip="missing",fade_seconds=0.25,speed=1,looping=true},{kind=.Emit,name="continued"}}; rules:=[1]scene.Trigger_Rule{{phase=.Enter,has_other=true,other=actor,once=true,actions=actions[:]}}
    result,change:=trigger_execute(&app,scene.Trigger_Op{action=.Set_Rules,entity=trigger,rules=rules[:]}); testing.expect(t,result.error==.None); editor.tool_result_destroy(&result)
    events_dispatch(&app,Physics_Event{.Enter,trigger,actor}); stored:=ecs.get_component_mut(&app.world,trigger,Trigger_Rules); signals:=ecs.get_resource_mut(&app.world,Script_Signals)
    testing.expect(t,stored.fired[0] && len(stored.last_errors)==1 && len(signals.pending)==1 && signals.pending[0].name=="continued")
    events_dispatch(&app,Physics_Event{.Exit,trigger,actor}); events_dispatch(&app,Physics_Event{.Enter,trigger,actor}); testing.expect(t,len(signals.pending)==1)
    snapshot,capture_error:=scene_snapshot_capture(&app); testing.expect(t,capture_error==.None); testing.expect(t,scene_snapshot_restore(&app,&snapshot)==.None); scene_snapshot_destroy(&snapshot)
    ids:=ecs.entity_ids(&app.world); new_trigger,new_actor:ecs.Entity_Id; for id in ids { if _,present:=ecs.get_component(&app.world,id,Trigger_Rules); present { new_trigger=id }; if _,present:=ecs.get_component(&app.world,id,Animation_Model); present { new_actor=id } }; delete(ids)
    restored:=ecs.get_component_mut(&app.world,new_trigger,Trigger_Rules); testing.expect(t,restored!=nil && restored.rules[0].other==new_actor && restored.rules[0].has_other)
    events_reset(&app); testing.expect(t,!restored.fired[0] && len(restored.last_errors)==0 && len(signals.pending)==0)
    wire:=trigger_wire_rules(restored.rules); bytes,marshal_error:=json.marshal(wire); testing.expect(t,marshal_error==nil); delete(bytes); json.destroy_value(wire)
    editor.undo_group_destroy(&change); editor.undo_group_destroy(&creation); authoring_destroy(&app); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_trigger_shared_creation_undo_redo_preserves_unique_generations :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app); register_test_scene_runtime(&app)
    result,undo:=trigger_execute(&app,scene.Trigger_Op{action=.Create_Box,name="Door",half_extents={1,2,3}}); testing.expect(t,result.error==.None); original:=result.entities[0]; editor.tool_result_destroy(&result)
    testing.expect(t,editor.undo_group(&app.world,&app.registry,&undo)==.None && !ecs.entity_exists(&app.world,original))
    testing.expect(t,editor.redo_group(&app.world,&app.registry,&undo)==.None); testing.expect(t,len(undo.entities)==1 && undo.entities[0]!=original && ecs.entity_exists(&app.world,undo.entities[0]))
    body,present:=ecs.get_component(&app.world,undo.entities[0],Physics_Body); testing.expect(t,present && body.sensor && body.shape.half_extents==[3]f32{1,2,3}); editor.undo_group_destroy(&undo)
}
