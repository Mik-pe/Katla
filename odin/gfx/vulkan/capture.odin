//! Passive native observations are recorded where Vulkan accepts or encodes their operation.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

/// Enables optional submission-thread diagnostics without changing native execution.
capture_enable :: proc(r:^Renderer,enabled:bool)->gfx.Gpu_Error {
    if r.device==nil || r.failed { return .Invalid_Resource }
    if r.capture.recording { return .Busy }
    r.capture.enabled=enabled; return .None
}
/// Returns an independently owned recording of an accepted submission, or the latest.
capture_snapshot :: proc(r:^Renderer,submission:u64=0,allocator:=context.allocator)->(gfx.Capture_Snapshot,bool) { return gfx.capture_snapshot(&r.capture,submission,allocator) }

@(private="package")
Capture_Handle_Key :: struct { kind:u32,handle:u64 }
@(private="package")
capture_handle :: proc(r:^Renderer,kind:u32,handle:u64)->u64 {
    if !r.capture.recording || handle==0 { return 0 }
    key:=Capture_Handle_Key{kind,handle}
    identity,found:=r.capture_handles[key]
    if !found { identity=new(byte,r.allocator);r.capture_handles[key]=identity }
    return gfx.capture_object(&r.capture,identity)
}
@(private="package")
capture_clear_handles :: proc(r:^Renderer) { for _,identity in r.capture_handles { free(identity,r.allocator) };clear(&r.capture_handles);r.capture_prepared=nil;r.capture_pass=-1;r.capture_phase=-1;r.capture_encoder=0 }
@(private="package")
capture_image_id :: proc(r:^Renderer,texture:^Native_Texture)->int {
    if !r.capture.recording { return -1 }
    if r.capture_prepared!=nil&&r.capture_pass>=0 { for pass in r.capture.candidate.passes { if pass.index!=r.capture_pass { continue };for access in pass.images { for input in r.capture_prepared.textures { if input.resource.index!=access.resource_index { continue };if entry,ok:=gfx.storage_get(&r.textures,input.handle);ok&&entry^==texture { return access.resource_index } } } } }
    if r.capture_prepared!=nil { for input in r.capture_prepared.textures { entry,ok:=gfx.storage_get(&r.textures,input.handle);if ok&&entry^==texture { return input.resource.index } } }
    return -1
}
@(private="package")
capture_buffer_id :: proc(r:^Renderer,buffer:^Native_Buffer)->int {
    if !r.capture.recording { return -1 }
    if r.capture_prepared!=nil&&r.capture_pass>=0 { for pass in r.capture.candidate.passes { if pass.index!=r.capture_pass { continue };for access in pass.buffers { for input in r.capture_prepared.buffers { if input.resource.index!=access.resource_index { continue };if entry,ok:=gfx.storage_get(&r.buffers,input.handle);ok&&entry^==buffer { return access.resource_index } } } } }
    if r.capture_prepared!=nil { for input in r.capture_prepared.buffers { entry,ok:=gfx.storage_get(&r.buffers,input.handle);if ok&&entry^==buffer { return input.resource.index } } }
    return -1
}
@(private="package")
capture_buffer_binding :: proc(r:^Renderer,buffer:^Native_Buffer,range:gfx.Buffer_Range,group,binding:u32,table:vk.DescriptorSet=0,label:string="buffer",stages:u64=0,path:gfx.Capture_Binding_Path=.Descriptor) {
    if !r.capture.recording { return }
    index:=capture_buffer_id(r,buffer)
    gfx.capture_record(&r.capture,{kind=.Bind_Buffer,pass_index=r.capture_pass,phase_index=r.capture_phase,resource_kind=.Buffer if index>=0 else .Auxiliary,resource_index=index,object=capture_handle(r,5,u64(buffer.object)),encoder=r.capture_encoder,heap=gfx.capture_object(&r.capture,buffer.heap),table=capture_handle(r,1,u64(table)),group=group,binding=binding,buffer_range=range,offset=range.offset,size=range.size,binding_stages=stages,binding_path=path,emitted=true,label=label})
}
@(private="package")
capture_pipeline_binding :: proc(r:^Renderer,pipeline:vk.Pipeline,layout:vk.PipelineLayout,bind_point:vk.PipelineBindPoint,label:string) {
    if !r.capture.recording { return }
    gfx.capture_record(&r.capture,{kind=.Bind_Pipeline,pass_index=r.capture_pass,phase_index=r.capture_phase,encoder=r.capture_encoder,pipeline=capture_handle(r,3,u64(pipeline)),layout=capture_handle(r,2,u64(layout)),native_index=u32(bind_point),resource_index=-1,emitted=true,label=label})
}
@(private="package")
capture_descriptor_tables :: proc(r:^Renderer,sets:[]vk.DescriptorSet,layout:vk.PipelineLayout) {
    if !r.capture.recording { return }
    for set,group in sets { gfx.capture_record(&r.capture,{kind=.Argument_Table,pass_index=r.capture_pass,phase_index=r.capture_phase,encoder=r.capture_encoder,object=capture_handle(r,1,u64(set)),table=capture_handle(r,1,u64(set)),layout=capture_handle(r,2,u64(layout)),group=u32(group),resource_index=-1,emitted=true,label="vkCmdBindDescriptorSets"}) }
}
@(private="package")
capture_physical_resources :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph) {
    if !r.capture.recording { return }
    for input in prepared.buffers {
        buffer,ok:=resolve_buffer(r,prepared,input.resource);if !ok { continue }
        requirements,_:=buffer_requirements(r,buffer.object)
        gfx.capture_record(&r.capture,{kind=.Allocation,pass_index=-1,phase_index=-1,resource_kind=.Buffer,resource_index=input.resource.index,object=capture_handle(r,5,u64(buffer.object)),heap=gfx.capture_object(&r.capture,buffer.heap),size=buffer.heap.size,alignment=u64(requirements.alignment),memory_type=buffer.heap.memory_type,memory_flags=u64(transmute(u32)r.memory_properties.memoryTypes[buffer.heap.memory_type].propertyFlags),emitted=true,label="native buffer allocation",reason="retained allocation; actual Vulkan requirements queried"})
    }
    for input in prepared.textures {
        texture,ok:=resolve_texture(r,prepared,input.resource);if !ok { continue }
        event:=gfx.Capture_Event{kind=.Allocation,pass_index=-1,phase_index=-1,resource_kind=.Image,resource_index=input.resource.index,object=capture_handle(r,4,u64(texture.allocation.object)),emitted=true,label="native image allocation"}
        if heap:=texture.allocation.heap;heap!=nil {
            requirements,_:=image_requirements(r,texture.allocation.object)
            event.heap=gfx.capture_object(&r.capture,heap);event.size=heap.size;event.alignment=u64(requirements.alignment);event.memory_type=heap.memory_type;event.memory_flags=u64(transmute(u32)r.memory_properties.memoryTypes[heap.memory_type].propertyFlags);event.reason="retained allocation; actual Vulkan requirements queried"
        } else { event.reason="swapchain owns allocation; memory requirements unavailable" }
        gfx.capture_record(&r.capture,event)
    }
}
@(private="package")
capture_buffer_expectations :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph) {
    if !r.capture.recording { return }
    for hazard in prepared.hazards {
        source_stage:=vk.PipelineStageFlags2{.ALL_COMMANDS};destination_stage:=vk.PipelineStageFlags2{.ALL_COMMANDS}
        for pass in prepared.passes { if pass.id==hazard.before&&pass.kind==.Transfer { source_stage={.ALL_TRANSFER} };if pass.id==hazard.after&&pass.kind==.Transfer { destination_stage={.ALL_TRANSFER} } }
        start:=max(hazard.source.range.offset,hazard.destination.range.offset);end:=min(hazard.source.range.offset+hazard.source.range.size,hazard.destination.range.offset+hazard.destination.range.size)
        gfx.capture_expect(&r.capture,{kind=.Buffer_Barrier,pass_index=hazard.after.index,phase_index=-1,resource_kind=.Buffer,resource_index=hazard.resource.index,buffer_range={start,end-start},source_stages=transmute(u64)source_stage,destination_stages=transmute(u64)destination_stage,source_access=transmute(u64)access_mask(hazard.source),destination_access=transmute(u64)access_mask(hazard.destination),emitted=true,label="compiled buffer dependency"})
    }
}
@(private="package")
capture_memory_barrier :: proc(r:^Renderer,barrier:vk.MemoryBarrier2,label:string,kind:gfx.Capture_Event_Kind=.Global_Barrier) {
    if !r.capture.recording { return }
    gfx.capture_record(&r.capture,{kind=kind,pass_index=r.capture_pass,phase_index=r.capture_phase,resource_index=-1,encoder=r.capture_encoder,source_stages=transmute(u64)barrier.srcStageMask,destination_stages=transmute(u64)barrier.dstStageMask,source_access=transmute(u64)barrier.srcAccessMask,destination_access=transmute(u64)barrier.dstAccessMask,emitted=true,label=label,reason="backend ownership/visibility barrier"})
}

@(private="package")
capture_buffer_binding_expect :: proc(r:^Renderer,range:gfx.Buffer_Range,index:int,group,binding:u32,stages:u64=0,path:gfx.Capture_Binding_Path=.Descriptor) {
    if !r.capture.recording { return }
    gfx.capture_expect(&r.capture,{kind=.Bind_Buffer,pass_index=r.capture_pass,phase_index=r.capture_phase,resource_kind=.Buffer if index>=0 else .Auxiliary,resource_index=index,group=group,binding=binding,buffer_range=range,offset=range.offset,size=range.size,binding_stages=stages,binding_path=path,emitted=true,label="translated buffer binding"})
}
@(private="package")
capture_global_expect :: proc(r:^Renderer,source_stages:vk.PipelineStageFlags2,source_access:vk.AccessFlags2,destination_stages:vk.PipelineStageFlags2,destination_access:vk.AccessFlags2) {
    if !r.capture.recording { return }
    gfx.capture_expect(&r.capture,{kind=.Global_Barrier,pass_index=r.capture_pass,phase_index=r.capture_phase,resource_index=-1,source_stages=transmute(u64)source_stages,source_access=transmute(u64)source_access,destination_stages=transmute(u64)destination_stages,destination_access=transmute(u64)destination_access,emitted=true,label="translated backend ownership visibility"})
}

@(private="package")
capture_pipeline_expect :: proc(r:^Renderer,pipeline:vk.Pipeline,layout:vk.PipelineLayout,bind_point:vk.PipelineBindPoint) {
    if !r.capture.recording { return }
    gfx.capture_expect(&r.capture,{kind=.Bind_Pipeline,pass_index=r.capture_pass,phase_index=r.capture_phase,pipeline=capture_handle(r,3,u64(pipeline)),layout=capture_handle(r,2,u64(layout)),native_index=u32(bind_point),resource_index=-1,emitted=true,label="immutable selected pipeline"})
}
