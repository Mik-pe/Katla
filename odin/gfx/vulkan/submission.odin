//! Synchronization2 submission retains native resources through its exact fence.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"
import "core:log"

@(private="package")
clear_frame :: proc(r:^Renderer,slot:^Native_Frame,submitted:bool) {
    for buffer in slot.buffers { if submitted { buffer.pending-=1; buffer.heap.pending-=1 }; release_buffer(r,buffer) }
    for pipeline in slot.pipelines { release_pipeline(r,pipeline) }
    for texture in slot.textures { if submitted { texture.pending-=1; if texture.allocation.heap!=nil { texture.allocation.heap.pending-=1 } }; release_texture(r,texture) }
    for pipeline in slot.graphics { release_graphics_pipeline(r,pipeline) }
    for sampler in slot.samplers { release_sampler(r,sampler) }
    clear(&slot.textures); clear(&slot.graphics); clear(&slot.samplers); clear(&slot.buffers); clear(&slot.pipelines); slot.submission=0; slot.accepted=false
}
@(private="package")
retire_slot :: proc(r:^Renderer,slot:^Native_Frame,failed:bool)->gfx.Gpu_Error {
    err:=gfx.frame_completed(&r.frames,slot.token,slot.submission)
    clear_frame(r,slot,true)
    if failed || err!=.None { r.failed=true; return .Native_Failure }
    return .None
}
@(private="package")
submission_slot :: proc(r:^Renderer,submission:gfx.Submission)->(^Native_Frame,bool) {
    if submission.owner!=r || submission.token.owner!=&r.frames || submission.token.slot<0 || submission.token.slot>=len(r.slots) { return nil,false }
    slot:=&r.slots[submission.token.slot]
    return slot,slot.submission!=0 && slot.submission==submission.id && slot.token==submission.token
}
/// Polls only the exact accepted fence and retires its retained objects once terminal.
poll :: proc(r:^Renderer,submission:gfx.Submission)->(bool,gfx.Gpu_Error) {
    slot,ok:=submission_slot(r,submission); if !ok { return false,.Invalid_Resource }
    result:=r.table.GetFenceStatus(r.device,slot.fence)
    if result==.NOT_READY { return false,.None }
    if result!=.SUCCESS && result!=.ERROR_DEVICE_LOST { return false,.Native_Failure }
    return true,retire_slot(r,slot,result==.ERROR_DEVICE_LOST)
}
/// A nonterminal CPU wait failure retains GPU owners for a later retry or successful drain.
wait :: proc(r:^Renderer,submission:gfx.Submission)->gfx.Gpu_Error {
    slot,ok:=submission_slot(r,submission); if !ok { return .Invalid_Resource }
    result:=r.table.WaitForFences(r.device,1,&slot.fence,true,max(u64))
    if result!=.SUCCESS && result!=.ERROR_DEVICE_LOST { log.error("Vulkan fence wait failed",result); return .Native_Failure }
    return retire_slot(r,slot,result==.ERROR_DEVICE_LOST)
}
@(private="package")
retain_pipeline :: proc(slot:^Native_Frame,pipeline:^Native_Pipeline) {
    for previous in slot.pipelines { if previous==pipeline { return } }
    pipeline.refs+=1; append(&slot.pipelines,pipeline)
}
@(private="package")
resolve_buffer :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph,id:gfx.Resource_Id)->(^Native_Buffer,bool) {
    handle,found:=gfx.prepared_buffer(prepared,id); if !found { return nil,false }
    entry,ok:=gfx.storage_get(&r.buffers,handle); if !ok { return nil,false }
    return entry^,true
}
@(private="package")
access_mask :: proc(access:gfx.Buffer_Access)->vk.AccessFlags2 {
    mask:vk.AccessFlags2
    if access.usage==.Vertex { return {.VERTEX_ATTRIBUTE_READ} }
    if access.usage==.Index { return {.INDEX_READ} }
    if access.usage==.Indirect { return {.INDIRECT_COMMAND_READ} }
    if access.usage==.Transfer_Source { return {.TRANSFER_READ} }
    if access.usage==.Transfer_Destination { return {.TRANSFER_WRITE} }
    if access.usage==.Uniform { return {.UNIFORM_READ} }
    if gfx.access_reads(access.mode) { mask|={.SHADER_STORAGE_READ} }
    if gfx.access_writes(access.mode) { mask|={.SHADER_STORAGE_WRITE} }
    return mask
}
/// Records validated packets and accepts them through one Vulkan synchronization2 entry point.
submit :: proc(r:^Renderer,token:gfx.Frame_Token,g:^gfx.Graph,plan:^gfx.Compiled_Graph,inputs:[]gfx.Buffer_Input,textures:[]gfx.Texture_Input=nil)->(gfx.Submission,gfx.Gpu_Error,gfx.Packet_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure,.None }
    if !valid_acquisition(r,token) { return {},.Invalid_Resource,.None }
    prepared,preflight:=gfx.graph_prepare(g,plan,inputs,resource_query(r),textures,graphics_query(r))
    if preflight!=.None { return {},.Invalid_Graph,preflight }; defer gfx.prepared_graph_destroy(&prepared)
    recording:Image_Recording; image_recording_init(&recording,r); defer image_recording_destroy(&recording,r)
    if r.frames.next_submission==max(u64) { return {},.Native_Failure,.None }
    uses_surface:=surface_recording_used(r,&prepared)
    if uses_surface && r.surface.submitted.owner!=nil { return {},.Busy,.None }
    import_error:=image_recording_imports(r,&recording,&prepared)
    if import_error!=.None { return {},import_error,.None }
    index:=token.slot
    slot:=&r.slots[index]; slot.token=token
    committed:=false
    defer { if !committed { clear_frame(r,slot,false); slot.poisoned=true } }
    for &block in slot.uploads { block.cursor=0 }
    for &scratch in slot.mip_scratch { scratch.used=false }
    if r.table.ResetCommandPool(r.device,slot.pool,{})!=.SUCCESS { return {},.Native_Failure,.None }
    for pool in slot.descriptors { if r.table.ResetDescriptorPool(r.device,pool,{})!=.SUCCESS { return {},.Native_Failure,.None } }
    slot.active_descriptor=0
    begin:=vk.CommandBufferBeginInfo{sType=.COMMAND_BUFFER_BEGIN_INFO,flags={.ONE_TIME_SUBMIT}}
    if r.table.BeginCommandBuffer(slot.command,&begin)!=.SUCCESS { return {},.Native_Failure,.None }
    for input in prepared.buffers {
        entry,ok:=gfx.storage_get(&r.buffers,input.handle); if !ok { return {},.Invalid_Resource,.None }
        entry^.refs+=1; append(&slot.buffers,entry^)
    }
    for input in prepared.textures {
        entry,ok:=gfx.storage_get(&r.textures,input.handle); if !ok { return {},.Invalid_Resource,.None }
        retained:=false; for prior in slot.textures { if prior==entry^ { retained=true; break } }
        if !retained { entry^.refs+=1; append(&slot.textures,entry^) }
    }
    // Persistent allocations may have been used by an earlier graph on this queue.
    history:=vk.MemoryBarrier2{sType=.MEMORY_BARRIER_2,srcStageMask={.ALL_COMMANDS},srcAccessMask={.MEMORY_READ,.MEMORY_WRITE},dstStageMask={.ALL_COMMANDS},dstAccessMask={.MEMORY_READ,.MEMORY_WRITE}}
    initial_dependency:=vk.DependencyInfo{sType=.DEPENDENCY_INFO,memoryBarrierCount=1,pMemoryBarriers=&history}
    r.table.CmdPipelineBarrier2(slot.command,&initial_dependency)
    for pass in prepared.passes {
        for alias in prepared.aliases {
            if alias.after!=pass.id { continue }
            err:=image_alias_handoff(r,slot.command,&recording,&prepared,alias); if err!=.None { return {},err,.None }
        }
        for hazard in prepared.hazards {
            if hazard.after!=pass.id { continue }
            buffer,present:=resolve_buffer(r,&prepared,hazard.resource); if !present { return {},.Invalid_Resource,.None }
            source_stage:=vk.PipelineStageFlags2{.ALL_COMMANDS}
            for previous in prepared.passes { if previous.id==hazard.before && previous.kind==.Transfer { source_stage={.ALL_TRANSFER}; break } }
            destination_stage:=vk.PipelineStageFlags2{.ALL_TRANSFER} if pass.kind==.Transfer else vk.PipelineStageFlags2{.ALL_COMMANDS}
            start:=max(hazard.source.range.offset,hazard.destination.range.offset)
            end:=min(hazard.source.range.offset+hazard.source.range.size,hazard.destination.range.offset+hazard.destination.range.size)
            barrier:=vk.BufferMemoryBarrier2{sType=.BUFFER_MEMORY_BARRIER_2,srcStageMask=source_stage,srcAccessMask=access_mask(hazard.source),dstStageMask=destination_stage,dstAccessMask=access_mask(hazard.destination),srcQueueFamilyIndex=vk.QUEUE_FAMILY_IGNORED,dstQueueFamilyIndex=vk.QUEUE_FAMILY_IGNORED,buffer=buffer.object,offset=vk.DeviceSize(start),size=vk.DeviceSize(end-start)}
            dependency:=vk.DependencyInfo{sType=.DEPENDENCY_INFO,bufferMemoryBarrierCount=1,pBufferMemoryBarriers=&barrier}
            r.table.CmdPipelineBarrier2(slot.command,&dependency)
        }
        switch packet in pass.packet {
        case gfx.Fill_Buffer:
            destination,present:=resolve_buffer(r,&prepared,packet.destination)
            if !present { return {},.Invalid_Resource,.None }
            r.table.CmdFillBuffer(slot.command,destination.object,vk.DeviceSize(packet.offset),vk.DeviceSize(packet.size),packet.value)
        case gfx.Generate_Mips:
            err:=encode_generate_mips(r,slot,&recording,&prepared,packet); if err!=.None { return {},err,.None }
        case gfx.Render:
            err:=encode_render(r,slot,&recording,&prepared,packet); if err!=.None { return {},err,.None }
        case gfx.Copy_Buffer_Image:
            err:=encode_buffer_image_copy(r,slot,&recording,&prepared,packet); if err!=.None { return {},err,.None }
        case gfx.Copy_Image_Buffer:
            err:=encode_image_copy(r,slot,&recording,&prepared,packet); if err!=.None { return {},err,.None }
        case gfx.Dispatch:
            err:=encode_dispatch(r,slot,&recording,&prepared,packet); if err!=.None { return {},err,.None }
        case gfx.Copy_Buffer:
            source,source_ok:=resolve_buffer(r,&prepared,packet.source)
            destination,destination_ok:=resolve_buffer(r,&prepared,packet.destination)
            if !source_ok || !destination_ok { return {},.Invalid_Resource,.None }
            copy_info:=vk.BufferCopy{vk.DeviceSize(packet.source_offset),vk.DeviceSize(packet.destination_offset),vk.DeviceSize(packet.size)}
            r.table.CmdCopyBuffer(slot.command,source.object,destination.object,1,&copy_info)
        }
    }
    for image in prepared.images {
        if image.contract.final==.Undefined { continue }
        texture,ok:=resolve_texture(r,&prepared,image.input.resource); if !ok { return {},.Invalid_Resource,.None }
        err:=transition_image(r,slot.command,&recording,texture,gfx.image_full_range(image.desc),image.contract.final,{.ALL_COMMANDS},{.MEMORY_READ,.MEMORY_WRITE}); if err!=.None { return {},err,.None }
    }
    if uses_surface {
        err:=surface_recording_final(r,slot.command,&recording); if err!=.None { return {},err,.None }
    }
    visible:=vk.MemoryBarrier2{sType=.MEMORY_BARRIER_2,srcStageMask={.ALL_COMMANDS},srcAccessMask={.MEMORY_WRITE},dstStageMask={.HOST},dstAccessMask={.HOST_READ}}
    final_dependency:=vk.DependencyInfo{sType=.DEPENDENCY_INFO,memoryBarrierCount=1,pMemoryBarriers=&visible}
    r.table.CmdPipelineBarrier2(slot.command,&final_dependency)
    if r.table.EndCommandBuffer(slot.command)!=.SUCCESS { return {},.Native_Failure,.None }
    if gfx.frame_recorded(&r.frames,token)!=.None { return {},.Native_Failure,.None }
    if r.table.ResetFences(r.device,1,&slot.fence)!=.SUCCESS { return {},.Native_Failure,.None }
    command_info:=vk.CommandBufferSubmitInfo{sType=.COMMAND_BUFFER_SUBMIT_INFO,commandBuffer=slot.command}
    submit_info:=vk.SubmitInfo2{sType=.SUBMIT_INFO_2,commandBufferInfoCount=1,pCommandBufferInfos=&command_info}
    surface_wait,surface_signal:vk.SemaphoreSubmitInfo
    if uses_surface {
        surface_wait={sType=.SEMAPHORE_SUBMIT_INFO,semaphore=r.surface.image_ready[r.surface.semaphore_slot],stageMask={.ALL_COMMANDS}}
        surface_signal={sType=.SEMAPHORE_SUBMIT_INFO,semaphore=r.surface.render_ready[r.surface.image_index],stageMask={.ALL_COMMANDS}}
        submit_info.waitSemaphoreInfoCount=1; submit_info.pWaitSemaphoreInfos=&surface_wait
        submit_info.signalSemaphoreInfoCount=1; submit_info.pSignalSemaphoreInfos=&surface_signal
    }
    result:=r.table.QueueSubmit2(r.queue,1,&submit_info,slot.fence)
    if result!=.SUCCESS { if result==.ERROR_DEVICE_LOST { r.failed=true }; log.error("Vulkan queue submission rejected",result); return {},.Native_Failure,.None }
    slot.accepted=true
    for buffer in slot.buffers { buffer.pending+=1; buffer.heap.pending+=1 }
    for texture in slot.textures { texture.pending+=1; if texture.allocation.heap!=nil { texture.allocation.heap.pending+=1 } }
    submission,frame_error:=gfx.frame_submitted(&r.frames,token); slot.submission=submission; committed=true
    if frame_error!=.None {
        r.failed=true; log.error("Accepted Vulkan submission lost its frame token")
        return {},.Native_Failure,.None
    }
    commit_content_epochs(r,g,&prepared)
    image_recording_commit(&recording,submission)
    publish_exports(r,&prepared,{r,token,submission})
    if uses_surface { r.surface.submitted={r,token,submission}; r.surface.present_signal=r.surface.render_ready[r.surface.image_index] }
    r.next_slot=(index+1)%len(r.slots)
    return {r,token,submission},.None,.None
}
