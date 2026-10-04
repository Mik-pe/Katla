#+test
package render
import app ".."
import ecs "../../ecs"
import gfx "../../gfx"
import km "../../math"
import shader "../../gfx/shader"
import "core:testing"

Particle_Test_GPU :: struct { creates:int }
particle_test_compute :: proc(r:^Particle_Test_GPU,_:gfx.Compute_Desc)->(gfx.Pipeline_Handle,gfx.Gpu_Error) { r.creates+=1; return {},.No_Device }
particle_test_destroy_compute :: proc(_:^Particle_Test_GPU,_:gfx.Pipeline_Handle)->gfx.Gpu_Error { return .Invalid_Resource }
particle_test_graphics :: proc(r:^Particle_Test_GPU,_:gfx.Graphics_Desc)->(gfx.Graphics_Pipeline_Handle,gfx.Gpu_Error) { r.creates+=1; return {},.No_Device }
particle_test_destroy_graphics :: proc(_:^Particle_Test_GPU,_:gfx.Graphics_Pipeline_Handle)->gfx.Gpu_Error { return .Invalid_Resource }
particle_test_buffer :: proc(r:^Particle_Test_GPU,_:gfx.Buffer_Desc,_:[]byte)->(gfx.Buffer_Handle,gfx.Gpu_Error) { r.creates+=1; return {},.No_Device }
particle_test_destroy_buffer :: proc(_:^Particle_Test_GPU,_:gfx.Buffer_Handle)->gfx.Gpu_Error { return .Invalid_Resource }
particle_test_write :: proc(_:^Particle_Test_GPU,_:gfx.Frame_Token,_:gfx.Buffer_Handle,_:u64,_:[]byte)->gfx.Gpu_Error { return .No_Device }
particle_test_read :: proc(_:^Particle_Test_GPU,_:gfx.Buffer_Handle,_:u64,_:[]byte)->gfx.Gpu_Error { return .Busy }

@(test)
test_particle_admission_preserves_whole_queue_and_exact_emitter_requests :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner); app.particle_register(&owner.world,&owner.registry)
    descriptor:=app.particle_defaults(); descriptor.emit_rate=0
    first:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform(position={1,2,3})},{descriptor}})
    second:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform(position={4,5,6})},{descriptor}})
    app.particle_burst(&owner.world,first,32); app.particle_burst(&owner.world,second,2)
    rejected,error:=particle_plan(&owner,nil,33,4,0); defer particle_preparation_destroy(&rejected,context.allocator)
    testing.expect_value(t,error,Particle_Error{}); testing.expect(t,rejected.deferred && rejected.requested==32 && len(rejected.bursts)==1)
    testing.expect_value(t,len(ecs.get_component_mut(&owner.world,first,app.Particle_Emitter).descriptor.burst_queue),1)
    accepted,accepted_error:=particle_plan(&owner,nil,34,4,0); defer particle_preparation_destroy(&accepted,context.allocator)
    testing.expect_value(t,accepted_error,Particle_Error{}); testing.expect_value(t,accepted.requested,u32(34))
    for index in accepted.indices[:32] { testing.expect_value(t,index,u32(0)) }
    for index in accepted.indices[32:] { testing.expect_value(t,index,u32(1)) }
    testing.expect_value(t,accepted.states[0].config.position,([3]f32{1,2,3}))
    testing.expect_value(t,accepted.states[1].config.position,([3]f32{4,5,6}))
}
@(test)
test_particle_closed_compiler_and_rejected_frame_publish_no_gpu_or_queue_state :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner); app.particle_register(&owner.world,&owner.registry)
    descriptor:=app.particle_defaults(); descriptor.emit_rate=0
    entity:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform()},{descriptor}})
    app.particle_burst(&owner.world,entity,32)
    gpu:Particle_Test_GPU; compiler:shader.Compiler; consumer:Particle_Consumer(Particle_Test_GPU)
    operations:=Particle_GPU_Ops(Particle_Test_GPU){particle_test_compute,particle_test_destroy_compute,particle_test_graphics,particle_test_destroy_graphics,particle_test_buffer,particle_test_destroy_buffer,particle_test_write,particle_test_read}
    error,compile_error:=particle_consumer_init(&consumer,&owner,&gpu,operations,&compiler,.RGBA8_Unorm,64,4,3)
    testing.expect_value(t,error,Particle_Error{}); testing.expect_value(t,compile_error,shader.Error.Closed); testing.expect_value(t,gpu.creates,0)
    testing.expect(t,consumer.owner==nil && consumer.renderer==nil)
    inputs,frame_error:=particle_prepare(&consumer,nil,{},Frame_Data{},0)
    testing.expect_value(t,frame_error.code,Particle_Error_Code.Invalid_Frame); testing.expect_value(t,len(inputs),0)
    testing.expect_value(t,len(ecs.get_component_mut(&owner.world,entity,app.Particle_Emitter).descriptor.burst_queue),1)
}

@(test)
test_particle_accepted_publication_consumes_only_snapshot_and_advances_rollover :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner); app.particle_register(&owner.world,&owner.registry)
    descriptor:=app.particle_defaults(); descriptor.emit_rate=0; descriptor.has_timed_emission=true; descriptor.timed_emission=0.5
    entity:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform()},{descriptor}})
    app.particle_burst(&owner.world,entity,32)
    prepared,error:=particle_plan(&owner,nil,64,4,0.25); testing.expect_value(t,error,Particle_Error{})
    frames:gfx.Frames; gfx.frames_init(&frames,3); defer gfx.frames_destroy(&frames)
    token,acquire_error:=gfx.frame_acquire(&frames,0); testing.expect_value(t,acquire_error,gfx.Frame_Error.None)
    prepared.token=token; prepared.ready=true
    consumer:=Particle_Consumer(Particle_Test_GPU){owner=&owner,pending=prepared,capacity=64,allocator=context.allocator,slots=make([]Particle_Slot,3),records=make([dynamic]Particle_Record)}
    defer { particle_abort(&consumer); delete(consumer.states); delete(consumer.slots); delete(consumer.records) }
    testing.expect_value(t,particle_frame_delta(&consumer,0.5).code,Particle_Error_Code.Invalid_Configuration)
    _=particle_composition(&consumer)
    app.particle_burst(&owner.world,entity,2)
    testing.expect_value(t,gfx.frame_recorded(&frames,token),gfx.Frame_Error.None)
    sequence,submit_error:=gfx.frame_submitted(&frames,token); testing.expect_value(t,submit_error,gfx.Frame_Error.None)
    testing.expect_value(t,particle_committed(&consumer,gfx.Submission{owner=&frames,token=token,id=sequence}),Particle_Error{})
    testing.expect(t,consumer.sequence==1 && consumer.previous_slot==0 && consumer.alive_upper==32)
    emitter:=ecs.get_component_mut(&owner.world,entity,app.Particle_Emitter)
    testing.expect_value(t,len(emitter.descriptor.burst_queue),1); testing.expect_value(t,emitter.descriptor.burst_queue[0],u32(2))
    testing.expect_value(t,emitter.descriptor.timed_emission,f32(0.5))
    testing.expect_value(t,consumer.states[0].remaining_duration,f32(0.25))
    testing.expect_value(t,gfx.frame_completed(&frames,token,sequence),gfx.Frame_Error.None)
}

@(test)
test_particle_burst_prefix_is_atomic_and_impossible_bursts_keep_owned_queue :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner); app.particle_register(&owner.world,&owner.registry)
    descriptor:=app.particle_defaults(); descriptor.emit_rate=10; descriptor.has_timed_emission=true; descriptor.timed_emission=1
    entity:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform()},{descriptor}})
    app.particle_burst(&owner.world,entity,40); app.particle_burst(&owner.world,entity,40)
    previous:=[1]Particle_Emitter_State{{entity=entity,accumulator=0.75,active=true,clock_initialized=true,configured_active=true,configured_timed=true,configured_duration=1,remaining_duration=1}}
    prepared,error:=particle_plan(&owner,previous[:],64,4,0.5,pool_capacity=64)
    defer particle_preparation_destroy(&prepared,context.allocator)
    testing.expect_value(t,error,Particle_Error{})
    testing.expect(t,prepared.deferred && prepared.requested==40 && prepared.burst_count==40 && len(prepared.bursts)==1 && len(prepared.bursts[0].counts)==1)
    testing.expect_value(t,prepared.states[0].accumulator,f64(0.75))
    emitter:=ecs.get_component_mut(&owner.world,entity,app.Particle_Emitter)
    testing.expect_value(t,len(emitter.descriptor.burst_queue),2)
    testing.expect_value(t,emitter.descriptor.timed_emission,f32(1))
    app.particle_burst(&owner.world,entity,65)
    rejected,reject_error:=particle_plan(&owner,previous[:],64,4,0.5,pool_capacity=64)
    defer particle_preparation_destroy(&rejected,context.allocator)
    testing.expect_value(t,reject_error.code,Particle_Error_Code.Particle_Capacity)
    testing.expect_value(t,len(emitter.descriptor.burst_queue),3)
}

@(test)
test_particle_emitter_indices_reclaim_only_after_empty_gpu_population :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner); app.particle_register(&owner.world,&owner.registry)
    descriptor:=app.particle_defaults(); descriptor.emit_rate=0
    first:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform()},{descriptor}})
    previous:=[1]Particle_Emitter_State{{entity=first,config=particle_config(descriptor,{}),active=true}}
    testing.expect(t,ecs.destroy_entity(&owner.world,first))
    second:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform()},{descriptor}})
    app.particle_burst(&owner.world,second,1)
    retained,error:=particle_plan(&owner,previous[:],63,1,0.25)
    defer particle_preparation_destroy(&retained,context.allocator)
    testing.expect_value(t,error.code,Particle_Error_Code.Emitter_Capacity)
    reclaimed,reclaim_error:=particle_plan(&owner,previous[:],64,1,0.25,reclaim=true)
    defer particle_preparation_destroy(&reclaimed,context.allocator)
    testing.expect_value(t,reclaim_error,Particle_Error{})
    testing.expect(t,len(reclaimed.states)==1 && reclaimed.states[0].entity==second && reclaimed.indices[0]==0)
}

@(test)
test_particle_scene_validation_counts_retired_indices_before_publication :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner); app.particle_register(&owner.world,&owner.registry)
    descriptor:=app.particle_defaults(); descriptor.emit_rate=0
    first:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform()},{descriptor}})
    second:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,particles:app.Particle_Emitter}{{km.transform()},{descriptor}})
    app.particle_burst(&owner.world,second,32)
    states:=[1]Particle_Emitter_State{{entity=first}}
    consumer:=Particle_Consumer(Particle_Test_GPU){owner=&owner,states=states[:],capacity=64,emitter_capacity=1,alive_upper=1}
    testing.expect_value(t,particle_scene_validate(&consumer,&owner,{second}).code,Particle_Error_Code.Emitter_Capacity)
    consumer.alive_upper=0
    testing.expect_value(t,particle_scene_validate(&consumer,&owner,{second}),Particle_Error{})
    testing.expect_value(t,particle_scene_validate(&consumer,&owner,{first,second}).code,Particle_Error_Code.Emitter_Capacity)
    testing.expect_value(t,len(ecs.get_component_mut(&owner.world,second,app.Particle_Emitter).descriptor.burst_queue),1)
    testing.expect_value(t,states[0].entity,first)
}
