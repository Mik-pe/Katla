#+test
package render
import app ".."
import ecs "../../ecs"
import gfx "../../gfx"
import km "../../math"
import editor "../../editor"
import "core:testing"
import "core:mem"
import "core:encoding/json"

@(private="package")
particle_clock_accept :: proc(consumer:^Particle_Consumer(Particle_Test_GPU),prepared:Particle_Preparation)->Particle_Error {
    consumer.pending=prepared; consumer.pending.ready=true; return particle_committed(consumer,gfx.Submission{})
}

@(test)
test_particle_timer_is_accepted_renderer_state_and_authored_edits_keep_live_clock :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:app.Authoring; app.authoring_init(&owner); testing.expect_value(t,app.authoring_services_init(&owner),editor.Scene_Error.None)
    descriptor:=app.particle_defaults(); descriptor.emit_rate=0; descriptor.has_timed_emission=true; descriptor.timed_emission=.5
    entity:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform()},{descriptor}})
    consumer:=Particle_Consumer(Particle_Test_GPU){owner=&owner,allocator=context.allocator,capacity=64,slots=make([]Particle_Slot,1),records=make([dynamic]Particle_Record)}
    planned,error:=particle_plan(&owner,nil,64,4,.25); testing.expect_value(t,error,Particle_Error{})
    consumer.pending=planned; particle_abort(&consumer); testing.expect(t,len(consumer.states)==0)
    planned,error=particle_plan(&owner,nil,64,4,.25); testing.expect_value(t,error,Particle_Error{}); testing.expect_value(t,particle_clock_accept(&consumer,planned),Particle_Error{})
    testing.expect_value(t,consumer.states[0].remaining_duration,f32(.25))
    live:=ecs.get_component_mut(&owner.world,entity,app.Particle_Emitter); testing.expect(t,live.descriptor.timed_emission==.5 && live.descriptor.active && live.descriptor.has_timed_emission)
    gesture:app.Scene_Gesture; testing.expect_value(t,app.scene_gesture_begin(&owner,&gesture,{entity}),editor.Scene_Error.None)
    descriptor=live.descriptor; descriptor.emit_rate=1; data,encode_error:=json.marshal(descriptor); testing.expect(t,encode_error==nil)
    testing.expect_value(t,app.scene_gesture_preview(&owner,&gesture,{kind=.Set_Field,component="ParticleEmitter",field="descriptor",value=data}),editor.Scene_Error.None); delete(data)
    planned,error=particle_plan(&owner,consumer.states,64,4,.25); testing.expect_value(t,error,Particle_Error{}); testing.expect_value(t,particle_clock_accept(&consumer,planned),Particle_Error{})
    testing.expect(t,!consumer.states[0].active && consumer.states[0].remaining_duration==0)
    testing.expect_value(t,app.scene_gesture_finish(&owner,&gesture),editor.Scene_Error.None)
    testing.expect_value(t,app.authoring_undo_last(&owner),editor.Scene_Error.None); testing.expect_value(t,app.authoring_redo_last(&owner),editor.Scene_Error.None)
    planned,error=particle_plan(&owner,consumer.states,64,4,.25); testing.expect_value(t,error,Particle_Error{}); testing.expect(t,!planned.states[0].active && planned.requested==0); particle_preparation_destroy(&planned,context.allocator)
    testing.expect_value(t,app.particle_burst(&owner.world,entity,3),editor.Scene_Error.None)
    planned,error=particle_plan(&owner,consumer.states,64,4,.25); testing.expect_value(t,error,Particle_Error{}); testing.expect_value(t,planned.requested,u32(3)); testing.expect_value(t,particle_clock_accept(&consumer,planned),Particle_Error{})
    testing.expect_value(t,consumer.states[0].remaining_duration,f32(0))
    testing.expect_value(t,app.particle_restart(&owner.world,entity),editor.Scene_Error.None)
    planned,error=particle_plan(&owner,consumer.states,64,4,.125); testing.expect_value(t,error,Particle_Error{}); testing.expect_value(t,particle_clock_accept(&consumer,planned),Particle_Error{})
    testing.expect_value(t,consumer.states[0].remaining_duration,f32(.375))
    live=ecs.get_component_mut(&owner.world,entity,app.Particle_Emitter); live.descriptor.timed_emission=1
    planned,error=particle_plan(&owner,consumer.states,64,4,.25); testing.expect_value(t,error,Particle_Error{}); testing.expect_value(t,particle_clock_accept(&consumer,planned),Particle_Error{})
    testing.expect_value(t,consumer.states[0].remaining_duration,f32(.75))
    particle_abort(&consumer); delete(consumer.states); delete(consumer.slots); delete(consumer.records); app.authoring_destroy(&owner)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(test)
test_particle_capacity_backpressure_freezes_timer_until_accepted_emission :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); app.scene_components_register(&owner); app.particle_register(&owner.world,&owner.registry)
    descriptor:=app.particle_defaults(); descriptor.emit_rate=0; descriptor.has_timed_emission=true; descriptor.timed_emission=1
    entity:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform()},{descriptor}})
    previous:=[1]Particle_Emitter_State{{entity=entity,clock_initialized=true,configured_active=true,configured_timed=true,configured_duration=1,remaining_duration=.75,active=true}}
    testing.expect_value(t,app.particle_burst(&owner.world,entity,32),editor.Scene_Error.None)
    prepared,error:=particle_plan(&owner,previous[:],0,4,.25); testing.expect_value(t,error,Particle_Error{}); testing.expect(t,prepared.deferred && prepared.requested==0)
    consumer:=Particle_Consumer(Particle_Test_GPU){owner=&owner,allocator=context.allocator,capacity=64,slots=make([]Particle_Slot,1),records=make([dynamic]Particle_Record)}
    testing.expect_value(t,particle_clock_accept(&consumer,prepared),Particle_Error{}); testing.expect_value(t,consumer.states[0].remaining_duration,f32(.75))
    prepared,error=particle_plan(&owner,consumer.states,64,4,.25); testing.expect_value(t,error,Particle_Error{}); testing.expect_value(t,particle_clock_accept(&consumer,prepared),Particle_Error{}); testing.expect_value(t,consumer.states[0].remaining_duration,f32(.5))
    emitter:=ecs.get_component_mut(&owner.world,entity,app.Particle_Emitter); testing.expect(t,len(emitter.descriptor.burst_queue)==0 && emitter.descriptor.timed_emission==1)
    particle_abort(&consumer); delete(consumer.states); delete(consumer.slots); delete(consumer.records)
}
