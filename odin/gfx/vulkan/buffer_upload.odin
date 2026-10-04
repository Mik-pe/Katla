//! Private immutable buffers are initialized by an accepted staging copy.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
upload_private_buffer :: proc(r:^Renderer,destination:^Native_Buffer,data:[]byte)->gfx.Gpu_Error {
    staging_handle,error:=create_buffer_with_data(r,{size=u64(len(data)),usage={.Transfer_Source},memory=.CPU_Visible},data)
    if error!=.None { return error }
    staging,_:=gfx.storage_remove(&r.buffers,staging_handle)
    transfer:=new(Native_Readback,r.allocator); transfer.buffer=staging; transfer.destination=destination; destination.refs+=1
    retained:=false; defer { if !retained { readback_release(r,transfer) } }
    pool_info:=vk.CommandPoolCreateInfo{sType=.COMMAND_POOL_CREATE_INFO,queueFamilyIndex=r.queue_family}
    if r.table.CreateCommandPool(r.device,&pool_info,nil,&transfer.pool)!=.SUCCESS { return .Allocation_Failed }
    allocate:=vk.CommandBufferAllocateInfo{sType=.COMMAND_BUFFER_ALLOCATE_INFO,commandPool=transfer.pool,level=.PRIMARY,commandBufferCount=1}
    if r.table.AllocateCommandBuffers(r.device,&allocate,&transfer.command)!=.SUCCESS { return .Allocation_Failed }
    fence_info:=vk.FenceCreateInfo{sType=.FENCE_CREATE_INFO}
    if r.table.CreateFence(r.device,&fence_info,nil,&transfer.fence)!=.SUCCESS { return .Allocation_Failed }
    begin:=vk.CommandBufferBeginInfo{sType=.COMMAND_BUFFER_BEGIN_INFO,flags={.ONE_TIME_SUBMIT}}
    if r.table.BeginCommandBuffer(transfer.command,&begin)!=.SUCCESS { return .Native_Failure }
    region:=vk.BufferCopy{size=vk.DeviceSize(len(data))}
    r.table.CmdCopyBuffer(transfer.command,staging.object,destination.object,1,&region)
    visibility:=vk.MemoryBarrier2{sType=.MEMORY_BARRIER_2,srcStageMask={.COPY},srcAccessMask={.TRANSFER_WRITE},dstStageMask={.ALL_COMMANDS},dstAccessMask={.MEMORY_READ,.MEMORY_WRITE}}
    dependency:=vk.DependencyInfo{sType=.DEPENDENCY_INFO,memoryBarrierCount=1,pMemoryBarriers=&visibility}
    r.table.CmdPipelineBarrier2(transfer.command,&dependency)
    if r.table.EndCommandBuffer(transfer.command)!=.SUCCESS { return .Native_Failure }
    command_info:=vk.CommandBufferSubmitInfo{sType=.COMMAND_BUFFER_SUBMIT_INFO,commandBuffer=transfer.command}
    submit_info:=vk.SubmitInfo2{sType=.SUBMIT_INFO_2,commandBufferInfoCount=1,pCommandBufferInfos=&command_info}
    result:=r.table.QueueSubmit2(r.queue,1,&submit_info,transfer.fence)
    if result!=.SUCCESS { if result==.ERROR_DEVICE_LOST { r.failed=true }; return .Native_Failure }
    transfer.accepted=true; destination.pending+=1; destination.heap.pending+=1; destination.heap.epoch+=1
    result=r.table.WaitForFences(r.device,1,&transfer.fence,true,max(u64))
    if result!=.SUCCESS && result!=.ERROR_DEVICE_LOST { append(&r.pending_uploads,transfer); retained=true; return .Native_Failure }
    if result==.ERROR_DEVICE_LOST { r.failed=true; return .Native_Failure }
    return .None
}
