package app

import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:testing"
import "core:mem"
import "core:encoding/json"
import "core:bytes"

@(private="package")
particle_test_field :: proc(value:Particle_Descriptor)->[]byte { data,error:=json.marshal(value); assert(error==nil); return data }

@(test)
test_particle_authored_gesture_preserves_pending_work_without_replaying_consumed_bursts :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    entity:=ecs.spawn(&owner.world,struct {transform:Scene_Transform}{{km.TRANSFORM_IDENTITY}})
    added,add_group:=scene_action_execute(&owner,{kind=.Add_Component,entity=entity,component="ParticleEmitter"}); testing.expect_value(t,added.error,editor.Scene_Error.None); editor.tool_result_destroy(&added); editor.undo_group_destroy(&add_group)
    emitter:=ecs.get_component_mut(&owner.world,entity,Particle_Emitter); testing.expect(t,particle_descriptor_valid(emitter.descriptor) && emitter.descriptor.emit_rate==50)
    testing.expect_value(t,particle_burst(&owner.world,entity,32),editor.Scene_Error.None)
    gesture:Scene_Gesture; testing.expect_value(t,scene_gesture_begin(&owner,&gesture,{entity}),editor.Scene_Error.None)
    transform:=transmute([]byte)string(`{"position":[1,2,3],"rotation":[0,0,0,1],"scale":[1,1,1]}`)
    testing.expect_value(t,scene_gesture_preview(&owner,&gesture,{kind=.Set_Field,component="SceneTransform",field="local",value=transform}),editor.Scene_Error.None)
    accepted:=particle_take_bursts(ecs.get_component_mut(&owner.world,entity,Particle_Emitter)); testing.expect(t,len(accepted)==1 && accepted[0]==32); delete(accepted)
    testing.expect_value(t,particle_burst(&owner.world,entity,17),editor.Scene_Error.None)
    testing.expect_value(t,scene_gesture_finish(&owner,&gesture),editor.Scene_Error.None)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None); testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None)
    emitter=ecs.get_component_mut(&owner.world,entity,Particle_Emitter); testing.expect(t,len(emitter.descriptor.burst_queue)==1 && emitter.descriptor.burst_queue[0]==17)
    testing.expect_value(t,scene_gesture_begin(&owner,&gesture,{entity}),editor.Scene_Error.None)
    descriptor:=emitter.descriptor; descriptor.emit_rate=123
    field:=particle_test_field(descriptor); testing.expect_value(t,scene_gesture_preview(&owner,&gesture,{kind=.Set_Field,component="ParticleEmitter",field="descriptor",value=field}),editor.Scene_Error.None); delete(field)
    accepted=particle_take_bursts(ecs.get_component_mut(&owner.world,entity,Particle_Emitter)); testing.expect(t,len(accepted)==1 && accepted[0]==17); delete(accepted)
    testing.expect_value(t,particle_burst(&owner.world,entity,9),editor.Scene_Error.None)
    testing.expect_value(t,scene_gesture_finish(&owner,&gesture),editor.Scene_Error.None)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None)
    emitter=ecs.get_component_mut(&owner.world,entity,Particle_Emitter); testing.expect(t,emitter.descriptor.emit_rate==50 && len(emitter.descriptor.burst_queue)==1 && emitter.descriptor.burst_queue[0]==9)
    testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None)
    emitter=ecs.get_component_mut(&owner.world,entity,Particle_Emitter); testing.expect(t,emitter.descriptor.emit_rate==123 && len(emitter.descriptor.burst_queue)==1 && emitter.descriptor.burst_queue[0]==9)
    removed,remove_group:=scene_action_execute(&owner,{kind=.Remove_Component,entity=entity,component="ParticleEmitter"}); testing.expect_value(t,removed.error,editor.Scene_Error.None); editor.tool_result_destroy(&removed)
    testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&remove_group),editor.Scene_Error.None)
    emitter=ecs.get_component_mut(&owner.world,entity,Particle_Emitter); testing.expect(t,emitter.descriptor.emit_rate==123 && len(emitter.descriptor.burst_queue)==0)
    editor.undo_group_destroy(&remove_group); authoring_destroy(&owner)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(test)
test_particle_owned_preview_baseline_and_legacy_load_keep_real_bursts_but_export_authored_settings :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    entity:=ecs.spawn(&owner.world,struct {transform:Scene_Transform,particles:Particle_Emitter}{{km.TRANSFORM_IDENTITY},{particle_defaults()}})
    testing.expect_value(t,particle_burst(&owner.world,entity,32),editor.Scene_Error.None)
    snapshot,capture_error:=scene_snapshot_capture(&owner); testing.expect_value(t,capture_error,editor.Scene_Error.None)
    entry:=owner.registry.entries["ParticleEmitter"]; encoded,valid:=editor.editor_encode_value(entry,ecs.component_address(&owner.world,entity,Particle_Emitter),owner.world.allocator)
    testing.expect(t,valid && !bytes.contains(encoded,transmute([]byte)string("burst_queue"))); delete(encoded)
    drained:=particle_take_bursts(ecs.get_component_mut(&owner.world,entity,Particle_Emitter)); delete(drained)
    testing.expect_value(t,scene_snapshot_restore(&owner,&snapshot),editor.Scene_Error.None); scene_snapshot_destroy(&snapshot)
    restored:=ecs.entity_ids(&owner.world); for id in restored { if emitter:=ecs.get_component_mut(&owner.world,id,Particle_Emitter); emitter!=nil { testing.expect(t,len(emitter.descriptor.burst_queue)==1 && emitter.descriptor.burst_queue[0]==32) } }; delete(restored)
    tree,parse_error:=json.parse(transmute([]byte)string(`{"particle_emitter":{"burst_queue":[7]}}`),spec=.JSON,parse_integers=true); testing.expect(t,parse_error==nil)
    object,_:=tree.(json.Object); external:=Scene_Snapshot{entities=make([dynamic]Scene_Entity),next_entity_id=2,allocator=owner.world.allocator}; row:=Scene_Entity{key=1,components=make([dynamic]Scene_Component)}
    testing.expect_value(t,scene_builtin_components_decode(&owner,&row,object),editor.Scene_Error.None); json.destroy_value(tree); append(&external.entities,row)
    testing.expect_value(t,scene_snapshot_restore(&owner,&external),editor.Scene_Error.None); scene_snapshot_destroy(&external)
    live:=ecs.entity_ids(&owner.world); for id in live { if emitter:=ecs.get_component_mut(&owner.world,id,Particle_Emitter); emitter!=nil { testing.expect(t,len(emitter.descriptor.burst_queue)==1 && emitter.descriptor.burst_queue[0]==7) } }; delete(live)
    exported,export_error:=scene_snapshot_capture(&owner); testing.expect_value(t,export_error,editor.Scene_Error.None)
    fields:=make(json.Object); testing.expect_value(t,scene_builtin_components_encode(&owner,exported.entities[0],&fields),editor.Scene_Error.None)
    descriptor,has_descriptor:=fields["particle_emitter"].(json.Object); testing.expect(t,has_descriptor); _,has_queue:=descriptor["burst_queue"]; testing.expect(t,!has_queue)
    json.destroy_value(fields); scene_snapshot_destroy(&exported)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(private="package")
Particle_Test_Admission :: struct { reject:bool,commits,rollbacks:int }
@(private="package")
particle_test_prepare :: proc(state:rawptr,_:^Authoring,_:[]ecs.Entity_Id,_:Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) { owner:=cast(^Particle_Test_Admission)state; return nil,.Invalid_Operation if owner.reject else .None }
@(private="package")
particle_test_finish :: proc(state,token:rawptr,committed:bool) { owner:=cast(^Particle_Test_Admission)state; if committed { owner.commits+=1 } else { owner.rollbacks+=1 } }

@(test)
test_particle_descriptor_native_admission_rejection_and_behavior_undo_preserve_live_queue :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    entity:=ecs.spawn(&owner.world,struct {transform:Scene_Transform,particles:Particle_Emitter}{{km.TRANSFORM_IDENTITY},{particle_defaults()}})
    native:=Particle_Test_Admission{reject=true}; ecs.insert_resource(&owner.world,Scene_Participant{&native,particle_test_prepare,particle_test_finish})
    testing.expect_value(t,particle_burst(&owner.world,entity,41),editor.Scene_Error.None)
    descriptor:=particle_defaults(); descriptor.emit_rate=77; data:=particle_test_field(descriptor)
    op:=editor.Scene_Op{kind=.Set_Field,entity=entity,component="ParticleEmitter",field="descriptor",value=data}
    result,group:=scene_action_execute(&owner,op); testing.expect_value(t,result.error,editor.Scene_Error.Invalid_Operation); editor.tool_result_destroy(&result); editor.undo_group_destroy(&group)
    live:=ecs.get_component_mut(&owner.world,entity,Particle_Emitter); testing.expect(t,live.descriptor.emit_rate==50 && len(live.descriptor.burst_queue)==1 && live.descriptor.burst_queue[0]==41)
    native.reject=false; result,group=scene_action_execute(&owner,op); testing.expect_value(t,result.error,editor.Scene_Error.None); editor.tool_result_destroy(&result); delete(data)
    accepted:=particle_take_bursts(ecs.get_component_mut(&owner.world,entity,Particle_Emitter)); delete(accepted); testing.expect_value(t,particle_burst(&owner.world,entity,43),editor.Scene_Error.None)
    native.reject=true; testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.Invalid_Operation)
    live=ecs.get_component_mut(&owner.world,entity,Particle_Emitter); testing.expect(t,live.descriptor.emit_rate==77 && len(live.descriptor.burst_queue)==1 && live.descriptor.burst_queue[0]==43)
    native.reject=false; testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.None); editor.undo_group_destroy(&group)
    result,group=behavior_execute(&owner,{action=.Set_Active,entity=entity,active=false}); testing.expect_value(t,result.error,editor.Scene_Error.None); editor.tool_result_destroy(&result)
    testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.None); testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.None)
    live=ecs.get_component_mut(&owner.world,entity,Particle_Emitter); testing.expect(t,!live.descriptor.active && len(live.descriptor.burst_queue)==1 && live.descriptor.burst_queue[0]==43); editor.undo_group_destroy(&group)
    invalid:=particle_defaults(); invalid.base_lifetime=0; data=particle_test_field(invalid); op.value=data
    result,group=scene_action_execute(&owner,op); testing.expect_value(t,result.error,editor.Scene_Error.Invalid_Field_Value); editor.tool_result_destroy(&result); editor.undo_group_destroy(&group); delete(data)
    live=ecs.get_component_mut(&owner.world,entity,Particle_Emitter); testing.expect(t,live.descriptor.base_lifetime==5 && len(live.descriptor.burst_queue)==1 && live.descriptor.burst_queue[0]==43)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
