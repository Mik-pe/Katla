//! Native placement groups bind distinct buffer and image objects to one retained allocation.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
Native_Heap :: struct { memory:vk.DeviceMemory, mapped:rawptr, size,epoch:u64, refs,pending:int, domain:gfx.Memory_Domain }
@(private="package")
release_heap :: proc(r:^Renderer,heap:^Native_Heap) {
    if heap==nil { return }
    heap.refs-=1
    if heap.refs!=0 { return }
    if heap.mapped!=nil { r.table.UnmapMemory(r.device,heap.memory) }
    if heap.memory!=0 { r.table.FreeMemory(r.device,heap.memory,nil) }
    free(heap,r.allocator)
}
@(private="package")
heap_memory_types :: proc(r:^Renderer,bits:u32,domain:gfx.Memory_Domain)->u32 {
    result:u32
    for index in 0..<r.memory_properties.memoryTypeCount {
        flags:=r.memory_properties.memoryTypes[index].propertyFlags
        suitable:=.DEVICE_LOCAL in flags if domain==.GPU_Private else .HOST_VISIBLE in flags && .HOST_COHERENT in flags
        if bits&(u32(1)<<index)!=0 && suitable { result|=u32(1)<<index }
    }
    return result
}
@(private="package")
allocate_heap :: proc(r:^Renderer,size:u64,bits:u32,domain:gfx.Memory_Domain,buffer:vk.Buffer=0,image:vk.Image=0)->(^Native_Heap,gfx.Gpu_Error) {
    compatible:=heap_memory_types(r,bits,domain)
    if compatible==0 { return nil,.Unsupported }
    memory_type:u32
    for index in 0..<r.memory_properties.memoryTypeCount { if compatible&(u32(1)<<index)!=0 { memory_type=index; break } }
    heap:=new(Native_Heap,r.allocator); heap.refs=1; heap.size=size; heap.domain=domain
    success:=false; defer { if !success { release_heap(r,heap) } }
    dedicated:=vk.MemoryDedicatedAllocateInfo{sType=.MEMORY_DEDICATED_ALLOCATE_INFO,buffer=buffer,image=image}
    info:=vk.MemoryAllocateInfo{sType=.MEMORY_ALLOCATE_INFO,allocationSize=vk.DeviceSize(size),memoryTypeIndex=memory_type}
    if buffer!=0 || image!=0 { info.pNext=&dedicated }
    if r.table.AllocateMemory(r.device,&info,nil,&heap.memory)!=.SUCCESS { return nil,.Allocation_Failed }
    if domain==.CPU_Visible && r.table.MapMemory(r.device,heap.memory,0,vk.DeviceSize(size),{},&heap.mapped)!=.SUCCESS { return nil,.Allocation_Failed }
    success=true; return heap,.None
}
@(private="package")
buffer_requirements :: proc(r:^Renderer,object:vk.Buffer)->(vk.MemoryRequirements,bool) {
    dedicated:=vk.MemoryDedicatedRequirements{sType=.MEMORY_DEDICATED_REQUIREMENTS}
    requirements:=vk.MemoryRequirements2{sType=.MEMORY_REQUIREMENTS_2,pNext=&dedicated}
    info:=vk.BufferMemoryRequirementsInfo2{sType=.BUFFER_MEMORY_REQUIREMENTS_INFO_2,buffer=object}
    r.table.GetBufferMemoryRequirements2(r.device,&info,&requirements)
    return requirements.memoryRequirements,bool(dedicated.requiresDedicatedAllocation)
}
@(private="package")
image_requirements :: proc(r:^Renderer,object:vk.Image)->(vk.MemoryRequirements,bool) {
    dedicated:=vk.MemoryDedicatedRequirements{sType=.MEMORY_DEDICATED_REQUIREMENTS}
    requirements:=vk.MemoryRequirements2{sType=.MEMORY_REQUIREMENTS_2,pNext=&dedicated}
    info:=vk.ImageMemoryRequirementsInfo2{sType=.IMAGE_MEMORY_REQUIREMENTS_INFO_2,image=object}
    r.table.GetImageMemoryRequirements2(r.device,&info,&requirements)
    return requirements.memoryRequirements,bool(dedicated.requiresDedicatedAllocation)
}
@(private="package")
allocation_buffer_object :: proc(r:^Renderer,desc:gfx.Buffer_Desc)->(vk.Buffer,vk.MemoryRequirements,bool,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return 0,{},false,.Native_Failure }
    if desc.size==0 || desc.size>u64(max(int)) || desc.usage=={} { return 0,{},false,.Invalid_Range }
    usage:vk.BufferUsageFlags
    for kind in desc.usage {
        switch kind {
        case .Storage: usage|={.STORAGE_BUFFER}
        case .Uniform: usage|={.UNIFORM_BUFFER}
        case .Transfer_Source: usage|={.TRANSFER_SRC}
        case .Transfer_Destination,.Readback: usage|={.TRANSFER_DST}
        case .Vertex: usage|={.VERTEX_BUFFER}
        case .Index: usage|={.INDEX_BUFFER}
        case .Indirect: usage|={.INDIRECT_BUFFER}
        }
    }
    if desc.memory==.GPU_Private { usage|={.TRANSFER_DST} }
    info:=vk.BufferCreateInfo{sType=.BUFFER_CREATE_INFO,size=vk.DeviceSize(desc.size),usage=usage,sharingMode=.EXCLUSIVE}
    object:vk.Buffer
    if r.table.CreateBuffer(r.device,&info,nil,&object)!=.SUCCESS { return 0,{},false,.Allocation_Failed }
    requirements,dedicated:=buffer_requirements(r,object)
    return object,requirements,dedicated,.None
}
@(private="package")
allocation_query_buffer :: proc(state:rawptr,desc:gfx.Buffer_Desc)->(gfx.Memory_Requirements,gfx.Gpu_Error) {
    r:=cast(^Renderer)state
    object,requirements,dedicated,error:=allocation_buffer_object(r,desc)
    if error!=.None { return {},error }
    defer r.table.DestroyBuffer(r.device,object,nil)
    if dedicated { return {},.Unsupported }
    bits:=heap_memory_types(r,requirements.memoryTypeBits,desc.memory)
    if bits==0 { return {},.Unsupported }
    return {u64(requirements.size),u64(requirements.alignment),bits,desc.memory},.None
}
@(private="package")
allocation_query_texture :: proc(state:rawptr,desc:gfx.Texture_Desc)->(gfx.Memory_Requirements,gfx.Gpu_Error) {
    r:=cast(^Renderer)state
    info,error:=texture_create_info(r,desc)
    if error!=.None { return {},error }
    info.flags|={.ALIAS}
    object:vk.Image
    if r.table.CreateImage(r.device,&info,nil,&object)!=.SUCCESS { return {},.Allocation_Failed }
    defer r.table.DestroyImage(r.device,object,nil)
    requirements,dedicated:=image_requirements(r,object)
    if dedicated { return {},.Unsupported }
    bits:=heap_memory_types(r,requirements.memoryTypeBits,.GPU_Private)
    if bits==0 { return {},.Unsupported }
    return {u64(requirements.size),u64(requirements.alignment),bits,.GPU_Private},.None
}
/// Queries actual driver allocation sizes, alignment and memory type compatibility.
allocation_query :: proc(r:^Renderer)->gfx.Allocation_Query { return {r,allocation_query_buffer,allocation_query_texture} }
@(private="package")
allocate_group :: proc(state:rawptr,group:gfx.Allocation_Group,requests:[]gfx.Allocation_Request)->(gfx.Allocation_Result,gfx.Gpu_Error) {
    r:=cast(^Renderer)state
    if r.device==nil || r.failed { return {},.Native_Failure }
    if len(requests)==0 || group.size==0 || group.alignment==0 || group.size%group.alignment!=0 { return {},.Invalid_Range }
    buffers:=make([]^Native_Buffer,len(requests),r.allocator); defer delete(buffers,r.allocator)
    textures:=make([]^Native_Texture,len(requests),r.allocator); defer delete(textures,r.allocator)
    published:=false
    defer {
        if !published {
            for buffer in buffers { if buffer!=nil { release_buffer(r,buffer) } }
            for texture in textures { if texture!=nil { release_texture(r,texture) } }
        }
    }
    bits:=group.memory_types
    for request,index in requests {
        requirements:vk.MemoryRequirements
        dedicated:bool
        switch resource in request {
        case gfx.Buffer_Allocation:
            if resource.desc.memory!=group.domain { return {},.Invalid_Range }
            object,queried,requires_dedicated,error:=allocation_buffer_object(r,resource.desc)
            if error!=.None { return {},error }
            buffer:=new(Native_Buffer,r.allocator); buffer^={object=object,desc=resource.desc,refs=1}; buffers[index]=buffer
            requirements=queried; dedicated=requires_dedicated
        case gfx.Image_Allocation:
            if group.domain!=.GPU_Private { return {},.Invalid_Range }
            info,error:=texture_create_info(r,resource.desc)
            if error!=.None { return {},error }
            info.flags|={.ALIAS}
            texture:=new(Native_Texture,r.allocator); texture.desc=resource.desc; texture.refs=1; textures[index]=texture
            texture.views=make([dynamic]Native_Image_View,r.allocator)
            texture.layouts=make([]vk.ImageLayout,int(resource.desc.mip_levels)*int(resource.desc.layers)*3,r.allocator)
            texture.initialized=make([]bool,len(texture.layouts),r.allocator)
            if r.table.CreateImage(r.device,&info,nil,&texture.allocation.object)!=.SUCCESS { return {},.Allocation_Failed }
            requirements,dedicated=image_requirements(r,texture.allocation.object)
        }
        if dedicated || u64(requirements.size)>group.size || group.alignment%u64(requirements.alignment)!=0 { return {},.Unsupported }
        bits&=requirements.memoryTypeBits
    }
    heap,error:=allocate_heap(r,group.size,bits,group.domain)
    if error!=.None { return {},error }
    defer release_heap(r,heap)
    for index in 0..<len(requests) {
        if buffer:=buffers[index]; buffer!=nil {
            if r.table.BindBufferMemory(r.device,buffer.object,heap.memory,0)!=.SUCCESS { return {},.Allocation_Failed }
            heap.refs+=1; buffer.heap=heap; buffer.memory=heap.memory; buffer.mapped=heap.mapped
        } else {
            texture:=textures[index]
            if r.table.BindImageMemory(r.device,texture.allocation.object,heap.memory,0)!=.SUCCESS { return {},.Allocation_Failed }
            heap.refs+=1; texture.allocation.heap=heap; texture.allocation.memory=heap.memory
        }
    }
    handles:=make([]gfx.Allocation_Handle,len(requests),r.allocator)
    for index in 0..<len(requests) {
        if buffers[index]!=nil { handles[index]=gfx.storage_insert(&r.buffers,buffers[index]) }
        else { handles[index]=gfx.storage_insert(&r.textures,textures[index]) }
    }
    published=true; return {handles,r.allocator},.None
}
@(private="package")
allocation_destroy_buffer :: proc(state:rawptr,handle:gfx.Buffer_Handle)->gfx.Gpu_Error { return destroy_buffer(cast(^Renderer)state,handle) }
@(private="package")
allocation_destroy_texture :: proc(state:rawptr,handle:gfx.Texture_Handle)->gfx.Gpu_Error { return destroy_texture(cast(^Renderer)state,handle) }
/// Supplies shared native allocation ownership to the generic graph resource owner.
allocation_api :: proc(r:^Renderer)->gfx.Allocation_API { return {r,allocate_group,allocation_destroy_buffer,allocation_destroy_texture} }
@(private="package")
texture_content_epoch :: proc(texture:^Native_Texture)->u64 {
    return texture.allocation.heap.epoch if texture.allocation.heap!=nil else texture.content_epoch
}
@(private="package")
commit_content_epochs :: proc(r:^Renderer,g:^gfx.Graph,prepared:^gfx.Prepared_Graph) {
    for pass in prepared.passes {
        declared:=g.passes[pass.id.index]
        for access in declared.accesses {
            if !gfx.access_writes(access.mode) { continue }
            buffer,ok:=resolve_buffer(r,prepared,access.resource)
            if ok { buffer.heap.epoch+=1 }
        }
        for access in declared.images {
            if !gfx.access_writes(access.mode) { continue }
            texture,ok:=resolve_texture(r,prepared,access.resource)
            if ok {
                if texture.allocation.heap!=nil { texture.allocation.heap.epoch+=1 }
                else { texture.content_epoch+=1 }
            }
        }
    }
}
