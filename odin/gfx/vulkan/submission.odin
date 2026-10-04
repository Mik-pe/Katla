//! Synchronization2 submission retains native resources through its exact fence.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"
import "core:log"

@(private="package")
clear_frame :: proc(r:^Renderer,slot:^Native_Frame,submitted:bool) {
    for buffer in slot.buffers { if submitted { buffer.pending-=1 }; release_buffer(r,buffer) }
    for pipeline in slot.pipelines { release_pipeline(r,pipeline) }
    clear(&slot.buffers); clear(&slot.pipelines); slot.submission=0; slot.accepted=false
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
    if access.usage==.Transfer_Source { return {.TRANSFER_READ} }
    if access.usage==.Transfer_Destination { return {.TRANSFER_WRITE} }
    if access.usage==.Uniform { return {.UNIFORM_READ} }
    if access.mode!=.Write { mask|={.SHADER_STORAGE_READ} }
    if access.mode!=.Read { mask|={.SHADER_STORAGE_WRITE} }
    return mask
}
/// Records validated packets and accepts them through one Vulkan synchronization2 entry point.
submit :: proc(r:^Renderer,g:^gfx.Buffer_Graph,plan:^gfx.Compiled_Graph,inputs:[]gfx.Buffer_Input)->(gfx.Submission,gfx.Gpu_Error,gfx.Packet_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure,.None }
    prepared,preflight:=gfx.graph_prepare(g,plan,inputs,resource_query(r))
    if preflight!=.None { return {},.Invalid_Graph,preflight }; defer gfx.prepared_graph_destroy(&prepared)
    if r.frames.next_submission==max(u64) { return {},.Native_Failure,.None }
    index:=r.next_slot
    token,acquired:=gfx.frame_acquire(&r.frames,index)
    if acquired!=.None {
        if acquired==.Busy { return {},.Busy,.None }
        r.failed=true; log.error("Vulkan frame acquisition failed",acquired); return {},.Native_Failure,.None
    }
    slot:=&r.slots[index]; slot.token=token
    committed:=false
    defer { if !committed { clear_frame(r,slot,false); gfx.frame_abort(&r.frames,token) } }
    if r.table.ResetCommandPool(r.device,slot.pool,{})!=.SUCCESS || r.table.ResetDescriptorPool(r.device,slot.descriptors,{})!=.SUCCESS { return {},.Native_Failure,.None }
    begin:=vk.CommandBufferBeginInfo{sType=.COMMAND_BUFFER_BEGIN_INFO,flags={.ONE_TIME_SUBMIT}}
    if r.table.BeginCommandBuffer(slot.command,&begin)!=.SUCCESS { return {},.Native_Failure,.None }
    for input in prepared.buffers {
        entry,ok:=gfx.storage_get(&r.buffers,input.handle); if !ok { return {},.Invalid_Resource,.None }
        entry^.refs+=1; append(&slot.buffers,entry^)
    }
    // Persistent allocations may have been used by an earlier graph on this queue.
    history:=vk.MemoryBarrier2{sType=.MEMORY_BARRIER_2,srcStageMask={.ALL_COMMANDS},srcAccessMask={.MEMORY_READ,.MEMORY_WRITE},dstStageMask={.ALL_COMMANDS},dstAccessMask={.MEMORY_READ,.MEMORY_WRITE}}
    initial_dependency:=vk.DependencyInfo{sType=.DEPENDENCY_INFO,memoryBarrierCount=1,pMemoryBarriers=&history}
    r.table.CmdPipelineBarrier2(slot.command,&initial_dependency)
    for pass in prepared.passes {
        for hazard in prepared.hazards {
            if hazard.after!=pass.id { continue }
            buffer,present:=resolve_buffer(r,&prepared,hazard.resource); if !present { return {},.Invalid_Resource,.None }
            source_stage:=vk.PipelineStageFlags2{.COMPUTE_SHADER}
            for previous in prepared.passes { if previous.id==hazard.before && previous.kind==.Transfer { source_stage={.COPY}; break } }
            destination_stage:=vk.PipelineStageFlags2{.COMPUTE_SHADER} if pass.kind==.Compute else vk.PipelineStageFlags2{.COPY}
            start:=max(hazard.source.range.offset,hazard.destination.range.offset)
            end:=min(hazard.source.range.offset+hazard.source.range.size,hazard.destination.range.offset+hazard.destination.range.size)
            barrier:=vk.BufferMemoryBarrier2{sType=.BUFFER_MEMORY_BARRIER_2,srcStageMask=source_stage,srcAccessMask=access_mask(hazard.source),dstStageMask=destination_stage,dstAccessMask=access_mask(hazard.destination),srcQueueFamilyIndex=vk.QUEUE_FAMILY_IGNORED,dstQueueFamilyIndex=vk.QUEUE_FAMILY_IGNORED,buffer=buffer.object,offset=vk.DeviceSize(start),size=vk.DeviceSize(end-start)}
            dependency:=vk.DependencyInfo{sType=.DEPENDENCY_INFO,bufferMemoryBarrierCount=1,pBufferMemoryBarriers=&barrier}
            r.table.CmdPipelineBarrier2(slot.command,&dependency)
        }
        switch packet in pass.packet {
        case gfx.Dispatch:
            entry,ok:=gfx.storage_get(&r.pipelines,packet.pipeline); if !ok { return {},.Invalid_Resource,.None }
            pipeline:=entry^; retain_pipeline(slot,pipeline)
            set:vk.DescriptorSet
            allocate:=vk.DescriptorSetAllocateInfo{sType=.DESCRIPTOR_SET_ALLOCATE_INFO,descriptorPool=slot.descriptors,descriptorSetCount=1,pSetLayouts=&pipeline.set_layout}
            if r.table.AllocateDescriptorSets(r.device,&allocate,&set)!=.SUCCESS { return {},.Allocation_Failed,.None }
            infos:[32]vk.DescriptorBufferInfo
            writes:[32]vk.WriteDescriptorSet
            for binding,i in packet.bindings {
                buffer,present:=resolve_buffer(r,&prepared,binding.access.resource); if !present { return {},.Invalid_Resource,.None }
                infos[i]={buffer.object,vk.DeviceSize(binding.access.range.offset),vk.DeviceSize(binding.access.range.size)}
                kind:=vk.DescriptorType.STORAGE_BUFFER if binding.access.usage==.Storage else vk.DescriptorType.UNIFORM_BUFFER
                writes[i]={sType=.WRITE_DESCRIPTOR_SET,dstSet=set,dstBinding=binding.slot,descriptorCount=1,descriptorType=kind,pBufferInfo=&infos[i]}
            }
            r.table.UpdateDescriptorSets(r.device,u32(len(packet.bindings)),raw_data(writes[:]),0,nil)
            r.table.CmdBindPipeline(slot.command,.COMPUTE,pipeline.object)
            r.table.CmdBindDescriptorSets(slot.command,.COMPUTE,pipeline.layout,0,1,&set,0,nil)
            r.table.CmdDispatch(slot.command,packet.groups[0],packet.groups[1],packet.groups[2])
        case gfx.Copy_Buffer:
            source,source_ok:=resolve_buffer(r,&prepared,packet.source)
            destination,destination_ok:=resolve_buffer(r,&prepared,packet.destination)
            if !source_ok || !destination_ok { return {},.Invalid_Resource,.None }
            copy_info:=vk.BufferCopy{vk.DeviceSize(packet.source_offset),vk.DeviceSize(packet.destination_offset),vk.DeviceSize(packet.size)}
            r.table.CmdCopyBuffer(slot.command,source.object,destination.object,1,&copy_info)
        }
    }
    visible:=vk.MemoryBarrier2{sType=.MEMORY_BARRIER_2,srcStageMask={.ALL_COMMANDS},srcAccessMask={.MEMORY_WRITE},dstStageMask={.HOST},dstAccessMask={.HOST_READ}}
    final_dependency:=vk.DependencyInfo{sType=.DEPENDENCY_INFO,memoryBarrierCount=1,pMemoryBarriers=&visible}
    r.table.CmdPipelineBarrier2(slot.command,&final_dependency)
    if r.table.EndCommandBuffer(slot.command)!=.SUCCESS { return {},.Native_Failure,.None }
    if gfx.frame_recorded(&r.frames,token)!=.None { return {},.Native_Failure,.None }
    if r.table.ResetFences(r.device,1,&slot.fence)!=.SUCCESS { return {},.Native_Failure,.None }
    command_info:=vk.CommandBufferSubmitInfo{sType=.COMMAND_BUFFER_SUBMIT_INFO,commandBuffer=slot.command}
    submit_info:=vk.SubmitInfo2{sType=.SUBMIT_INFO_2,commandBufferInfoCount=1,pCommandBufferInfos=&command_info}
    result:=r.table.QueueSubmit2(r.queue,1,&submit_info,slot.fence)
    if result!=.SUCCESS { if result==.ERROR_DEVICE_LOST { r.failed=true }; log.error("Vulkan queue submission rejected",result); return {},.Native_Failure,.None }
    slot.accepted=true
    for buffer in slot.buffers { buffer.pending+=1 }
    submission,frame_error:=gfx.frame_submitted(&r.frames,token); slot.submission=submission; committed=true
    if frame_error!=.None {
        r.failed=true; log.error("Accepted Vulkan submission lost its frame token")
        return {},.Native_Failure,.None
    }
    r.next_slot=(index+1)%len(r.slots)
    return {r,token,submission},.None,.None
}
