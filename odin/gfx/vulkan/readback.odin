//! Readback copies retain the exact committed image independently of reusable frame fences.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
Exported_Image :: struct { source:gfx.Texture_Source, texture:^Native_Texture }
@(private="package")
Native_Readback :: struct {
    source:gfx.Texture_Source,
    region:gfx.Image_Region,
    texture:^Native_Texture,
    buffer:^Native_Buffer,
    destination:^Native_Buffer,
    pool:vk.CommandPool,
    command:vk.CommandBuffer,
    fence:vk.Fence,
    accepted,complete,failed,upload:bool,
    latched_bytes:[]byte,
}
@(private="package")
readback_release :: proc(r:^Renderer,readback:^Native_Readback) {
    if readback.texture!=nil { if readback.upload && readback.accepted { readback.texture.pending-=1; if readback.texture.allocation.heap!=nil { readback.texture.allocation.heap.pending-=1 } }; release_texture(r,readback.texture) }
    if readback.destination!=nil { if readback.accepted { readback.destination.pending-=1; readback.destination.heap.pending-=1 }; release_buffer(r,readback.destination) }
    if readback.buffer!=nil { release_buffer(r,readback.buffer) }
    if readback.fence!=0 { r.table.DestroyFence(r.device,readback.fence,nil) }
    if readback.pool!=0 { r.table.DestroyCommandPool(r.device,readback.pool,nil) }
    delete(readback.latched_bytes,r.allocator)
    free(readback,r.allocator)
}
@(private="package")
publish_exports :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph,submission:gfx.Submission) {
    for image in prepared.images {
        if !image.exported { continue }
        texture,ok:=resolve_texture(r,prepared,image.input.resource)
        assert(ok)
        index:int=-1
        for export,i in r.exports {
            if export.source.resource==image.input.resource && export.source.submission.token.slot==submission.token.slot { index=i; break }
        }
        source:=gfx.Texture_Source{r,image.input.resource,image.input.handle,submission,texture_content_epoch(texture),image.desc}
        texture.refs+=1
        if index>=0 { release_texture(r,r.exports[index].texture); r.exports[index]={source,texture} }
        else { append(&r.exports,Exported_Image{source,texture}) }
    }
}
/// Finds the export from an exact accepted submission rather than a current graph binding.
graph_texture_source :: proc(r:^Renderer,submission:gfx.Submission,resource:gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error) {
    if submission.owner!=r { return {},.Invalid_Resource }
    for export in r.exports {
        if export.source.submission==submission && export.source.resource==resource { return export.source,.None }
    }
    return {},.Invalid_Resource
}
/// Accepts a source-bound copy on the queue before later submissions can overwrite its image.
queue_texture_readback :: proc(r:^Renderer,source:gfx.Texture_Source,region:gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    if source.owner!=r || !gfx.image_region_valid(region,source.desc) || .Transfer_Source not_in source.desc.usage { return {},.Invalid_Resource }
    texture:^Native_Texture
    for export in r.exports { if export.source==source { texture=export.texture; break } }
    if texture==nil || texture_content_epoch(texture)!=source.generation { return {},.Invalid_Resource }
    if texture.swapchain!=nil && (!r.surface.acquired || r.surface.frame.texture!=source.texture) { return {},.Invalid_Resource }
    range:=gfx.image_region_range(region)
    recording:Image_Recording; image_recording_init(&recording,r); defer image_recording_destroy(&recording,r)
    if !image_contents(&recording,r,texture,range) { return {},.Invalid_Graph }
    layout,layout_ok:=gfx.image_region_layout(region,source.desc)
    if !layout_ok { return {},.Invalid_Range }
    bytes:=layout.required_bytes
    handle,buffer_error:=create_buffer(r,{size=bytes,usage={.Readback,.Transfer_Destination},memory=.CPU_Visible})
    if buffer_error!=.None { return {},buffer_error }
    buffer,_:=gfx.storage_remove(&r.buffers,handle)
    readback:=new(Native_Readback,r.allocator)
    readback.source=source; readback.region=region; readback.texture=texture; readback.buffer=buffer
    texture.refs+=1
    accepted:=false; defer { if !accepted { readback_release(r,readback) } }
    pool_info:=vk.CommandPoolCreateInfo{sType=.COMMAND_POOL_CREATE_INFO,queueFamilyIndex=r.queue_family}
    if r.table.CreateCommandPool(r.device,&pool_info,nil,&readback.pool)!=.SUCCESS { return {},.Allocation_Failed }
    allocate:=vk.CommandBufferAllocateInfo{sType=.COMMAND_BUFFER_ALLOCATE_INFO,commandPool=readback.pool,level=.PRIMARY,commandBufferCount=1}
    if r.table.AllocateCommandBuffers(r.device,&allocate,&readback.command)!=.SUCCESS { return {},.Allocation_Failed }
    fence_info:=vk.FenceCreateInfo{sType=.FENCE_CREATE_INFO}
    if r.table.CreateFence(r.device,&fence_info,nil,&readback.fence)!=.SUCCESS { return {},.Allocation_Failed }
    begin:=vk.CommandBufferBeginInfo{sType=.COMMAND_BUFFER_BEGIN_INFO,flags={.ONE_TIME_SUBMIT}}
    if r.table.BeginCommandBuffer(readback.command,&begin)!=.SUCCESS { return {},.Native_Failure }
    err:=transition_image(r,readback.command,&recording,texture,range,.Transfer_Source,{.COPY},{.TRANSFER_READ})
    if err!=.None { return {},err }
    copies,copy_error:=image_copy_regions(r,texture.desc,region,0)
    if copy_error!=.None { return {},copy_error }; defer delete(copies,r.allocator)
    r.table.CmdCopyImageToBuffer(readback.command,texture.allocation.object,.TRANSFER_SRC_OPTIMAL,buffer.object,u32(len(copies)),raw_data(copies))
    journal:=image_journal(&recording,r,texture)
    original:=texture.layouts[texture_state_index(texture,region.mip,region.layer,region.aspect)]
    restore:=vk.ImageMemoryBarrier2{sType=.IMAGE_MEMORY_BARRIER_2,srcStageMask={.COPY},srcAccessMask={.TRANSFER_READ},dstStageMask={.ALL_COMMANDS},dstAccessMask={.MEMORY_READ,.MEMORY_WRITE},oldLayout=.TRANSFER_SRC_OPTIMAL,newLayout=original,srcQueueFamilyIndex=vk.QUEUE_FAMILY_IGNORED,dstQueueFamilyIndex=vk.QUEUE_FAMILY_IGNORED,image=texture.allocation.object,subresourceRange=image_range(range)}
    restore_dependency:=vk.DependencyInfo{sType=.DEPENDENCY_INFO,imageMemoryBarrierCount=1,pImageMemoryBarriers=&restore}
    r.table.CmdPipelineBarrier2(readback.command,&restore_dependency)
    journal.layouts[texture_state_index(texture,region.mip,region.layer,region.aspect)]=original
    visible:=vk.MemoryBarrier2{sType=.MEMORY_BARRIER_2,srcStageMask={.COPY},srcAccessMask={.TRANSFER_WRITE},dstStageMask={.HOST},dstAccessMask={.HOST_READ}}
    dependency:=vk.DependencyInfo{sType=.DEPENDENCY_INFO,memoryBarrierCount=1,pMemoryBarriers=&visible}
    r.table.CmdPipelineBarrier2(readback.command,&dependency)
    if r.table.EndCommandBuffer(readback.command)!=.SUCCESS { return {},.Native_Failure }
    command_info:=vk.CommandBufferSubmitInfo{sType=.COMMAND_BUFFER_SUBMIT_INFO,commandBuffer=readback.command}
    submit_info:=vk.SubmitInfo2{sType=.SUBMIT_INFO_2,commandBufferInfoCount=1,pCommandBufferInfos=&command_info}
    surface_wait,surface_signal:vk.SemaphoreSubmitInfo
    if texture.swapchain!=nil {
        next_signal:=r.surface.copy_ready[r.surface.image_index] if r.surface.present_signal==r.surface.render_ready[r.surface.image_index] else r.surface.render_ready[r.surface.image_index]
        surface_wait={sType=.SEMAPHORE_SUBMIT_INFO,semaphore=r.surface.present_signal,stageMask={.ALL_COMMANDS}}
        surface_signal={sType=.SEMAPHORE_SUBMIT_INFO,semaphore=next_signal,stageMask={.ALL_COMMANDS}}
        submit_info.waitSemaphoreInfoCount=1; submit_info.pWaitSemaphoreInfos=&surface_wait
        submit_info.signalSemaphoreInfoCount=1; submit_info.pSignalSemaphoreInfos=&surface_signal
    }
    result:=r.table.QueueSubmit2(r.queue,1,&submit_info,readback.fence)
    if result!=.SUCCESS { if result==.ERROR_DEVICE_LOST { r.failed=true }; return {},.Native_Failure }
    if texture.swapchain!=nil { r.surface.present_signal=surface_signal.semaphore }
    for journal in recording.journals { copy(journal.texture.layouts,journal.layouts) }
    readback.accepted=true; accepted=true
    return gfx.storage_insert(&r.readbacks,readback),.None
}
/// Polls without waiting and transfers completed owned pixels exactly once.
poll_texture_readback :: proc(r:^Renderer,ticket:gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error) {
    entry,ok:=gfx.storage_get(&r.readbacks,ticket)
    if !ok { return {},false,.Invalid_Resource }
    readback:=entry^
    if !readback.complete {
        result:=r.table.GetFenceStatus(r.device,readback.fence)
        if result==.NOT_READY { return {},false,.None }
        if result!=.SUCCESS && result!=.ERROR_DEVICE_LOST { return {},false,.Native_Failure }
        readback_latch(r,readback,result==.ERROR_DEVICE_LOST)
    }
    gfx.storage_remove(&r.readbacks,ticket)
    defer readback_release(r,readback)
    if readback.failed { r.failed=true; return {},true,.Native_Failure }
    bytes:=readback.latched_bytes; readback.latched_bytes=nil
    layout,_:=gfx.image_region_layout(readback.region,readback.source.desc)
    return {readback.source,readback.region,layout.bytes_per_row,bytes,r.allocator,layout.bytes_per_image},true,.None
}
/// Explicit ticket destruction waits its accepted copy before releasing native parents.
destroy_readback :: proc(r:^Renderer,ticket:gfx.Readback_Ticket)->gfx.Gpu_Error {
    entry,ok:=gfx.storage_get(&r.readbacks,ticket)
    if !ok { return .Invalid_Resource }
    readback:=entry^
    if !readback.complete {
        result:=r.table.WaitForFences(r.device,1,&readback.fence,true,max(u64))
        if result!=.SUCCESS && result!=.ERROR_DEVICE_LOST { return .Native_Failure }
        readback_latch(r,readback,result==.ERROR_DEVICE_LOST)
    }
    failed:=readback.failed
    gfx.storage_remove(&r.readbacks,ticket)
    readback_release(r,readback)
    if failed { r.failed=true; return .Native_Failure }
    return .None
}

@(private="package")
readback_latch :: proc(r:^Renderer,readback:^Native_Readback,failed:bool) {
    if readback.complete { return }
    if !failed && !readback.upload {
        readback.latched_bytes=make([]byte,int(readback.buffer.desc.size),r.allocator)
        copy(readback.latched_bytes,(cast([^]byte)readback.buffer.mapped)[:len(readback.latched_bytes)])
    }
    readback.complete=true; readback.failed=failed
    if readback.texture!=nil { if readback.upload && readback.accepted { readback.texture.pending-=1; if readback.texture.allocation.heap!=nil { readback.texture.allocation.heap.pending-=1 } }; release_texture(r,readback.texture); readback.texture=nil }
    if readback.buffer!=nil { release_buffer(r,readback.buffer); readback.buffer=nil }
    if readback.fence!=0 { r.table.DestroyFence(r.device,readback.fence,nil); readback.fence=0 }
    if readback.pool!=0 { r.table.DestroyCommandPool(r.device,readback.pool,nil); readback.pool=0; readback.command=nil }
}
@(private="package")
readback_latch_drain :: proc(r:^Renderer) {
    for &slot in r.readbacks.slots { if slot.occupied { readback_latch(r,slot.value,false) } }
}

/// Releases published sources for one graph while queued copies retain their own native owners.
release_graph_exports :: proc(r:^Renderer,g:^gfx.Graph)->gfx.Gpu_Error {
    for index:=len(r.exports)-1; index>=0; index-=1 {
        if r.exports[index].source.resource.owner!=g { continue }
        release_texture(r,r.exports[index].texture)
        ordered_remove(&r.exports,index)
    }
    return .None
}
