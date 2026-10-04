//! Images retain their shared native allocation until every bound object is released.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
Image_Memory :: struct { object:vk.Image, memory:vk.DeviceMemory, heap:^Native_Heap }

@(private="package")
image_memory_allocate :: proc(r:^Renderer,info:^vk.ImageCreateInfo)->(Image_Memory,gfx.Gpu_Error) {
    image:Image_Memory
    if r.table.CreateImage(r.device,info,nil,&image.object)!=.SUCCESS { return {},.Allocation_Failed }
    success:=false; defer { if !success { r.table.DestroyImage(r.device,image.object,nil) } }
    requirements,dedicated:=image_requirements(r,image.object)
    _=dedicated
    heap,error:=allocate_heap(r,u64(requirements.size),requirements.memoryTypeBits,.GPU_Private,0,image.object)
    if error!=.None { return {},error }
    if r.table.BindImageMemory(r.device,image.object,heap.memory,0)!=.SUCCESS { release_heap(r,heap); return {},.Allocation_Failed }
    image.memory=heap.memory; image.heap=heap
    success=true; return image,.None
}
