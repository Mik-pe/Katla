//! Vulkan 1.3 headless resources use explicit device dispatch and generational owners.
package katla_vulkan

import gfx ".."
import spirv "../spirv"
import vk "vendor:vulkan"
import "core:dynlib"
import "core:mem"
import "core:strings"
import "core:sync"
import "core:log"
import "core:fmt"
import "base:runtime"

@(private="package")
Native_Buffer :: struct { object:vk.Buffer, memory:vk.DeviceMemory, mapped:rawptr, desc:gfx.Buffer_Desc, refs,pending:int }
@(private="package")
Native_Pipeline :: struct { object:vk.Pipeline, layout:vk.PipelineLayout, set_layout:vk.DescriptorSetLayout, requirements:[]gfx.Binding_Requirement, local_size:[3]u32, refs:int }
@(private="package")
Native_Frame :: struct { pool:vk.CommandPool, command:vk.CommandBuffer, descriptors:vk.DescriptorPool, fence:vk.Fence, token:gfx.Frame_Token, submission:u64, accepted:bool, buffers:[dynamic]^Native_Buffer, pipelines:[dynamic]^Native_Pipeline }
/// Stationary Vulkan 1.3 owner; native resources are retained through exact fences.
Renderer :: struct {
    loader:dynlib.Library,
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
    buffers:gfx.Resource_Storage(^Native_Buffer,gfx.Buffer_Kind),
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
    extensions:[3]cstring; extension_count:u32
    flags:vk.InstanceCreateFlags
    when ODIN_OS==.Darwin { extensions[extension_count]="VK_KHR_portability_enumeration"; extension_count+=1; flags={.ENUMERATE_PORTABILITY_KHR} }
    layers:=[1]cstring{"VK_LAYER_KHRONOS_validation"}
    debug:=vk.DebugUtilsMessengerCreateInfoEXT{sType=.DEBUG_UTILS_MESSENGER_CREATE_INFO_EXT,messageSeverity={.ERROR,.WARNING},messageType={.GENERAL,.VALIDATION,.PERFORMANCE},pfnUserCallback=validation_callback,pUserData=r}
    sync_enable:=[1]vk.ValidationFeatureEnableEXT{.SYNCHRONIZATION_VALIDATION}
    validation_features:=vk.ValidationFeaturesEXT{sType=.VALIDATION_FEATURES_EXT,enabledValidationFeatureCount=1,pEnabledValidationFeatures=raw_data(sync_enable[:]),pNext=&debug}
    info:=vk.InstanceCreateInfo{sType=.INSTANCE_CREATE_INFO,flags=flags,pApplicationInfo=&application}
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
        features:=vk.PhysicalDeviceFeatures2{sType=.PHYSICAL_DEVICE_FEATURES_2,pNext=&features13}
        r.instance_api.GetPhysicalDeviceFeatures2(device,&features)
        if !bool(features13.synchronization2) || !bool(features13.maintenance4) { continue }
        queue_count:u32; r.instance_api.GetPhysicalDeviceQueueFamilyProperties(device,&queue_count,nil)
        queues:=make([]vk.QueueFamilyProperties,int(queue_count),allocator)
        r.instance_api.GetPhysicalDeviceQueueFamilyProperties(device,&queue_count,raw_data(queues))
        for queue,i in queues {
            if queue.queueCount>0 && .COMPUTE in queue.queueFlags { r.physical=device; r.queue_family=u32(i); r.limits=properties.limits; break }
        }
        delete(queues,allocator)
        if r.physical!=nil { log.info("Odin Vulkan device",string(cast(cstring)raw_data(properties.deviceName[:]))); break }
    }
    if r.physical==nil { return .Unsupported }
    r.instance_api.GetPhysicalDeviceMemoryProperties(r.physical,&r.memory_properties)
    priority:f32=1
    queue_info:=vk.DeviceQueueCreateInfo{sType=.DEVICE_QUEUE_CREATE_INFO,queueFamilyIndex=r.queue_family,queueCount=1,pQueuePriorities=&priority}
    enabled:=vk.PhysicalDeviceVulkan13Features{sType=.PHYSICAL_DEVICE_VULKAN_1_3_FEATURES,synchronization2=true,maintenance4=true}
    device_info:=vk.DeviceCreateInfo{sType=.DEVICE_CREATE_INFO,pNext=&enabled,queueCreateInfoCount=1,pQueueCreateInfos=&queue_info}
    extension_total:u32
    if r.instance_api.EnumerateDeviceExtensionProperties(r.physical,nil,&extension_total,nil)!=.SUCCESS { return .Unsupported }
    device_extensions:=make([]vk.ExtensionProperties,int(extension_total),allocator); defer delete(device_extensions,allocator)
    if r.instance_api.EnumerateDeviceExtensionProperties(r.physical,nil,&extension_total,raw_data(device_extensions))!=.SUCCESS { return .Unsupported }
    portability:=[1]cstring{"VK_KHR_portability_subset"}
    for &extension in device_extensions {
        if string(cast(cstring)raw_data(extension.extensionName[:]))=="VK_KHR_portability_subset" { device_info.enabledExtensionCount=1; device_info.ppEnabledExtensionNames=raw_data(portability[:]); break }
    }
    if r.instance_api.CreateDevice(r.physical,&device_info,nil,&r.device)!=.SUCCESS { return .Allocation_Failed }
    load_device_api(&r.table,r.device,r.instance_api.GetDeviceProcAddr,r.allocator)
    r.table.GetDeviceQueue(r.device,r.queue_family,0,&r.queue)
    gfx.storage_init(&r.buffers,allocator); gfx.storage_init(&r.pipelines,allocator); gfx.frames_init(&r.frames,3,allocator)
    pool_sizes:=[2]vk.DescriptorPoolSize{{.STORAGE_BUFFER,4096},{.UNIFORM_BUFFER,4096}}
    for &slot in r.slots {
        pool_info:=vk.CommandPoolCreateInfo{sType=.COMMAND_POOL_CREATE_INFO,queueFamilyIndex=r.queue_family}
        if r.table.CreateCommandPool(r.device,&pool_info,nil,&slot.pool)!=.SUCCESS { return .Allocation_Failed }
        allocate_info:=vk.CommandBufferAllocateInfo{sType=.COMMAND_BUFFER_ALLOCATE_INFO,commandPool=slot.pool,level=.PRIMARY,commandBufferCount=1}
        if r.table.AllocateCommandBuffers(r.device,&allocate_info,&slot.command)!=.SUCCESS { return .Allocation_Failed }
        descriptor_info:=vk.DescriptorPoolCreateInfo{sType=.DESCRIPTOR_POOL_CREATE_INFO,maxSets=128,poolSizeCount=2,pPoolSizes=raw_data(pool_sizes[:])}
        if r.table.CreateDescriptorPool(r.device,&descriptor_info,nil,&slot.descriptors)!=.SUCCESS { return .Allocation_Failed }
        fence_info:=vk.FenceCreateInfo{sType=.FENCE_CREATE_INFO}
        if r.table.CreateFence(r.device,&fence_info,nil,&slot.fence)!=.SUCCESS { return .Allocation_Failed }
        slot.buffers=make([dynamic]^Native_Buffer,allocator); slot.pipelines=make([dynamic]^Native_Pipeline,allocator)
    }
    success=true; return .None
}
@(private="package")
release_buffer :: proc(r:^Renderer,buffer:^Native_Buffer) {
    buffer.refs-=1
    if buffer.refs==0 { r.table.UnmapMemory(r.device,buffer.memory); r.table.DestroyBuffer(r.device,buffer.object,nil); r.table.FreeMemory(r.device,buffer.memory,nil); free(buffer,r.allocator) }
}
@(private="package")
release_pipeline :: proc(r:^Renderer,pipeline:^Native_Pipeline) {
    pipeline.refs-=1
    if pipeline.refs==0 {
        r.table.DestroyPipeline(r.device,pipeline.object,nil); r.table.DestroyPipelineLayout(r.device,pipeline.layout,nil); r.table.DestroyDescriptorSetLayout(r.device,pipeline.set_layout,nil)
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
            }
            if slot.fence!=0 { r.table.DestroyFence(r.device,slot.fence,nil) }
            if slot.descriptors!=0 { r.table.DestroyDescriptorPool(r.device,slot.descriptors,nil) }
            if slot.pool!=0 { r.table.DestroyCommandPool(r.device,slot.pool,nil) }
            delete(slot.buffers); delete(slot.pipelines)
        }
        for &slot in r.buffers.slots { if slot.occupied { release_buffer(r,slot.value); slot.occupied=false } }
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
/// Creates coherent host-visible storage; bytes remain undefined until caller initialization.
create_buffer :: proc(r:^Renderer,desc:gfx.Buffer_Desc)->(gfx.Buffer_Handle,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    if desc.size==0 || desc.size>u64(max(int)) || desc.usage=={} { return {},.Invalid_Range }
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
    buffer:=new(Native_Buffer,r.allocator); buffer.desc=desc; buffer.refs=1
    success:=false
    defer {
        if !success {
            if buffer.mapped!=nil { r.table.UnmapMemory(r.device,buffer.memory) }
            if buffer.object!=0 { r.table.DestroyBuffer(r.device,buffer.object,nil) }
            if buffer.memory!=0 { r.table.FreeMemory(r.device,buffer.memory,nil) }
            free(buffer,r.allocator)
        }
    }
    info:=vk.BufferCreateInfo{sType=.BUFFER_CREATE_INFO,size=vk.DeviceSize(desc.size),usage=usage,sharingMode=.EXCLUSIVE}
    if r.table.CreateBuffer(r.device,&info,nil,&buffer.object)!=.SUCCESS { return {},.Allocation_Failed }
    requirements:vk.MemoryRequirements; r.table.GetBufferMemoryRequirements(r.device,buffer.object,&requirements)
    memory_type:u32; found:=false
    for i in 0..<r.memory_properties.memoryTypeCount {
        properties:=r.memory_properties.memoryTypes[i].propertyFlags
        if requirements.memoryTypeBits&(u32(1)<<i)!=0 && .HOST_VISIBLE in properties && .HOST_COHERENT in properties { memory_type=i; found=true; break }
    }
    if !found { return {},.Unsupported }
    allocate:=vk.MemoryAllocateInfo{sType=.MEMORY_ALLOCATE_INFO,allocationSize=requirements.size,memoryTypeIndex=memory_type}
    if r.table.AllocateMemory(r.device,&allocate,nil,&buffer.memory)!=.SUCCESS { return {},.Allocation_Failed }
    if r.table.BindBufferMemory(r.device,buffer.object,buffer.memory,0)!=.SUCCESS { return {},.Allocation_Failed }
    if r.table.MapMemory(r.device,buffer.memory,0,vk.DeviceSize(desc.size),{},&buffer.mapped)!=.SUCCESS { return {},.Allocation_Failed }
    success=true; return gfx.storage_insert(&r.buffers,buffer),.None
}
/// Checks generational identity, pending native use and byte bounds before CPU mutation.
write_buffer :: proc(r:^Renderer,handle:gfx.Buffer_Handle,offset:u64,data:[]byte)->gfx.Gpu_Error {
    entry,ok:=gfx.storage_get(&r.buffers,handle); if !ok { return .Invalid_Resource }; buffer:=entry^
    if buffer.pending!=0 { return .Busy }
    if offset>buffer.desc.size || u64(len(data))>buffer.desc.size-offset { return .Invalid_Range }
    copy((cast([^]byte)buffer.mapped)[int(offset):int(offset)+len(data)],data); return .None
}
/// Copies coherent host bytes only after all native consumers of the allocation retire.
read_buffer :: proc(r:^Renderer,handle:gfx.Buffer_Handle,offset:u64,data:[]byte)->gfx.Gpu_Error {
    entry,ok:=gfx.storage_get(&r.buffers,handle); if !ok { return .Invalid_Resource }; buffer:=entry^
    if buffer.pending!=0 { return .Busy }
    if offset>buffer.desc.size || u64(len(data))>buffer.desc.size-offset { return .Invalid_Range }
    copy(data,(cast([^]byte)buffer.mapped)[int(offset):int(offset)+len(data)]); return .None
}
/// Reflects SPIR-V buffer layouts and compiles a prepared set-zero compute pipeline.
create_pipeline :: proc(r:^Renderer,desc:gfx.Compute_Desc)->(gfx.Pipeline_Handle,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    reflection,reflection_error:=spirv.reflect(desc.spirv,desc.entry,r.allocator)
    if reflection_error!=.None { return {},.Invalid_Shader }; defer spirv.destroy(&reflection)
    if reflection.local_size!=desc.local_size || len(reflection.buffers)!=len(desc.buffers) || len(desc.buffers)>32 { return {},.Invalid_Shader }
    threads:u64=1
    for count,i in desc.local_size { if count==0 || count>r.limits.maxComputeWorkGroupSize[i] || u64(count)>u64(r.limits.maxComputeWorkGroupInvocations)/threads { return {},.Invalid_Shader }; threads*=u64(count) }
    layouts:[32]vk.DescriptorSetLayoutBinding
    requirements:=make([]gfx.Binding_Requirement,len(desc.buffers),r.allocator)
    success:=false; defer { if !success { delete(requirements,r.allocator) } }
    for binding,i in desc.buffers {
        for previous in desc.buffers[:i] { if previous.slot==binding.slot { return {},.Invalid_Shader } }
        matched:=false
        for reflected in reflection.buffers {
            if reflected.slot!=binding.slot { continue }
            usage:=gfx.Buffer_Usage.Storage if reflected.storage else gfx.Buffer_Usage.Uniform
            if usage!=binding.usage { return {},.Invalid_Shader }
            alignment:=u64(r.limits.minStorageBufferOffsetAlignment) if reflected.storage else u64(r.limits.minUniformBufferOffsetAlignment)
            maximum:=u64(r.limits.maxStorageBufferRange) if reflected.storage else u64(r.limits.maxUniformBufferRange)
            requirements[i]={binding.slot,usage,reflected.minimum_size,max(alignment,1),maximum}; matched=true
        }
        if !matched { return {},.Invalid_Shader }
        descriptor_type:=vk.DescriptorType.STORAGE_BUFFER if binding.usage==.Storage else vk.DescriptorType.UNIFORM_BUFFER
        layouts[i]={binding=binding.slot,descriptorType=descriptor_type,descriptorCount=1,stageFlags={.COMPUTE}}
    }
    pipeline:=new(Native_Pipeline,r.allocator); pipeline.requirements=requirements; pipeline.local_size=desc.local_size; pipeline.refs=1
    defer {
        if !success {
            if pipeline.object!=0 { r.table.DestroyPipeline(r.device,pipeline.object,nil) }
            if pipeline.layout!=0 { r.table.DestroyPipelineLayout(r.device,pipeline.layout,nil) }
            if pipeline.set_layout!=0 { r.table.DestroyDescriptorSetLayout(r.device,pipeline.set_layout,nil) }
            free(pipeline,r.allocator)
        }
    }
    set_info:=vk.DescriptorSetLayoutCreateInfo{sType=.DESCRIPTOR_SET_LAYOUT_CREATE_INFO,bindingCount=u32(len(desc.buffers)),pBindings=raw_data(layouts[:])}
    if r.table.CreateDescriptorSetLayout(r.device,&set_info,nil,&pipeline.set_layout)!=.SUCCESS { return {},.Allocation_Failed }
    layout_info:=vk.PipelineLayoutCreateInfo{sType=.PIPELINE_LAYOUT_CREATE_INFO,setLayoutCount=1,pSetLayouts=&pipeline.set_layout}
    if r.table.CreatePipelineLayout(r.device,&layout_info,nil,&pipeline.layout)!=.SUCCESS { return {},.Allocation_Failed }
    module:vk.ShaderModule
    shader_info:=vk.ShaderModuleCreateInfo{sType=.SHADER_MODULE_CREATE_INFO,codeSize=len(desc.spirv)*4,pCode=raw_data(desc.spirv)}
    if r.table.CreateShaderModule(r.device,&shader_info,nil,&module)!=.SUCCESS { return {},.Shader_Compile_Failed }; defer r.table.DestroyShaderModule(r.device,module,nil)
    name:=strings.clone_to_cstring(desc.entry,r.allocator); defer delete(name,r.allocator)
    pipeline_info:=vk.ComputePipelineCreateInfo{sType=.COMPUTE_PIPELINE_CREATE_INFO,stage={sType=.PIPELINE_SHADER_STAGE_CREATE_INFO,stage={.COMPUTE},module=module,pName=name},layout=pipeline.layout}
    if r.table.CreateComputePipelines(r.device,0,1,&pipeline_info,nil,&pipeline.object)!=.SUCCESS { return {},.Shader_Compile_Failed }
    success=true; return gfx.storage_insert(&r.pipelines,pipeline),.None
}
@(private="package")
query_buffer :: proc(state:rawptr,handle:gfx.Buffer_Handle)->(gfx.Buffer_Info,bool) {
    r:=cast(^Renderer)state; entry,ok:=gfx.storage_get(&r.buffers,handle); if !ok { return {},false }
    return {entry^.desc,entry^},true
}
@(private="package")
query_pipeline :: proc(state:rawptr,handle:gfx.Pipeline_Handle)->(gfx.Pipeline_Info,bool) {
    r:=cast(^Renderer)state; entry,ok:=gfx.storage_get(&r.pipelines,handle); if !ok { return {},false }
    return {entry^.requirements,entry^.local_size,u64(r.limits.maxComputeWorkGroupInvocations)},true
}
/// Supplies actual allocation/layout metadata and hardware workgroup-count limits.
resource_query :: proc(r:^Renderer)->gfx.Resource_Query { return {r,query_buffer,query_pipeline,r.limits.maxComputeWorkGroupCount} }
/// Counts error-severity messages from the live native validation layer.
validation_error_count :: proc(r:^Renderer)->u32 { return sync.atomic_load(&r.validation_errors) }
