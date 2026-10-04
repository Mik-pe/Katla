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
    if r.capture.recording { gfx.capture_record(&r.capture,{kind=.Allocation,pass_index=r.capture_pass,phase_index=r.capture_phase,resource_kind=.Auxiliary,resource_index=-1,object=capture_handle(r,4,u64(image.object)),heap=gfx.capture_object(&r.capture,heap),offset=0,size=heap.size,alignment=u64(requirements.alignment),memory_type=heap.memory_type,memory_flags=u64(transmute(u32)r.memory_properties.memoryTypes[heap.memory_type].propertyFlags),emitted=true,label="vkBindImageMemory",reason="actual image bound to accepted allocation"}) }
    success=true; return image,.None
}
