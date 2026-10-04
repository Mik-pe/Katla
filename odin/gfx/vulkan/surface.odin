//! Swapchain parents outlive retained images and preserve accepted presentation outcomes.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
Native_Surface_Owner :: struct { object:vk.SurfaceKHR, platform:rawptr, refs:int }
@(private="package")
Native_Swapchain :: struct { object:vk.SwapchainKHR, owner:^Native_Surface_Owner, refs:int }
@(private="package")
Native_Surface :: struct {
    owner:^Native_Surface_Owner,
    swapchain:^Native_Swapchain,
    images:[]gfx.Texture_Handle,
    acquired:bool,
    image_index:u32,
    image_ready:[3]vk.Semaphore,
    render_ready,copy_ready:[]vk.Semaphore,
    present_signal:vk.Semaphore,
    frame:gfx.Surface_Frame,
    next_generation:u64,
    desc:gfx.Surface_Desc,
    semaphore_slot:int,
    submitted:gfx.Submission,
}
@(private="package")
release_surface_owner :: proc(r:^Renderer,owner:^Native_Surface_Owner) {
    owner.refs-=1
    if owner.refs!=0 { return }
    r.instance_api.DestroySurfaceKHR(r.instance,owner.object,nil)
    when ODIN_OS==.Darwin { native_surface_owner_release(owner.platform) }
    free(owner,r.allocator)
}
@(private="package")
release_swapchain :: proc(r:^Renderer,swapchain:^Native_Swapchain) {
    swapchain.refs-=1
    if swapchain.refs!=0 { return }
    r.table.DestroySwapchainKHR(r.device,swapchain.object,nil)
    release_surface_owner(r,swapchain.owner)
    free(swapchain,r.allocator)
}
@(private="package")
surface_release :: proc(r:^Renderer) {
    surface:=&r.surface
    for handle in surface.images {
        image,ok:=gfx.storage_remove(&r.textures,handle)
        if ok { release_texture(r,image) }
    }
    delete(surface.images,r.allocator)
    for semaphore in surface.image_ready { if semaphore!=0 { r.table.DestroySemaphore(r.device,semaphore,nil) } }
    for semaphore in surface.render_ready { if semaphore!=0 { r.table.DestroySemaphore(r.device,semaphore,nil) } }
    delete(surface.render_ready,r.allocator)
    for semaphore in surface.copy_ready { if semaphore!=0 { r.table.DestroySemaphore(r.device,semaphore,nil) } }
    delete(surface.copy_ready,r.allocator)
    if surface.swapchain!=nil { release_swapchain(r,surface.swapchain) }
    if surface.owner!=nil { release_surface_owner(r,surface.owner) }
    generation:=surface.next_generation
    surface^={next_generation=generation}
}
/// Replaces the presentation owner after draining GPU work, using physical pixel dimensions.
attach_surface :: proc(r:^Renderer,desc:gfx.Surface_Desc)->gfx.Gpu_Error {
    if r.device==nil || r.failed { return .Native_Failure }
    if !r.swapchain_supported || !r.surface_supported { return .Unsupported }
    if desc.view==nil || desc.width==0 || desc.height==0 { return .Invalid_Range }
    if r.table.DeviceWaitIdle(r.device)!=.SUCCESS { return .Native_Failure }
    for &slot in r.slots {
        if slot.accepted {
            err:=retire_slot(r,&slot,false)
            if err!=.None { return err }
        }
    }
    readback_latch_drain(r); drop_surface_exports(r)
    surface_release(r)
    object,platform,error:=native_surface_create(r,desc)
    if error!=.None { return error }
    surface:=&r.surface
    surface.owner=new(Native_Surface_Owner,r.allocator); surface.owner^={object,platform,1}; surface.desc=desc
    success:=false; defer { if !success { surface_release(r) } }
    supported:b32
    if r.instance_api.GetPhysicalDeviceSurfaceSupportKHR(r.physical,r.queue_family,object,&supported)!=.SUCCESS || !bool(supported) { return .Unsupported }
    capabilities:vk.SurfaceCapabilitiesKHR
    if r.instance_api.GetPhysicalDeviceSurfaceCapabilitiesKHR(r.physical,object,&capabilities)!=.SUCCESS { return .Native_Failure }
    width,height:=desc.width,desc.height
    if capabilities.currentExtent.width!=max(u32) { width=capabilities.currentExtent.width; height=capabilities.currentExtent.height }
    else { width=clamp(width,capabilities.minImageExtent.width,capabilities.maxImageExtent.width); height=clamp(height,capabilities.minImageExtent.height,capabilities.maxImageExtent.height) }
    if width==0 || height==0 { return .Invalid_Range }
    count:u32
    if r.instance_api.GetPhysicalDeviceSurfaceFormatsKHR(r.physical,object,&count,nil)!=.SUCCESS || count==0 { return .Unsupported }
    formats:=make([]vk.SurfaceFormatKHR,int(count),r.allocator); defer delete(formats,r.allocator)
    if r.instance_api.GetPhysicalDeviceSurfaceFormatsKHR(r.physical,object,&count,raw_data(formats))!=.SUCCESS { return .Native_Failure }
    chosen:vk.SurfaceFormatKHR; found:=false
    for format in formats { if format.format==.B8G8R8A8_UNORM && format.colorSpace==.SRGB_NONLINEAR { chosen=format; found=true; break } }
    if !found { return .Unsupported }
    usages:vk.ImageUsageFlags={.COLOR_ATTACHMENT}
    if .TRANSFER_SRC in capabilities.supportedUsageFlags { usages|={.TRANSFER_SRC} }
    if usages&capabilities.supportedUsageFlags!=usages { return .Unsupported }
    image_count:=capabilities.minImageCount+1
    if capabilities.maxImageCount>0 { image_count=min(image_count,capabilities.maxImageCount) }
    alpha:=vk.CompositeAlphaFlagsKHR{.OPAQUE}
    if alpha&capabilities.supportedCompositeAlpha!={.OPAQUE} {
        found_alpha:=false
        for candidate in capabilities.supportedCompositeAlpha { alpha={candidate}; found_alpha=true; break }
        if !found_alpha { return .Unsupported }
    }
    info:=vk.SwapchainCreateInfoKHR{sType=.SWAPCHAIN_CREATE_INFO_KHR,surface=object,minImageCount=image_count,imageFormat=chosen.format,imageColorSpace=chosen.colorSpace,imageExtent={width,height},imageArrayLayers=1,imageUsage=usages,imageSharingMode=.EXCLUSIVE,preTransform=capabilities.currentTransform,compositeAlpha=alpha,presentMode=.FIFO,clipped=true}
    swapchain:=new(Native_Swapchain,r.allocator); swapchain.owner=surface.owner; swapchain.refs=1; surface.owner.refs+=1; surface.swapchain=swapchain
    if r.table.CreateSwapchainKHR(r.device,&info,nil,&swapchain.object)!=.SUCCESS { return .Allocation_Failed }
    if r.table.GetSwapchainImagesKHR(r.device,swapchain.object,&count,nil)!=.SUCCESS || count==0 { return .Native_Failure }
    images:=make([]vk.Image,int(count),r.allocator); defer delete(images,r.allocator)
    if r.table.GetSwapchainImagesKHR(r.device,swapchain.object,&count,raw_data(images))!=.SUCCESS { return .Native_Failure }
    surface.images=make([]gfx.Texture_Handle,int(count),r.allocator)
    surface.render_ready=make([]vk.Semaphore,int(count),r.allocator)
    surface.copy_ready=make([]vk.Semaphore,int(count),r.allocator)
    texture_usage:gfx.Texture_Usages={.Color_Attachment,.Present}
    if .TRANSFER_SRC in usages { texture_usage|={.Transfer_Source} }
    texture_desc:=gfx.Texture_Desc{width,height,1,1,.BGRA8_Unorm,texture_usage,1}
    semaphore_info:=vk.SemaphoreCreateInfo{sType=.SEMAPHORE_CREATE_INFO}
    for image,i in images {
        texture:=new(Native_Texture,r.allocator)
        texture.allocation.object=image; texture.swapchain=swapchain; swapchain.refs+=1
        texture.desc=texture_desc; texture.refs=1; texture.views=make([dynamic]Native_Image_View,r.allocator)
        texture.layouts=make([]vk.ImageLayout,3,r.allocator); texture.initialized=make([]bool,3,r.allocator)
        surface.images[i]=gfx.storage_insert(&r.textures,texture)
        if r.table.CreateSemaphore(r.device,&semaphore_info,nil,&surface.render_ready[i])!=.SUCCESS { return .Allocation_Failed }
        if r.table.CreateSemaphore(r.device,&semaphore_info,nil,&surface.copy_ready[i])!=.SUCCESS { return .Allocation_Failed }
    }
    for &semaphore in surface.image_ready { if r.table.CreateSemaphore(r.device,&semaphore_info,nil,&semaphore)!=.SUCCESS { return .Allocation_Failed } }
    surface.desc.width=width; surface.desc.height=height
    success=true
    return .None
}
/// Borrows an acquired image after the current frame slot grants CPU ownership.
acquire_surface :: proc(r:^Renderer)->(gfx.Surface_Frame,gfx.Surface_Result,gfx.Gpu_Error) {
    surface:=&r.surface
    if surface.swapchain==nil { return {},.Unavailable,.None }
    token:=r.slots[r.next_slot].token
    if !valid_acquisition(r,token) { return {},.Fatal,.Invalid_Resource }
    if surface.submitted.owner!=nil { return {},.Fatal,.Busy }
    if !surface.acquired {
        result:=r.table.AcquireNextImageKHR(r.device,surface.swapchain.object,max(u64),surface.image_ready[token.slot],0,&surface.image_index)
        if result==.ERROR_OUT_OF_DATE_KHR { return {},.Recreate,.None }
        if result!=.SUCCESS && result!=.SUBOPTIMAL_KHR { return {},.Fatal,.Native_Failure }
        surface.acquired=true; surface.semaphore_slot=token.slot
    }
    if surface.next_generation==max(u64) { return {},.Fatal,.Native_Failure }
    surface.next_generation+=1
    surface.frame={surface,surface.next_generation,surface.images[surface.image_index],surface.desc.width,surface.desc.height}
    return surface.frame,.Presented,.None
}
/// Abandoning CPU encoding keeps Vulkan's acquired image and semaphore for later consumption.
abort_surface :: proc(r:^Renderer,frame:gfx.Surface_Frame)->gfx.Gpu_Error {
    if frame!=r.surface.frame || !r.surface.acquired || r.surface.submitted.owner!=nil { return .Invalid_Resource }
    r.surface.frame={}
    return .None
}
/// Returns accepted work even when native presentation requires recreation or fails.
present_surface :: proc(r:^Renderer,frame:gfx.Surface_Frame,submission:gfx.Submission)->(gfx.Present_Outcome,gfx.Gpu_Error) {
    surface:=&r.surface
    if frame!=surface.frame || !surface.acquired || surface.submitted!=submission || submission.owner!=r { return {},.Invalid_Resource }
    signal:=surface.present_signal
    info:=vk.PresentInfoKHR{sType=.PRESENT_INFO_KHR,waitSemaphoreCount=1,pWaitSemaphores=&signal,swapchainCount=1,pSwapchains=&surface.swapchain.object,pImageIndices=&surface.image_index}
    result:=r.table.QueuePresentKHR(r.queue,&info)
    surface.acquired=false; surface.frame={}; surface.submitted={}
    if result==.ERROR_OUT_OF_DATE_KHR || result==.SUBOPTIMAL_KHR { return {submission,.Recreate},.None }
    if result!=.SUCCESS { r.failed=true; return {submission,.Fatal},.None }
    return {submission,.Presented},.None
}
@(private="package")
surface_recording_used :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph)->bool {
    if !r.surface.acquired || r.surface.frame.owner==nil { return false }
    for input in prepared.textures { if input.handle==r.surface.frame.texture { return true } }
    return false
}
@(private="package")
surface_recording_final :: proc(r:^Renderer,command:vk.CommandBuffer,recording:^Image_Recording)->gfx.Gpu_Error {
    entry,ok:=gfx.storage_get(&r.textures,r.surface.frame.texture)
    if !ok { return .Invalid_Resource }
    return transition_image(r,command,recording,entry^,gfx.image_full_range(entry^.desc),.Present,{.ALL_COMMANDS},{.MEMORY_READ})
}
/// Releases presentation parents before the application's native window is destroyed.
detach_surface :: proc(r:^Renderer)->gfx.Gpu_Error {
    if r.device==nil { return .Invalid_Resource }
    if r.table.DeviceWaitIdle(r.device)!=.SUCCESS { return .Native_Failure }
    for &slot in r.slots { if slot.accepted { err:=retire_slot(r,&slot,false); if err!=.None { return err } } }
    readback_latch_drain(r); drop_surface_exports(r)
    surface_release(r)
    return .None
}

/// Recreates physical presentation images while retained copies keep their original sources.
resize_surface :: proc(r:^Renderer,width,height:u32)->gfx.Gpu_Error {
    if r.surface.owner==nil { return .Invalid_Resource }
    desc:=r.surface.desc; desc.width=width; desc.height=height
    return attach_surface(r,desc)
}

@(private="package")
drop_surface_exports :: proc(r:^Renderer) {
    index:=0
    for index<len(r.exports) {
        if r.exports[index].texture.swapchain!=nil {
            release_texture(r,r.exports[index].texture)
            ordered_remove(&r.exports,index)
        } else { index+=1 }
    }
}
