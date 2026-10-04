//! Descriptor pools grow only on native exhaustion and reset after exact frame retirement.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
create_descriptor_pool :: proc(r:^Renderer)->(vk.DescriptorPool,gfx.Gpu_Error) {
    sizes:=[5]vk.DescriptorPoolSize{{.STORAGE_BUFFER,4096},{.UNIFORM_BUFFER,4096},{.SAMPLED_IMAGE,4096},{.STORAGE_IMAGE,4096},{.SAMPLER,4096}}
    info:=vk.DescriptorPoolCreateInfo{sType=.DESCRIPTOR_POOL_CREATE_INFO,maxSets=128,poolSizeCount=u32(len(sizes)),pPoolSizes=raw_data(sizes[:])}
    pool:vk.DescriptorPool
    if r.table.CreateDescriptorPool(r.device,&info,nil,&pool)!=.SUCCESS { return 0,.Allocation_Failed }
    return pool,.None
}
@(private="package")
allocate_descriptor_sets :: proc(r:^Renderer,slot:^Native_Frame,layouts:[]vk.DescriptorSetLayout,sets:[^]vk.DescriptorSet)->gfx.Gpu_Error {
    if len(layouts)==0 { return .None }
    if len(layouts)>32 { return .Invalid_Shader }
    for slot.active_descriptor<len(slot.descriptors) {
        info:=vk.DescriptorSetAllocateInfo{sType=.DESCRIPTOR_SET_ALLOCATE_INFO,descriptorPool=slot.descriptors[slot.active_descriptor],descriptorSetCount=u32(len(layouts)),pSetLayouts=raw_data(layouts)}
        result:=r.table.AllocateDescriptorSets(r.device,&info,sets)
        if result==.SUCCESS { return .None }
        if result!=.ERROR_OUT_OF_POOL_MEMORY && result!=.ERROR_FRAGMENTED_POOL { return .Allocation_Failed }
        slot.active_descriptor+=1
    }
    pool,error:=create_descriptor_pool(r)
    if error!=.None { return error }
    append(&slot.descriptors,pool)
    info:=vk.DescriptorSetAllocateInfo{sType=.DESCRIPTOR_SET_ALLOCATE_INFO,descriptorPool=pool,descriptorSetCount=u32(len(layouts)),pSetLayouts=raw_data(layouts)}
    if r.table.AllocateDescriptorSets(r.device,&info,sets)!=.SUCCESS { return .Allocation_Failed }
    return .None
}
