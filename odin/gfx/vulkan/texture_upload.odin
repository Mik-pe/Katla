//! Texture uploads retain coherent staging and exact subresource layout ownership until completion.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"
import "core:log"

@(private="package")
retire_uploads :: proc(r:^Renderer,drained:bool=false)->gfx.Gpu_Error {
    index:=0
    for index<len(r.pending_uploads) {
        transfer:=r.pending_uploads[index]
        result:=vk.Result.SUCCESS
        if !drained { result=r.table.GetFenceStatus(r.device,transfer.fence) }
        if result==.NOT_READY { index+=1;continue }
        if result!=.SUCCESS && result!=.ERROR_DEVICE_LOST { log.error("Vulkan upload fence query failed",result);return .Native_Failure }
        readback_release(r,transfer);ordered_remove(&r.pending_uploads,index)
        if result==.ERROR_DEVICE_LOST { r.failed=true;return .Native_Failure }
    }
    return .None
}

/// Writes tightly packed bytes to one mip/layer/aspect while preserving untouched initialized pixels.
upload_texture :: proc(r:^Renderer,handle:gfx.Texture_Handle,region:gfx.Image_Region,bytes:[]byte)->gfx.Gpu_Error {
    if r.device==nil || r.failed { return .Native_Failure }
    if error:=retire_uploads(r);error!=.None { return error }
    entry,ok:=gfx.storage_get(&r.textures,handle)
    if !ok { return .Invalid_Resource }
    texture:=entry^
    queued:int
    for transfer in r.pending_uploads { if transfer.texture==texture { queued+=1 } }
    if texture.swapchain!=nil || texture.pending!=queued || texture.allocation.heap.pending!=queued { return .Busy }
    if .Transfer_Destination not_in texture.desc.usage || !gfx.image_region_valid(region,texture.desc) { return .Invalid_Range }
    layout,layout_ok:=gfx.image_region_layout(region,texture.desc)
    if !layout_ok { return .Invalid_Range }
    size:=layout.required_bytes
    if size!=u64(len(bytes)) { return .Invalid_Range }
    recording:Image_Recording; image_recording_init(&recording,r); defer image_recording_destroy(&recording,r)
    range:=gfx.image_region_range(region)
    width,height:=gfx.texture_mip_extent(texture.desc,region.mip)
    full:=region.x==0 && region.y==0 && region.width==width && region.height==height && region.z==0 && region.depth==max(u32(1),texture.desc.depth>>region.mip)
    if !full && !image_contents(&recording,r,texture,range) { return .Invalid_Graph }
    buffer_handle,buffer_error:=create_buffer_with_data(r,{size=size,usage={.Transfer_Source},memory=.CPU_Visible},bytes)
    if buffer_error!=.None { return buffer_error }
    buffer,_:=gfx.storage_remove(&r.buffers,buffer_handle)
    transfer:=new(Native_Readback,r.allocator)
    transfer.texture=texture; transfer.buffer=buffer; transfer.upload=true; texture.refs+=1
    accepted:=false; defer { if !accepted { readback_release(r,transfer) } }
    pool_info:=vk.CommandPoolCreateInfo{sType=.COMMAND_POOL_CREATE_INFO,queueFamilyIndex=r.queue_family}
    if r.table.CreateCommandPool(r.device,&pool_info,nil,&transfer.pool)!=.SUCCESS { return .Allocation_Failed }
    allocate:=vk.CommandBufferAllocateInfo{sType=.COMMAND_BUFFER_ALLOCATE_INFO,commandPool=transfer.pool,level=.PRIMARY,commandBufferCount=1}
    if r.table.AllocateCommandBuffers(r.device,&allocate,&transfer.command)!=.SUCCESS { return .Allocation_Failed }
    fence_info:=vk.FenceCreateInfo{sType=.FENCE_CREATE_INFO}
    if r.table.CreateFence(r.device,&fence_info,nil,&transfer.fence)!=.SUCCESS { return .Allocation_Failed }
    begin:=vk.CommandBufferBeginInfo{sType=.COMMAND_BUFFER_BEGIN_INFO,flags={.ONE_TIME_SUBMIT}}
    if r.table.BeginCommandBuffer(transfer.command,&begin)!=.SUCCESS { return .Native_Failure }
    error:=transition_image(r,transfer.command,&recording,texture,range,.Transfer_Destination,{.COPY},{.TRANSFER_WRITE})
    if error!=.None { return error }
    copies,copy_error:=image_copy_regions(r,texture.desc,region,0)
    if copy_error!=.None { return copy_error }; defer delete(copies,r.allocator)
    r.table.CmdCopyBufferToImage(transfer.command,buffer.object,texture.allocation.object,.TRANSFER_DST_OPTIMAL,u32(len(copies)),raw_data(copies))
    final:=gfx.Image_State.Shader_Read if .Sampled in texture.desc.usage else gfx.Image_State.Storage if .Storage in texture.desc.usage else gfx.Image_State.Transfer_Destination
    error=transition_image(r,transfer.command,&recording,texture,range,final,{.ALL_COMMANDS},{.MEMORY_READ,.MEMORY_WRITE})
    if error!=.None { return error }
    image_mark_contents(&recording,r,texture,range,true)
    if r.table.EndCommandBuffer(transfer.command)!=.SUCCESS { return .Native_Failure }
    command_info:=vk.CommandBufferSubmitInfo{sType=.COMMAND_BUFFER_SUBMIT_INFO,commandBuffer=transfer.command}
    submit_info:=vk.SubmitInfo2{sType=.SUBMIT_INFO_2,commandBufferInfoCount=1,pCommandBufferInfos=&command_info}
    result:=r.table.QueueSubmit2(r.queue,1,&submit_info,transfer.fence)
    if result!=.SUCCESS { if result==.ERROR_DEVICE_LOST { r.failed=true }; return .Native_Failure }
    transfer.accepted=true; accepted=true; texture.pending+=1; texture.allocation.heap.pending+=1; texture.allocation.heap.epoch+=1
    image_recording_commit(&recording,0)
    append(&r.pending_uploads,transfer)
    return .None
}
/// Publishes one full single-mip/single-layer image after upload queue acceptance.
/// Accepted uploads retain their staging and image independently until an actual fence retires.
create_texture_with_data :: proc(r:^Renderer,desc:gfx.Texture_Desc,bytes:[]byte)->(gfx.Texture_Handle,gfx.Gpu_Error) {
    aspects:=gfx.texture_aspects(desc.format)
    if desc.mip_levels!=1 || desc.layers!=1 || (aspects!={.Color} && aspects!={.Depth}) { return {},.Invalid_Range }
    handle,error:=create_texture(r,desc)
    if error!=.None { return {},error }
    aspect:=gfx.Image_Aspect.Color if aspects=={.Color} else gfx.Image_Aspect.Depth
    error=upload_texture(r,handle,{0,0,0,0,desc.width,desc.height,aspect,0,desc.depth,0,0},bytes)
    if error!=.None { destroy_texture(r,handle); return {},error }
    return handle,.None
}
