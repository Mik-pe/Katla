//! Compute commands bind every reflected descriptor group without rewriting earlier packets.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
encode_dispatch :: proc(r:^Renderer,slot:^Native_Frame,recording:^Image_Recording,prepared:^gfx.Prepared_Graph,packet:gfx.Dispatch)->gfx.Gpu_Error {
    entry,ok:=gfx.storage_get(&r.pipelines,packet.pipeline)
    if !ok { return .Invalid_Resource }
    pipeline:=entry^; retain_pipeline(slot,pipeline)
    interface:=pipeline.interface
    sets:[32]vk.DescriptorSet
    if len(interface.set_layouts)>0 {
        error:=allocate_descriptor_sets(r,slot,interface.set_layouts,raw_data(sets[:]))
        if error!=.None { return error }
    }
    for binding in packet.bindings {
        buffer,present:=resolve_buffer(r,prepared,binding.access.resource)
        if !present { return .Invalid_Resource }
        info:=vk.DescriptorBufferInfo{buffer.object,vk.DeviceSize(binding.access.range.offset),vk.DeviceSize(binding.access.range.size)}
        descriptor:=vk.DescriptorType.STORAGE_BUFFER if binding.access.usage==.Storage else vk.DescriptorType.UNIFORM_BUFFER
        write:=vk.WriteDescriptorSet{sType=.WRITE_DESCRIPTOR_SET,dstSet=sets[binding.group],dstBinding=binding.slot,descriptorCount=1,descriptorType=descriptor,pBufferInfo=&info}
        r.table.UpdateDescriptorSets(r.device,1,&write,0,nil)
    }
    for binding in packet.images {
        texture,present:=resolve_texture(r,prepared,binding.access.resource)
        if !present { return .Invalid_Resource }
        if gfx.access_reads(binding.access.mode) && !image_contents(recording,r,texture,binding.access.range) { return .Invalid_Graph }
        state:=image_use_state(binding.access.usage)
        error:=transition_image(r,slot.command,recording,texture,binding.access.range,state,{.COMPUTE_SHADER},image_access_mask(binding.access))
        if error!=.None { return error }
        arrayed:=false
        for requirement in interface.images { if requirement.group==binding.group && requirement.slot==binding.slot { arrayed=requirement.arrayed; break } }
        view,view_error:=texture_view(r,texture,binding.access.range,arrayed)
        if view_error!=.None { return view_error }
        info:=vk.DescriptorImageInfo{imageView=view,imageLayout=image_layout(state)}
        descriptor:=vk.DescriptorType.STORAGE_IMAGE if binding.access.usage==.Storage else vk.DescriptorType.SAMPLED_IMAGE
        write:=vk.WriteDescriptorSet{sType=.WRITE_DESCRIPTOR_SET,dstSet=sets[binding.group],dstBinding=binding.slot,descriptorCount=1,descriptorType=descriptor,pImageInfo=&info}
        r.table.UpdateDescriptorSets(r.device,1,&write,0,nil)
    }
    for binding in packet.samplers {
        sampler_entry,present:=gfx.storage_get(&r.samplers,binding.handle)
        if !present { return .Invalid_Resource }
        sampler:=sampler_entry^; retain_sampler(slot,sampler)
        info:=vk.DescriptorImageInfo{sampler=sampler.object}
        write:=vk.WriteDescriptorSet{sType=.WRITE_DESCRIPTOR_SET,dstSet=sets[binding.group],dstBinding=binding.slot,descriptorCount=1,descriptorType=.SAMPLER,pImageInfo=&info}
        r.table.UpdateDescriptorSets(r.device,1,&write,0,nil)
    }
    r.table.CmdBindPipeline(slot.command,.COMPUTE,pipeline.object)
    if len(interface.set_layouts)>0 { r.table.CmdBindDescriptorSets(slot.command,.COMPUTE,interface.layout,0,u32(len(interface.set_layouts)),raw_data(sets[:]),0,nil) }
    if packet.indirect.enabled {
        command,present:=resolve_buffer(r,prepared,packet.indirect.command.resource)
        if !present { return .Invalid_Resource }
        r.table.CmdDispatchIndirect(slot.command,command.object,vk.DeviceSize(packet.indirect.command.range.offset))
    } else { r.table.CmdDispatch(slot.command,packet.groups[0],packet.groups[1],packet.groups[2]) }
    for binding in packet.images {
        if gfx.access_writes(binding.access.mode) {
            texture,_:=resolve_texture(r,prepared,binding.access.resource)
            image_mark_contents(recording,r,texture,binding.access.range,true)
        }
    }
    return .None
}
