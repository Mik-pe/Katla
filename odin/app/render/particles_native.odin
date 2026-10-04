//! GPU particle storage and pipelines are prepared before the frame path and retain native ownership.
package render

import app ".."
import gfx "../../gfx"
import shader "../../gfx/shader"
import "core:mem"

@(private="package")
particle_indices_bytes :: proc(indices:[]u32)->[]byte { return mem.slice_to_bytes(indices) }
@(private="package")
particle_counters_bytes :: proc(counters:[]Particle_Counters)->[]byte { return mem.slice_to_bytes(counters) }
/// Creates initialized GPU pools plus separate native-slot upload, command and rollover buffers.
particle_consumer_init :: proc(consumer:^Particle_Consumer($R),owner:^app.Authoring,renderer:^R,operations:Particle_GPU_Ops(R),compiler:^shader.Compiler,format:gfx.Texture_Format,capacity:u32=1_048_576,emitter_capacity:u32=1024,slot_count:int=3,capture_state:=false,allocator:=context.allocator)->(Particle_Error,shader.Error) {
    if owner==nil || renderer==nil || capacity==0 || capacity>1_048_576 || emitter_capacity==0 || emitter_capacity>1024 || slot_count<2 || slot_count>16 { return {code=.Invalid_Configuration},.None }
    if operations.create_compute==nil || operations.destroy_compute==nil || operations.create_graphics==nil || operations.destroy_graphics==nil || operations.create_buffer==nil || operations.destroy_buffer==nil || operations.write_buffer==nil || operations.read_buffer==nil { return {gpu=.Unsupported},.None }
    consumer^={owner=owner,renderer=renderer,operations=operations,capacity=capacity,emitter_capacity=emitter_capacity,capture_state=capture_state,delta_time=1.0/60.0,allocator=allocator}
    success:=false; defer { if !success { particle_consumer_destroy(consumer) } }
    shader_error:shader.Error
    consumer.shaders,shader_error=particle_shader_compile(compiler,format,allocator)
    if shader_error!=.None { return {},shader_error }
    for &pipeline,i in consumer.pipelines {
        error:gfx.Gpu_Error
        pipeline,error=operations.create_compute(renderer,consumer.shaders.compute[i].descriptor); if error!=.None { return {gpu=error},.None }
    }
    error:gfx.Gpu_Error
    consumer.pipeline,error=operations.create_graphics(renderer,consumer.shaders.graphics.descriptor); if error!=.None { return {gpu=error},.None }
    consumer.reverse_pipeline,error=operations.create_graphics(renderer,depth_descriptor(consumer.shaders.graphics.descriptor,.Reverse)); if error!=.None { return {gpu=error},.None }
    consumer.records=make([dynamic]Particle_Record,allocator); consumer.inputs=make([dynamic]gfx.Buffer_Input,allocator)
    consumer.slots=make([]Particle_Slot,slot_count,allocator)
    storage:=gfx.Buffer_Usages{.Storage,.Transfer_Source,.Transfer_Destination}
    data:=make([]byte,int(capacity)*64,allocator); defer delete(data,allocator)
    indices:=make([]u32,int(capacity),allocator); defer delete(indices,allocator)
    for &index,i in indices { index=u32(i) }
    consumer.data,error=operations.create_buffer(renderer,{size=u64(len(data)),usage=storage,memory=.GPU_Private},data); if error!=.None { return {gpu=error},.None }
    consumer.dead,error=operations.create_buffer(renderer,{size=u64(capacity)*4,usage=storage,memory=.GPU_Private},particle_indices_bytes(indices)); if error!=.None { return {gpu=error},.None }
    for &index in indices { index=0 }
    consumer.rollover_alive,error=operations.create_buffer(renderer,{size=u64(capacity)*4,usage=storage,memory=.GPU_Private},particle_indices_bytes(indices)); if error!=.None { return {gpu=error},.None }
    initial:=[1]Particle_Counters{{dead=capacity}}
    consumer.rollover_counters,error=operations.create_buffer(renderer,{size=16,usage=storage,memory=.GPU_Private},particle_counters_bytes(initial[:])); if error!=.None { return {gpu=error},.None }
    for &slot in consumer.slots {
        handles:=[10]^gfx.Buffer_Handle{&slot.alive,&slot.working,&slot.counters,&slot.indirect,&slot.dispatch,&slot.frame,&slot.configs,&slot.indices,&slot.camera,&slot.readback}
        sizes:=[10]u64{u64(capacity)*4,u64(capacity)*4,16,16,12,32,u64(emitter_capacity)*160,u64(capacity)*4,112,particle_readback_size(consumer)}
        for handle,i in handles {
            usage:=storage; domain:=gfx.Memory_Domain.GPU_Private
            if i==3 || i==4 { usage|={.Indirect} }
            if i==5 || i==8 { usage={.Uniform}; domain=.CPU_Visible }
            if i==6 || i==7 { usage={.Storage}; domain=.CPU_Visible }
            if i==9 { usage={.Readback,.Transfer_Destination}; domain=.CPU_Visible }
            zero:=make([]byte,int(sizes[i]),allocator)
            handle^,error=operations.create_buffer(renderer,{size=sizes[i],usage=usage,memory=domain},zero); delete(zero,allocator)
            if error!=.None { return {gpu=error},.None }
        }
    }
    success=true; return {},.None
}
@(private="package")
particle_readback_size :: proc(consumer:^Particle_Consumer($R))->u64 { return 32+u64(consumer.capacity)*68 if consumer.capture_state else 16 }
/// Releases public handles after detaching composition; accepted GPU parents remain natively retained.
particle_consumer_destroy :: proc(consumer:^Particle_Consumer($R))->gfx.Gpu_Error {
    particle_abort(consumer)
    error:=gfx.Gpu_Error.None
    if consumer.renderer!=nil {
        for slot in consumer.slots {
            for handle in ([10]gfx.Buffer_Handle{slot.alive,slot.working,slot.counters,slot.indirect,slot.dispatch,slot.frame,slot.configs,slot.indices,slot.camera,slot.readback}) {
                if handle.owner!=nil { result:=consumer.operations.destroy_buffer(consumer.renderer,handle); if result!=.None { error=result } }
            }
        }
        for handle in ([4]gfx.Buffer_Handle{consumer.data,consumer.dead,consumer.rollover_alive,consumer.rollover_counters}) {
            if handle.owner!=nil { result:=consumer.operations.destroy_buffer(consumer.renderer,handle); if result!=.None { error=result } }
        }
        for pipeline in consumer.pipelines {
            if pipeline.owner!=nil { result:=consumer.operations.destroy_compute(consumer.renderer,pipeline); if result!=.None { error=result } }
        }
        if consumer.reverse_pipeline.owner!=nil { result:=consumer.operations.destroy_graphics(consumer.renderer,consumer.reverse_pipeline); if result!=.None { error=result } }
        if consumer.pipeline.owner!=nil { result:=consumer.operations.destroy_graphics(consumer.renderer,consumer.pipeline); if result!=.None { error=result } }
    }
    particle_shader_destroy(&consumer.shaders)
    delete(consumer.states,consumer.allocator); delete(consumer.slots,consumer.allocator); delete(consumer.records); delete(consumer.inputs); consumer^={}
    return error
}
