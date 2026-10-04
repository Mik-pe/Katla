package app
import "core:testing"
import "core:mem"
import "core:encoding/json"
import ecs "../ecs"
import editor "../editor"
import scene "../agent/scene"
import km "../math"

@(private="package")
register_test_scene_runtime :: proc(app:^Authoring) {
    assert(authoring_services_init(app)==.None)

}
@(test)
test_behavior_particle_full_descriptor_roundtrip_undo_and_queue_limits :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    app:Authoring; authoring_init(&app); register_test_scene_runtime(&app); entity:=ecs.create_entity(&app.world); ecs.add_component(&app.world,entity,Scene_Transform{km.TRANSFORM_IDENTITY})
    describe,describe_undo:=behavior_execute(&app,scene.Behavior_Op{action=.Describe}); testing.expect(t,describe.error==.None); editor.undo_group_destroy(&describe_undo)
    tree,parse_error:=json.parse(describe.data,spec=.JSON,parse_integers=true); testing.expect(t,parse_error==nil); object,_:=tree.(json.Object)
    result,undo:=behavior_execute(&app,scene.Behavior_Op{action=.Set_Particles,entity=entity,document=object["particle_example"]}); testing.expect(t,result.error==.None); editor.tool_result_destroy(&result); json.destroy_value(tree); editor.tool_result_destroy(&describe)
    for _ in 0..<1024 { testing.expect(t,particle_burst(&app.world,entity,1)==.None) }; testing.expect(t,particle_burst(&app.world,entity,1)==.Invalid_Operation)
    target:=ecs.get_component_mut(&app.world,entity,Scene_Transform); target.local.position={3,4,5}
    testing.expect(t,editor.undo_group(&app.world,&app.registry,&undo)==.None); _,present:=ecs.get_component(&app.world,entity,Particle_Emitter); testing.expect(t,!present && target.local.position==km.Vec3{3,4,5})
    testing.expect(t,editor.redo_group(&app.world,&app.registry,&undo)==.None); emitter:=ecs.get_component_mut(&app.world,entity,Particle_Emitter); testing.expect(t,emitter.descriptor.emit_rate==50 && emitter.descriptor.base_lifetime==5 && len(emitter.descriptor.burst_queue)==0)
    editor.undo_group_destroy(&undo); authoring_destroy(&app); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_particle_invalid_document_preserves_attachment_and_programmatic_rejection :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app); register_test_scene_runtime(&app); entity:=ecs.create_entity(&app.world); ecs.add_component(&app.world,entity,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&app.world,entity,Particle_Emitter{particle_defaults()})
    for invalid in ([]string{`{"base_lifetime":0}`,`{"velocity_direction":[0,0,0]}`,`{"lifetime_variation":1.1}`,`{"emitter_handle":1}`,`{"burst_queue":[0]}`,`{"color":[-1,0,0,1]}`}) {
        tree,parse_error:=json.parse(transmute([]byte)invalid,spec=.JSON,parse_integers=true); testing.expect(t,parse_error==nil)
        result,undo:=behavior_execute(&app,scene.Behavior_Op{action=.Set_Particles,entity=entity,document=tree}); testing.expect(t,result.error==.Invalid_Field_Value && undo.state==nil); editor.tool_result_destroy(&result); editor.undo_group_destroy(&undo); json.destroy_value(tree)
        emitter:=ecs.get_component_mut(&app.world,entity,Particle_Emitter); testing.expect(t,emitter.descriptor.base_lifetime==5)
    }
}
