//! Fixed descriptor arrays are populated before publication and retained by the frame owner.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
image_descriptor_limits :: proc(r:^Renderer,desc:gfx.Graphics_Desc)->gfx.Gpu_Error {
    sampled,storage,ordinary_sampled,ordinary_storage:[3]u64
    sampled_total,storage_total,ordinary_sampled_total,ordinary_storage_total:u64
    sampled_update,storage_update:bool
    for binding in desc.images {
        if binding.array_count==0 { return .Invalid_Shader }
        if binding.array_count>4096 { return .Unsupported }
        group_update:=false
        for other in desc.images { if other.group==binding.group && other.array_count>1 { group_update=true; break } }
        if binding.usage==.Sampled {
            sampled_total+=u64(binding.array_count)
            sampled_update=sampled_update || binding.array_count>1
            if !group_update { ordinary_sampled_total+=u64(binding.array_count) }
            for stage in binding.stages {
                sampled[int(stage)]+=u64(binding.array_count)
                if binding.array_count==1 { ordinary_sampled[int(stage)]+=1 }
            }
        } else if binding.usage==.Storage {
            storage_total+=u64(binding.array_count)
            storage_update=storage_update || binding.array_count>1
            if !group_update { ordinary_storage_total+=u64(binding.array_count) }
            for stage in binding.stages {
                storage[int(stage)]+=u64(binding.array_count)
                if binding.array_count==1 { ordinary_storage[int(stage)]+=1 }
            }
        } else { return .Invalid_Shader }
    }
    sampled_set:=r.limits.maxDescriptorSetSampledImages
    sampled_stage:=r.limits.maxPerStageDescriptorSampledImages
    storage_set:=r.limits.maxDescriptorSetStorageImages
    storage_stage:=r.limits.maxPerStageDescriptorStorageImages
    if sampled_update {
        if !r.sampled_arrays { return .Unsupported }
        sampled_set=r.descriptor_limits.maxDescriptorSetUpdateAfterBindSampledImages
        sampled_stage=r.descriptor_limits.maxPerStageDescriptorUpdateAfterBindSampledImages
    }
    if storage_update {
        if !r.storage_arrays { return .Unsupported }
        storage_set=r.descriptor_limits.maxDescriptorSetUpdateAfterBindStorageImages
        storage_stage=r.descriptor_limits.maxPerStageDescriptorUpdateAfterBindStorageImages
    }
    if ordinary_sampled_total>u64(r.limits.maxDescriptorSetSampledImages) || ordinary_storage_total>u64(r.limits.maxDescriptorSetStorageImages) { return .Unsupported }
    for count in ordinary_sampled { if count>u64(r.limits.maxPerStageDescriptorSampledImages) { return .Unsupported } }
    for count in ordinary_storage { if count>u64(r.limits.maxPerStageDescriptorStorageImages) { return .Unsupported } }
    if sampled_total>u64(sampled_set) || storage_total>u64(storage_set) { return .Unsupported }
    for count in sampled { if count>u64(sampled_stage) { return .Unsupported } }
    for count in storage { if count>u64(storage_stage) { return .Unsupported } }
    return .None
}
@(private="package")
write_image_descriptors :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph,binding:gfx.Image_Binding,requirement:gfx.Image_Binding_Requirement,set:vk.DescriptorSet)->gfx.Gpu_Error {
    infos:=make([]vk.DescriptorImageInfo,len(binding.accesses),r.allocator); defer delete(infos,r.allocator)
    for access,i in binding.accesses {
        texture,present:=resolve_texture(r,prepared,access.resource)
        if !present { return .Invalid_Resource }
        view,error:=texture_view(r,texture,access.range,requirement.arrayed)
        if error!=.None { return error }
        infos[i]={imageView=view,imageLayout=image_layout(image_use_state(access.usage))}
    }
    descriptor:=vk.DescriptorType.STORAGE_IMAGE if requirement.usage==.Storage else vk.DescriptorType.SAMPLED_IMAGE
    write:=vk.WriteDescriptorSet{sType=.WRITE_DESCRIPTOR_SET,dstSet=set,dstBinding=binding.slot,descriptorCount=u32(len(infos)),descriptorType=descriptor,pImageInfo=raw_data(infos)}
    r.table.UpdateDescriptorSets(r.device,1,&write,0,nil)
    return .None
}
@(private="package")
transition_bound_images :: proc(r:^Renderer,slot:^Native_Frame,recording:^Image_Recording,prepared:^gfx.Prepared_Graph,bindings:[]gfx.Image_Binding,compute:bool)->gfx.Gpu_Error {
    seen:=make([dynamic]gfx.Image_Access,r.allocator); defer delete(seen)
    for binding in bindings { for access in binding.accesses {
        repeated:=false
        for prior in seen { if prior==access { repeated=true; break } }
        if repeated { continue }
        append(&seen,access)
        texture,present:=resolve_texture(r,prepared,access.resource)
        if !present { return .Invalid_Resource }
        if gfx.access_reads(access.mode) && !image_contents(recording,r,texture,access.range) { return .Invalid_Graph }
        stages:=image_stage(access.usage)
        if compute { stages={.COMPUTE_SHADER} }
        error:=transition_image(r,slot.command,recording,texture,access.range,image_use_state(access.usage),stages,image_access_mask(access))
        if error!=.None { return error }
    } }
    return .None
}
@(private="package")
mark_bound_images :: proc(r:^Renderer,recording:^Image_Recording,prepared:^gfx.Prepared_Graph,bindings:[]gfx.Image_Binding) {
    for binding in bindings { for access in binding.accesses {
        if gfx.access_writes(access.mode) {
            texture,_:=resolve_texture(r,prepared,access.resource)
            image_mark_contents(recording,r,texture,access.range,true)
        }
    } }
}
