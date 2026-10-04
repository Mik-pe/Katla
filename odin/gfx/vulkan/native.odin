//! Vulkan 1.3 headless resources use explicit device dispatch and generational owners.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"
import "core:dynlib"
import "core:mem"
import "core:sync"
import "core:log"
import "core:fmt"
import "base:runtime"

@(private="package")
Native_Buffer :: struct { object:vk.Buffer, memory:vk.DeviceMemory, mapped:rawptr, heap:^Native_Heap, desc:gfx.Buffer_Desc, refs,pending:int }
@(private="package")
Native_Pipeline :: struct { object:vk.Pipeline, interface:^Native_Graphics_Pipeline, requirements:[]gfx.Binding_Requirement, local_size:[3]u32, refs:int }
@(private="package")
Native_Frame :: struct { pool:vk.CommandPool, command:vk.CommandBuffer, descriptors:[dynamic]vk.DescriptorPool, active_descriptor:int, fence:vk.Fence, token:gfx.Frame_Token, submission:u64, accepted,poisoned:bool, buffers:[dynamic]^Native_Buffer, pipelines:[dynamic]^Native_Pipeline, textures:[dynamic]^Native_Texture, graphics:[dynamic]^Native_Graphics_Pipeline, samplers:[dynamic]^Native_Sampler, uploads:[dynamic]Native_Upload_Block, mip_scratch:[dynamic]Native_Mip_Scratch }
/// Stationary Vulkan 1.3 owner; native resources are retained through exact fences.
Renderer :: struct {
    loader:dynlib.Library,
    surface:Native_Surface,
    instance:vk.Instance,
    instance_api:Instance_API,
    messenger:vk.DebugUtilsMessengerEXT,
    physical:vk.PhysicalDevice,
    device:vk.Device,
    queue:vk.Queue,
    queue_family:u32,
    table:vk.Device_VTable,
    memory_properties:vk.PhysicalDeviceMemoryProperties,
    limits:vk.PhysicalDeviceLimits,
    descriptor_limits:vk.PhysicalDeviceDescriptorIndexingProperties,
    sampled_arrays,storage_arrays:bool,
    sampler_anisotropy,separate_depth_stencil,swapchain_supported,surface_supported,wireframe,depth_bias_clamp,vertex_stores,fragment_stores,indirect_first_instance,compression_bc:bool,
    buffers:gfx.Resource_Storage(^Native_Buffer,gfx.Buffer_Kind),
    textures:gfx.Resource_Storage(^Native_Texture,gfx.Texture_Kind),
    samplers:gfx.Resource_Storage(^Native_Sampler,gfx.Sampler_Kind),
    graphics:gfx.Resource_Storage(^Native_Graphics_Pipeline,gfx.Graphics_Pipeline_Kind),
    readbacks:gfx.Resource_Storage(^Native_Readback,gfx.Readback_Kind),
    exports:[dynamic]Exported_Image,
    pending_uploads:[dynamic]^Native_Readback,
    pipelines:gfx.Resource_Storage(^Native_Pipeline,gfx.Pipeline_Kind),
    frames:gfx.Frames,
    slots:[3]Native_Frame,
    next_slot:int,
    validation_errors:u32,
    failed:bool,
    allocator:mem.Allocator,
}
@(private="package")
validation_callback :: proc "system" (severity:vk.DebugUtilsMessageSeverityFlagsEXT,message_type:vk.DebugUtilsMessageTypeFlagsEXT,data:^vk.DebugUtilsMessengerCallbackDataEXT,user:rawptr )->b32 {
    context=runtime.default_context()
    if .ERROR in severity {
        r:=cast(^Renderer)user
        sync.atomic_add(&r.validation_errors,1)
        fmt.eprintln("[error] Vulkan validation:",string(data.pMessage))
    }
    return false
}
/// Requires synchronization2 and enables validation explicitly when requested.
renderer_init :: proc(r:^Renderer,validation:=false,loader_path:string="",allocator:=context.allocator)->gfx.Gpu_Error {
    r.allocator=allocator
    path:=loader_path
    if path=="" {
        when ODIN_OS==.Darwin { path="libvulkan.dylib" }
        else when ODIN_OS==.Windows { path="vulkan-1.dll" }
        else { path="libvulkan.so.1" }
    }
    loaded:bool; r.loader,loaded=dynlib.load_library(path)
    if !loaded { return .No_Device }
    success:=false; defer { if !success { renderer_destroy(r) } }
    address,found:=dynlib.symbol_address(r.loader,"vkGetInstanceProcAddr"); if !found { return .Unsupported }
    get_instance:=cast(vk.ProcGetInstanceProcAddr)address
    create_instance:=cast(vk.ProcCreateInstance)get_instance(nil,"vkCreateInstance")
    if create_instance==nil { return .Unsupported }
    application:=vk.ApplicationInfo{sType=.APPLICATION_INFO,pApplicationName="Katla Odin",apiVersion=vk.MAKE_API_VERSION(0,1,3,0)}
    extensions:[8]cstring; extension_count:u32
    flags:vk.InstanceCreateFlags
    when ODIN_OS==.Darwin { extensions[extension_count]="VK_KHR_portability_enumeration"; extension_count+=1; flags={.ENUMERATE_PORTABILITY_KHR} }
    layers:=[1]cstring{"VK_LAYER_KHRONOS_validation"}
    debug:=vk.DebugUtilsMessengerCreateInfoEXT{sType=.DEBUG_UTILS_MESSENGER_CREATE_INFO_EXT,messageSeverity={.ERROR,.WARNING},messageType={.GENERAL,.VALIDATION,.PERFORMANCE},pfnUserCallback=validation_callback,pUserData=r}
    sync_enable:=[1]vk.ValidationFeatureEnableEXT{.SYNCHRONIZATION_VALIDATION}
    validation_features:=vk.ValidationFeaturesEXT{sType=.VALIDATION_FEATURES_EXT,enabledValidationFeatureCount=1,pEnabledValidationFeatures=raw_data(sync_enable[:]),pNext=&debug}
    info:=vk.InstanceCreateInfo{sType=.INSTANCE_CREATE_INFO,flags=flags,pApplicationInfo=&application}
    enumerate_extensions:=cast(vk.ProcEnumerateInstanceExtensionProperties)get_instance(nil,"vkEnumerateInstanceExtensionProperties")
    available_count:u32
    if enumerate_extensions==nil || enumerate_extensions(nil,&available_count,nil)!=.SUCCESS { return .Unsupported }
    available_extensions:=make([]vk.ExtensionProperties,int(available_count),allocator); defer delete(available_extensions,allocator)
    if enumerate_extensions(nil,&available_count,raw_data(available_extensions))!=.SUCCESS { return .Unsupported }
    surface_extensions_found:int
    for requested in native_surface_extensions() {
        for &extension in available_extensions {
            if string(cast(cstring)raw_data(extension.extensionName[:]))==string(requested) { extensions[extension_count]=requested; extension_count+=1; surface_extensions_found+=1; break }
        }
    }
    r.surface_supported=surface_extensions_found==2
    if validation { extensions[extension_count]="VK_EXT_debug_utils"; extension_count+=1; extensions[extension_count]="VK_EXT_validation_features"; extension_count+=1; info.enabledLayerCount=1; info.ppEnabledLayerNames=raw_data(layers[:]); info.pNext=&validation_features }
    info.enabledExtensionCount=extension_count; info.ppEnabledExtensionNames=raw_data(extensions[:])
    result:=create_instance(&info,nil,&r.instance)
    if result!=.SUCCESS { log.error("Vulkan instance creation failed",result); return .Unsupported }
    load_instance_api(&r.instance_api,r.instance,get_instance)
    if validation && r.instance_api.CreateDebugUtilsMessengerEXT(r.instance,&debug,nil,&r.messenger)!=.SUCCESS { return .Allocation_Failed }
    count:u32
    if r.instance_api.EnumeratePhysicalDevices(r.instance,&count,nil)!=.SUCCESS || count==0 { return .No_Device }
    devices:=make([]vk.PhysicalDevice,int(count),allocator); defer delete(devices,allocator)
    if r.instance_api.EnumeratePhysicalDevices(r.instance,&count,raw_data(devices))!=.SUCCESS { return .No_Device }
    for device in devices {
        properties:vk.PhysicalDeviceProperties; r.instance_api.GetPhysicalDeviceProperties(device,&properties)
        if properties.apiVersion<vk.MAKE_API_VERSION(0,1,3,0) { continue }
        features13:=vk.PhysicalDeviceVulkan13Features{sType=.PHYSICAL_DEVICE_VULKAN_1_3_FEATURES}
        features12:=vk.PhysicalDeviceVulkan12Features{sType=.PHYSICAL_DEVICE_VULKAN_1_2_FEATURES,pNext=&features13}
        features:=vk.PhysicalDeviceFeatures2{sType=.PHYSICAL_DEVICE_FEATURES_2,pNext=&features12}
        r.instance_api.GetPhysicalDeviceFeatures2(device,&features)
        if !bool(features13.synchronization2) || !bool(features13.maintenance4) || !bool(features13.dynamicRendering) { continue }
        queue_count:u32; r.instance_api.GetPhysicalDeviceQueueFamilyProperties(device,&queue_count,nil)
        queues:=make([]vk.QueueFamilyProperties,int(queue_count),allocator)
        r.instance_api.GetPhysicalDeviceQueueFamilyProperties(device,&queue_count,raw_data(queues))
        for queue,i in queues {
            if queue.queueCount>0 && .COMPUTE in queue.queueFlags && .GRAPHICS in queue.queueFlags { r.physical=device; r.queue_family=u32(i); r.limits=properties.limits; r.sampler_anisotropy=bool(features.features.samplerAnisotropy); r.separate_depth_stencil=bool(features12.separateDepthStencilLayouts); r.wireframe=bool(features.features.fillModeNonSolid); r.depth_bias_clamp=bool(features.features.depthBiasClamp); r.vertex_stores=bool(features.features.vertexPipelineStoresAndAtomics); r.fragment_stores=bool(features.features.fragmentStoresAndAtomics); r.indirect_first_instance=bool(features.features.drawIndirectFirstInstance); r.compression_bc=bool(features.features.textureCompressionBC); r.sampled_arrays=bool(features.features.shaderSampledImageArrayDynamicIndexing) && bool(features12.shaderSampledImageArrayNonUniformIndexing) && bool(features12.descriptorBindingSampledImageUpdateAfterBind); r.storage_arrays=bool(features.features.shaderStorageImageArrayDynamicIndexing) && bool(features12.shaderStorageImageArrayNonUniformIndexing) && bool(features12.descriptorBindingStorageImageUpdateAfterBind); break }
        }
        delete(queues,allocator)
        if r.physical!=nil { log.info("Odin Vulkan device",string(cast(cstring)raw_data(properties.deviceName[:]))); break }
    }
    if r.physical==nil { return .Unsupported }
    r.descriptor_limits.sType=.PHYSICAL_DEVICE_DESCRIPTOR_INDEXING_PROPERTIES
    properties2:=vk.PhysicalDeviceProperties2{sType=.PHYSICAL_DEVICE_PROPERTIES_2,pNext=&r.descriptor_limits}
    r.instance_api.GetPhysicalDeviceProperties2(r.physical,&properties2)
    r.instance_api.GetPhysicalDeviceMemoryProperties(r.physical,&r.memory_properties)
    priority:f32=1
    queue_info:=vk.DeviceQueueCreateInfo{sType=.DEVICE_QUEUE_CREATE_INFO,queueFamilyIndex=r.queue_family,queueCount=1,pQueuePriorities=&priority}
    enabled:=vk.PhysicalDeviceVulkan13Features{sType=.PHYSICAL_DEVICE_VULKAN_1_3_FEATURES,synchronization2=true,maintenance4=true,dynamicRendering=true}
    enabled12:=vk.PhysicalDeviceVulkan12Features{sType=.PHYSICAL_DEVICE_VULKAN_1_2_FEATURES,pNext=&enabled,separateDepthStencilLayouts=b32(r.separate_depth_stencil),shaderSampledImageArrayNonUniformIndexing=b32(r.sampled_arrays),descriptorBindingSampledImageUpdateAfterBind=b32(r.sampled_arrays),shaderStorageImageArrayNonUniformIndexing=b32(r.storage_arrays),descriptorBindingStorageImageUpdateAfterBind=b32(r.storage_arrays)}
    enabled_base:=vk.PhysicalDeviceFeatures{samplerAnisotropy=b32(r.sampler_anisotropy),fillModeNonSolid=b32(r.wireframe),depthBiasClamp=b32(r.depth_bias_clamp),vertexPipelineStoresAndAtomics=b32(r.vertex_stores),fragmentStoresAndAtomics=b32(r.fragment_stores),drawIndirectFirstInstance=b32(r.indirect_first_instance),textureCompressionBC=b32(r.compression_bc),shaderSampledImageArrayDynamicIndexing=b32(r.sampled_arrays),shaderStorageImageArrayDynamicIndexing=b32(r.storage_arrays)}
    device_info:=vk.DeviceCreateInfo{sType=.DEVICE_CREATE_INFO,pEnabledFeatures=&enabled_base,pNext=&enabled12,queueCreateInfoCount=1,pQueueCreateInfos=&queue_info}
    extension_total:u32
    if r.instance_api.EnumerateDeviceExtensionProperties(r.physical,nil,&extension_total,nil)!=.SUCCESS { return .Unsupported }
    device_extensions:=make([]vk.ExtensionProperties,int(extension_total),allocator); defer delete(device_extensions,allocator)
    if r.instance_api.EnumerateDeviceExtensionProperties(r.physical,nil,&extension_total,raw_data(device_extensions))!=.SUCCESS { return .Unsupported }
    device_extension_names:[2]cstring
    for &extension in device_extensions {
        name:=string(cast(cstring)raw_data(extension.extensionName[:]))
        if name=="VK_KHR_portability_subset" { device_extension_names[device_info.enabledExtensionCount]="VK_KHR_portability_subset"; device_info.enabledExtensionCount+=1 }
        if name=="VK_KHR_swapchain" { device_extension_names[device_info.enabledExtensionCount]="VK_KHR_swapchain"; device_info.enabledExtensionCount+=1; r.swapchain_supported=true }
    }
    device_info.ppEnabledExtensionNames=raw_data(device_extension_names[:])
    if r.instance_api.CreateDevice(r.physical,&device_info,nil,&r.device)!=.SUCCESS { return .Allocation_Failed }
    load_device_api(&r.table,r.device,r.instance_api.GetDeviceProcAddr,r.allocator)
    r.table.GetDeviceQueue(r.device,r.queue_family,0,&r.queue)
    gfx.storage_init(&r.readbacks,allocator); r.exports=make([dynamic]Exported_Image,allocator); r.pending_uploads=make([dynamic]^Native_Readback,allocator); gfx.storage_init(&r.graphics,allocator); gfx.storage_init(&r.samplers,allocator); gfx.storage_init(&r.textures,allocator); gfx.storage_init(&r.buffers,allocator); gfx.storage_init(&r.pipelines,allocator); gfx.frames_init(&r.frames,3,allocator)
    for &slot in r.slots {
        pool_info:=vk.CommandPoolCreateInfo{sType=.COMMAND_POOL_CREATE_INFO,queueFamilyIndex=r.queue_family}
        if r.table.CreateCommandPool(r.device,&pool_info,nil,&slot.pool)!=.SUCCESS { return .Allocation_Failed }
        allocate_info:=vk.CommandBufferAllocateInfo{sType=.COMMAND_BUFFER_ALLOCATE_INFO,commandPool=slot.pool,level=.PRIMARY,commandBufferCount=1}
        if r.table.AllocateCommandBuffers(r.device,&allocate_info,&slot.command)!=.SUCCESS { return .Allocation_Failed }
        slot.descriptors=make([dynamic]vk.DescriptorPool,allocator)
        pool,pool_error:=create_descriptor_pool(r)
        if pool_error!=.None { return pool_error }
        append(&slot.descriptors,pool)
        fence_info:=vk.FenceCreateInfo{sType=.FENCE_CREATE_INFO}
        if r.table.CreateFence(r.device,&fence_info,nil,&slot.fence)!=.SUCCESS { return .Allocation_Failed }
        slot.mip_scratch=make([dynamic]Native_Mip_Scratch,allocator); slot.uploads=make([dynamic]Native_Upload_Block,allocator); slot.textures=make([dynamic]^Native_Texture,allocator); slot.graphics=make([dynamic]^Native_Graphics_Pipeline,allocator); slot.samplers=make([dynamic]^Native_Sampler,allocator); slot.buffers=make([dynamic]^Native_Buffer,allocator); slot.pipelines=make([dynamic]^Native_Pipeline,allocator)
    }
    success=true; return .None
}
@(private="package")
release_buffer :: proc(r:^Renderer,buffer:^Native_Buffer) {
    buffer.refs-=1
    if buffer.refs==0 {
        r.table.DestroyBuffer(r.device,buffer.object,nil)
        release_heap(r,buffer.heap)
        free(buffer,r.allocator)
    }
}
@(private="package")
release_pipeline :: proc(r:^Renderer,pipeline:^Native_Pipeline) {
    pipeline.refs-=1
    if pipeline.refs==0 {
        r.table.DestroyPipeline(r.device,pipeline.object,nil); release_graphics_pipeline(r,pipeline.interface)
        delete(pipeline.requirements,r.allocator); free(pipeline,r.allocator)
    }
}
/// Drains accepted work before releasing native owners; an uncertain drain preserves them.
renderer_destroy :: proc(r:^Renderer)->gfx.Gpu_Error {
    outcome:=gfx.Gpu_Error.None
    if r.device!=nil {
        result:=r.table.DeviceWaitIdle(r.device)
        if result!=.SUCCESS && result!=.ERROR_DEVICE_LOST { return .Native_Failure }
        if result==.ERROR_DEVICE_LOST { outcome=.Native_Failure }
        for &slot in r.slots {
            if slot.accepted {
                if slot.submission!=0 { err:=retire_slot(r,&slot,result==.ERROR_DEVICE_LOST); if err!=.None { outcome=err } }
                else { clear_frame(r,&slot,true); gfx.frame_abort(&r.frames,slot.token); outcome=.Native_Failure }
            } else if slot.token.owner==&r.frames && r.frames.slots[slot.token.slot].state!=.Idle { gfx.frame_abort(&r.frames,slot.token) }
            if slot.fence!=0 { r.table.DestroyFence(r.device,slot.fence,nil) }
            for pool in slot.descriptors { r.table.DestroyDescriptorPool(r.device,pool,nil) }; delete(slot.descriptors)
            if slot.pool!=0 { r.table.DestroyCommandPool(r.device,slot.pool,nil) }
            for scratch in slot.mip_scratch { release_texture(r,scratch.texture) }; delete(slot.mip_scratch)
            for block in slot.uploads { release_buffer(r,block.buffer) }; delete(slot.uploads)
            delete(slot.buffers); delete(slot.pipelines); delete(slot.textures); delete(slot.graphics); delete(slot.samplers)
        }
        for transfer in r.pending_uploads { readback_release(r,transfer) }; delete(r.pending_uploads)
        surface_release(r)
        for &slot in r.readbacks.slots { if slot.occupied { readback_release(r,slot.value); slot.occupied=false } }
        r.readbacks.count=0; gfx.storage_destroy(&r.readbacks)
        for export in r.exports { release_texture(r,export.texture) }; delete(r.exports)
        for &slot in r.buffers.slots { if slot.occupied { release_buffer(r,slot.value); slot.occupied=false } }
        for &slot in r.textures.slots { if slot.occupied { release_texture(r,slot.value); slot.occupied=false } }
        r.textures.count=0; gfx.storage_destroy(&r.textures)
        for &slot in r.samplers.slots { if slot.occupied { release_sampler(r,slot.value); slot.occupied=false } }
        r.samplers.count=0; gfx.storage_destroy(&r.samplers)
        for &slot in r.graphics.slots { if slot.occupied { release_graphics_pipeline(r,slot.value); slot.occupied=false } }
        r.graphics.count=0; gfx.storage_destroy(&r.graphics)
        for &slot in r.pipelines.slots { if slot.occupied { release_pipeline(r,slot.value); slot.occupied=false } }
        r.buffers.count=0; r.pipelines.count=0; gfx.storage_destroy(&r.buffers); gfx.storage_destroy(&r.pipelines)
        if r.frames.slots!=nil { gfx.frames_destroy(&r.frames) }
        r.table.DestroyDevice(r.device,nil)
    }
    if r.messenger!=0 { r.instance_api.DestroyDebugUtilsMessengerEXT(r.instance,r.messenger,nil) }
    if r.instance!=nil { r.instance_api.DestroyInstance(r.instance,nil) }
    if r.loader!=nil { dynlib.unload_library(r.loader) }
    if validation_error_count(r)!=0 { outcome=.Native_Failure }
    r^={}; return outcome
}
/// Invalidates a registry handle while accepted work retains the underlying allocation.
destroy_buffer :: proc(r:^Renderer,handle:gfx.Buffer_Handle)->gfx.Gpu_Error {
    buffer,ok:=gfx.storage_remove(&r.buffers,handle); if !ok { return .Invalid_Resource }; release_buffer(r,buffer); return .None
}
/// Invalidates a pipeline handle while accepted work retains its complete native layout.
destroy_pipeline :: proc(r:^Renderer,handle:gfx.Pipeline_Handle)->gfx.Gpu_Error {
    pipeline,ok:=gfx.storage_remove(&r.pipelines,handle); if !ok { return .Invalid_Resource }; release_pipeline(r,pipeline); return .None
}
/// Creates native storage in the requested CPU or GPU memory domain.
create_buffer :: proc(r:^Renderer,desc:gfx.Buffer_Desc)->(gfx.Buffer_Handle,gfx.Gpu_Error) {
    object,requirements,_,err:=allocation_buffer_object(r,desc)
    if err!=.None { return {},err }
    success:=false; defer { if !success { r.table.DestroyBuffer(r.device,object,nil) } }
    heap,heap_error:=allocate_heap(r,u64(requirements.size),requirements.memoryTypeBits,desc.memory,object,0)
    if heap_error!=.None { return {},heap_error }
    if r.table.BindBufferMemory(r.device,object,heap.memory,0)!=.SUCCESS { release_heap(r,heap); return {},.Allocation_Failed }
    buffer:=new(Native_Buffer,r.allocator); buffer^={object=object,memory=heap.memory,mapped=heap.mapped,heap=heap,desc=desc,refs=1}
    success=true; return gfx.storage_insert(&r.buffers,buffer),.None
}
/// Checks generational identity, pending native use and byte bounds before CPU mutation.
write_buffer :: proc(r:^Renderer,token:gfx.Frame_Token,handle:gfx.Buffer_Handle,offset:u64,data:[]byte)->gfx.Gpu_Error {
    if !valid_acquisition(r,token) { return .Invalid_Resource }
    entry,ok:=gfx.storage_get(&r.buffers,handle); if !ok { return .Invalid_Resource }; buffer:=entry^
    if buffer.desc.memory==.GPU_Private { return .Unsupported }
    if buffer.pending!=0 || buffer.heap.pending!=0 { return .Busy }
    if offset>buffer.desc.size || u64(len(data))>buffer.desc.size-offset { return .Invalid_Range }
    copy((cast([^]byte)buffer.mapped)[int(offset):int(offset)+len(data)],data); buffer.heap.epoch+=1; return .None
}
/// Copies coherent host bytes only after all native consumers of the allocation retire.
read_buffer :: proc(r:^Renderer,handle:gfx.Buffer_Handle,offset:u64,data:[]byte)->gfx.Gpu_Error {
    entry,ok:=gfx.storage_get(&r.buffers,handle); if !ok { return .Invalid_Resource }; buffer:=entry^
    if buffer.desc.memory==.GPU_Private { return .Unsupported }
    if buffer.pending!=0 || buffer.heap.pending!=0 { return .Busy }
    if offset>buffer.desc.size || u64(len(data))>buffer.desc.size-offset { return .Invalid_Range }
    copy(data,(cast([^]byte)buffer.mapped)[int(offset):int(offset)+len(data)]); return .None
}
@(private="package")
query_buffer :: proc(state:rawptr,handle:gfx.Buffer_Handle)->(gfx.Buffer_Info,bool) {
    r:=cast(^Renderer)state; entry,ok:=gfx.storage_get(&r.buffers,handle); if !ok { return {},false }
    return {entry^.desc,entry^.heap},true
}
@(private="package")
query_pipeline :: proc(state:rawptr,handle:gfx.Pipeline_Handle)->(gfx.Pipeline_Info,bool) {
    r:=cast(^Renderer)state; entry,ok:=gfx.storage_get(&r.pipelines,handle); if !ok { return {},false }
    return {bindings=entry^.requirements,local_size=entry^.local_size,max_threads=u64(r.limits.maxComputeWorkGroupInvocations),images=entry^.interface.images,samplers=entry^.interface.samplers},true
}
/// Supplies actual allocation/layout metadata and hardware workgroup-count limits.
resource_query :: proc(r:^Renderer)->gfx.Resource_Query { return {r,query_buffer,query_pipeline,r.limits.maxComputeWorkGroupCount} }
/// Counts error-severity messages from the live native validation layer.
validation_error_count :: proc(r:^Renderer)->u32 { return sync.atomic_load(&r.validation_errors) }
