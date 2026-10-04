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
        capture_buffer_binding_expect(r,binding.access.range,binding.access.resource.index,binding.group,binding.slot,u64(transmute(u32)vk.ShaderStageFlags{.COMPUTE}))
        info:=vk.DescriptorBufferInfo{buffer.object,vk.DeviceSize(binding.access.range.offset),vk.DeviceSize(binding.access.range.size)}
        descriptor:=vk.DescriptorType.STORAGE_BUFFER if binding.access.usage==.Storage else vk.DescriptorType.UNIFORM_BUFFER
        write:=vk.WriteDescriptorSet{sType=.WRITE_DESCRIPTOR_SET,dstSet=sets[binding.group],dstBinding=binding.slot,descriptorCount=1,descriptorType=descriptor,pBufferInfo=&info}
        r.table.UpdateDescriptorSets(r.device,1,&write,0,nil)
        capture_buffer_binding(r,buffer,{u64(info.offset),u64(info.range)},binding.group,write.dstBinding,write.dstSet,"vkUpdateDescriptorSets compute buffer",u64(transmute(u32)vk.ShaderStageFlags{.COMPUTE}))
    }
    transition_error:=transition_bound_images(r,slot,recording,prepared,packet.images,true)
    if transition_error!=.None { return transition_error }
    for binding in packet.images {
        for requirement in interface.images {
            if requirement.group!=binding.group || requirement.slot!=binding.slot { continue }
            error:=write_image_descriptors(r,prepared,binding,requirement,sets[binding.group])
            if error!=.None { return error }
            break
        }
    }
    for binding in packet.samplers {
        sampler_entry,present:=gfx.storage_get(&r.samplers,binding.handle)
        if !present { return .Invalid_Resource }
        sampler:=sampler_entry^; retain_sampler(slot,sampler)
        if r.capture.recording { gfx.capture_expect(&r.capture,{kind=.Bind_Sampler,pass_index=r.capture_pass,phase_index=-1,resource_index=-1,group=binding.group,binding=binding.slot,binding_stages=u64(transmute(u32)vk.ShaderStageFlags{.COMPUTE}),emitted=true,label="translated compute sampler binding"}) }
        info:=vk.DescriptorImageInfo{sampler=sampler.object}
        write:=vk.WriteDescriptorSet{sType=.WRITE_DESCRIPTOR_SET,dstSet=sets[binding.group],dstBinding=binding.slot,descriptorCount=1,descriptorType=.SAMPLER,pImageInfo=&info}
        r.table.UpdateDescriptorSets(r.device,1,&write,0,nil)
        if r.capture.recording { gfx.capture_record(&r.capture,{kind=.Bind_Sampler,pass_index=r.capture_pass,phase_index=-1,resource_index=-1,object=capture_handle(r,6,u64(sampler.object)),encoder=r.capture_encoder,table=capture_handle(r,1,u64(write.dstSet)),group=binding.group,binding=write.dstBinding,binding_stages=u64(transmute(u32)vk.ShaderStageFlags{.COMPUTE}),emitted=true,label="vkUpdateDescriptorSets compute sampler"}) }
    }
    capture_pipeline_expect(r,entry^.object,entry^.interface.layout,.COMPUTE)
    r.table.CmdBindPipeline(slot.command,.COMPUTE,pipeline.object)
    capture_pipeline_binding(r,pipeline.object,interface.layout,.COMPUTE,"vkCmdBindPipeline compute")
    if len(interface.set_layouts)>0 { r.table.CmdBindDescriptorSets(slot.command,.COMPUTE,interface.layout,0,u32(len(interface.set_layouts)),raw_data(sets[:]),0,nil);capture_descriptor_tables(r,sets[:len(interface.set_layouts)],interface.layout) }
    if packet.indirect.enabled {
        command,present:=resolve_buffer(r,prepared,packet.indirect.command.resource)
        if !present { return .Invalid_Resource }
        capture_buffer_binding_expect(r,packet.indirect.command.range,packet.indirect.command.resource.index,0,0,path=.Indirect)
        r.table.CmdDispatchIndirect(slot.command,command.object,vk.DeviceSize(packet.indirect.command.range.offset))
        capture_buffer_binding(r,command,packet.indirect.command.range,0,0,label="vkCmdDispatchIndirect",path=.Indirect)
    } else { r.table.CmdDispatch(slot.command,packet.groups[0],packet.groups[1],packet.groups[2]) }
    mark_bound_images(r,recording,prepared,packet.images)
    return .None
}
