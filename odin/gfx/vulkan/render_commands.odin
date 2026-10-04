//! Dynamic rendering consumes explicit attachments and immutable graphics descriptors.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
attachment_load :: proc(load:gfx.Load_Op)->vk.AttachmentLoadOp {
    switch load {
    case .Load: return .LOAD
    case .Clear: return .CLEAR
    case .Discard: return .DONT_CARE
    }
    return .DONT_CARE
}
@(private="package")
attachment_store :: proc(store:gfx.Store_Op)->vk.AttachmentStoreOp { return .STORE if store==.Store else .DONT_CARE }
@(private="package")
retain_graphics :: proc(slot:^Native_Frame,pipeline:^Native_Graphics_Pipeline) {
    for prior in slot.graphics { if prior==pipeline { return } }
    pipeline.refs+=1; append(&slot.graphics,pipeline)
}
@(private="package")
retain_sampler :: proc(slot:^Native_Frame,sampler:^Native_Sampler) {
    for prior in slot.samplers { if prior==sampler { return } }
    sampler.refs+=1; append(&slot.samplers,sampler)
}
@(private="package")
phase_descriptors :: proc(r:^Renderer,slot:^Native_Frame,prepared:^gfx.Prepared_Graph,packet:gfx.Render,phase:gfx.Render_Phase)->(Native_Phase,gfx.Gpu_Error) {
    pipeline:^Native_Graphics_Pipeline
    sets:[32]vk.DescriptorSet
    if phase.pipeline.owner!=nil {
        entry,ok:=gfx.storage_get(&r.graphics,phase.pipeline)
        if !ok { return {},.Invalid_Resource }
        pipeline=entry^
        retain_graphics(slot,pipeline)
        if len(pipeline.set_layouts)>0 {
            error:=allocate_descriptor_sets(r,slot,pipeline.set_layouts,raw_data(sets[:]))
            if error!=.None { return {},error }
        }
    }
    for binding in packet.buffers {
        used:=false;visibility:u64; for requirement in pipeline.buffers { if requirement.group==binding.group && requirement.slot==binding.slot { used=true;visibility=u64(transmute(u32)shader_stages(requirement.stages)); break } }
        if !used { continue }
        buffer,present:=resolve_buffer(r,prepared,binding.access.resource)
        if !present { return {},.Invalid_Resource }
        capture_buffer_binding_expect(r,binding.access.range,binding.access.resource.index,binding.group,binding.slot,visibility)
        info:=vk.DescriptorBufferInfo{buffer.object,vk.DeviceSize(binding.access.range.offset),vk.DeviceSize(binding.access.range.size)}
        descriptor:=vk.DescriptorType.STORAGE_BUFFER if binding.access.usage==.Storage else vk.DescriptorType.UNIFORM_BUFFER
        write:=vk.WriteDescriptorSet{sType=.WRITE_DESCRIPTOR_SET,dstSet=sets[binding.group],dstBinding=binding.slot,descriptorCount=1,descriptorType=descriptor,pBufferInfo=&info}
        r.table.UpdateDescriptorSets(r.device,1,&write,0,nil)
        capture_buffer_binding(r,buffer,{u64(info.offset),u64(info.range)},binding.group,write.dstBinding,write.dstSet,"vkUpdateDescriptorSets buffer",visibility)
    }
    for binding in phase.images {
        for requirement in pipeline.images {
            if requirement.group!=binding.group || requirement.slot!=binding.slot { continue }
            error:=write_image_descriptors(r,prepared,binding,requirement,sets[binding.group])
            if error!=.None { return {},error }
            break
        }
    }
    for binding in phase.samplers {
        used:=false;visibility:u64; for requirement in pipeline.samplers { if requirement.group==binding.group && requirement.slot==binding.slot { used=true;visibility=u64(transmute(u32)shader_stages(requirement.stages)); break } }
        if !used { continue }
        sampler_entry,present:=gfx.storage_get(&r.samplers,binding.handle)
        if !present { return {},.Invalid_Resource }
        sampler:=sampler_entry^
        retain_sampler(slot,sampler)
        if r.capture.recording { gfx.capture_expect(&r.capture,{kind=.Bind_Sampler,pass_index=r.capture_pass,phase_index=r.capture_phase,resource_index=-1,group=binding.group,binding=binding.slot,binding_stages=visibility,emitted=true,label="translated sampler binding"}) }
        info:=vk.DescriptorImageInfo{sampler=sampler.object}
        write:=vk.WriteDescriptorSet{sType=.WRITE_DESCRIPTOR_SET,dstSet=sets[binding.group],dstBinding=binding.slot,descriptorCount=1,descriptorType=.SAMPLER,pImageInfo=&info}
        r.table.UpdateDescriptorSets(r.device,1,&write,0,nil)
        if r.capture.recording { gfx.capture_record(&r.capture,{kind=.Bind_Sampler,pass_index=r.capture_pass,phase_index=r.capture_phase,resource_index=-1,object=capture_handle(r,6,u64(sampler.object)),encoder=r.capture_encoder,table=capture_handle(r,1,u64(write.dstSet)),group=binding.group,binding=write.dstBinding,binding_stages=visibility,emitted=true,label="vkUpdateDescriptorSets sampler"}) }
    }
    for constant in phase.constants {
        used:=false;visibility:u64; for requirement in pipeline.buffers { if requirement.group==constant.group && requirement.slot==constant.slot { used=true;visibility=u64(transmute(u32)shader_stages(requirement.stages)); break } }
        if !used { continue }
        buffer,offset,error:=upload_constant(r,slot,constant.bytes)
        if error!=.None { return {},error }
        capture_buffer_binding_expect(r,{offset,u64(len(constant.bytes))},-1,constant.group,constant.slot,visibility)
        info:=vk.DescriptorBufferInfo{buffer.object,vk.DeviceSize(offset),vk.DeviceSize(len(constant.bytes))}
        descriptor:=vk.DescriptorType.STORAGE_BUFFER if constant.usage==.Storage else vk.DescriptorType.UNIFORM_BUFFER
        write:=vk.WriteDescriptorSet{sType=.WRITE_DESCRIPTOR_SET,dstSet=sets[constant.group],dstBinding=constant.slot,descriptorCount=1,descriptorType=descriptor,pBufferInfo=&info}
        r.table.UpdateDescriptorSets(r.device,1,&write,0,nil)
        capture_buffer_binding(r,buffer,{u64(info.offset),u64(info.range)},constant.group,write.dstBinding,write.dstSet,"vkUpdateDescriptorSets immutable constant",visibility)
    }
    return {pipeline,sets,phase.viewport,phase.scissor,phase.draws},.None
}
@(private="package")
Native_Phase :: struct { pipeline:^Native_Graphics_Pipeline, sets:[32]vk.DescriptorSet, viewport:gfx.Viewport, scissor:gfx.Scissor, draws:[]gfx.Draw_Op }
@(private="package")
encode_render :: proc(r:^Renderer,slot:^Native_Frame,recording:^Image_Recording,prepared:^gfx.Prepared_Graph,packet:gfx.Render)->gfx.Gpu_Error {
    for phase,i in packet.phases {
        r.capture_phase=i
        transition_error:=transition_bound_images(r,slot,recording,prepared,phase.images,false)
        if transition_error!=.None { return transition_error }
    }
    phases:=make([]Native_Phase,len(packet.phases),r.allocator); defer delete(phases,r.allocator)
    for phase,i in packet.phases {
        r.capture_phase=i
        native,error:=phase_descriptors(r,slot,prepared,packet,phase)
        if error!=.None { return error }
        phases[i]=native
    }
    r.capture_phase=-1
    colors:[8]vk.RenderingAttachmentInfo
    width,height:u32
    for attachment,i in packet.colors {
        texture,present:=resolve_texture(r,prepared,attachment.access.resource)
        if !present { return .Invalid_Resource }
        if attachment.load==.Load && !image_contents(recording,r,texture,attachment.access.range) { return .Invalid_Graph }
        err:=transition_image(r,slot.command,recording,texture,attachment.access.range,.Color_Attachment,{.COLOR_ATTACHMENT_OUTPUT},image_access_mask(attachment.access))
        if err!=.None { return err }
        view,view_error:=texture_view(r,texture,attachment.access.range)
        if view_error!=.None { return view_error }
        width,height=gfx.texture_mip_extent(texture.desc,attachment.access.range.base_mip)
        clear:vk.ClearValue
        for channel in 0..<4 { clear.color.float32[channel]=f32(attachment.clear[channel]) }
        colors[i]={sType=.RENDERING_ATTACHMENT_INFO,imageView=view,imageLayout=.COLOR_ATTACHMENT_OPTIMAL,loadOp=attachment_load(attachment.load),storeOp=attachment_store(attachment.store),clearValue=clear}
    }
    depth:vk.RenderingAttachmentInfo
    depth_ptr,stencil_ptr:^vk.RenderingAttachmentInfo
    if packet.depth.enabled {
        attachment:=packet.depth
        texture,present:=resolve_texture(r,prepared,attachment.access.resource)
        if !present { return .Invalid_Resource }
        if attachment.load==.Load && !image_contents(recording,r,texture,attachment.access.range) { return .Invalid_Graph }
        err:=transition_image(r,slot.command,recording,texture,attachment.access.range,.Depth_Attachment,{.EARLY_FRAGMENT_TESTS,.LATE_FRAGMENT_TESTS},image_access_mask(attachment.access))
        if err!=.None { return err }
        view,view_error:=texture_view(r,texture,attachment.access.range)
        if view_error!=.None { return view_error }
        width,height=gfx.texture_mip_extent(texture.desc,attachment.access.range.base_mip)
        clear:vk.ClearValue; clear.depthStencil={f32(attachment.clear_depth),attachment.clear_stencil}
        depth={sType=.RENDERING_ATTACHMENT_INFO,imageView=view,imageLayout=.DEPTH_STENCIL_ATTACHMENT_OPTIMAL,loadOp=attachment_load(attachment.load),storeOp=attachment_store(attachment.store),clearValue=clear}
        if .Depth in attachment.access.range.aspects { depth_ptr=&depth }
        if .Stencil in attachment.access.range.aspects { stencil_ptr=&depth }
    }
    rendering:=vk.RenderingInfo{sType=.RENDERING_INFO,renderArea={{0,0},{width,height}},layerCount=1,colorAttachmentCount=u32(len(packet.colors)),pColorAttachments=raw_data(colors[:]),pDepthAttachment=depth_ptr,pStencilAttachment=stencil_ptr}
    r.table.CmdBeginRendering(slot.command,&rendering)
    if r.capture.recording {
        for attachment,i in packet.colors {
            texture,_:=resolve_texture(r,prepared,attachment.access.resource);native:=colors[i]
            event:=gfx.Capture_Event{kind=.Attachment,pass_index=r.capture_pass,phase_index=-1,resource_kind=.Image,resource_index=attachment.access.resource.index,object=capture_handle(r,4,u64(texture.allocation.object)),encoder=r.capture_encoder,image_range=attachment.access.range,native_index=u32(i),native_load=u64(native.loadOp),native_store=u64(native.storeOp),new_layout=u64(native.imageLayout),emitted=true,label="vkCmdBeginRendering color attachment"}
            for channel in 0..<4 { event.clear_color[channel]=f64(native.clearValue.color.float32[channel]) }
            gfx.capture_record(&r.capture,event)
            gfx.capture_expect(&r.capture,{kind=.Attachment,pass_index=r.capture_pass,phase_index=-1,resource_kind=.Image,resource_index=attachment.access.resource.index,image_range=attachment.access.range,native_index=u32(i),native_load=u64(attachment_load(attachment.load)),native_store=u64(attachment_store(attachment.store)),clear_color={f64(f32(attachment.clear[0])),f64(f32(attachment.clear[1])),f64(f32(attachment.clear[2])),f64(f32(attachment.clear[3]))},new_layout=u64(vk.ImageLayout.COLOR_ATTACHMENT_OPTIMAL),emitted=true,label="translated color attachment"})
        }
        if packet.depth.enabled {
            texture,_:=resolve_texture(r,prepared,packet.depth.access.resource)
            gfx.capture_record(&r.capture,{kind=.Attachment,pass_index=r.capture_pass,phase_index=-1,resource_kind=.Image,resource_index=packet.depth.access.resource.index,object=capture_handle(r,4,u64(texture.allocation.object)),encoder=r.capture_encoder,image_range=packet.depth.access.range,native_load=u64(depth.loadOp),native_store=u64(depth.storeOp),clear_depth=f64(depth.clearValue.depthStencil.depth),clear_stencil=depth.clearValue.depthStencil.stencil,new_layout=u64(depth.imageLayout),emitted=true,label="vkCmdBeginRendering depth/stencil attachment"})
            gfx.capture_expect(&r.capture,{kind=.Attachment,pass_index=r.capture_pass,phase_index=-1,resource_kind=.Image,resource_index=packet.depth.access.resource.index,image_range=packet.depth.access.range,native_load=u64(attachment_load(packet.depth.load)),native_store=u64(attachment_store(packet.depth.store)),clear_depth=f64(f32(packet.depth.clear_depth)),clear_stencil=packet.depth.clear_stencil,new_layout=u64(vk.ImageLayout.DEPTH_STENCIL_ATTACHMENT_OPTIMAL),emitted=true,label="translated depth/stencil attachment"})
        }
    }
    for &phase,i in phases {
        r.capture_phase=i
        if requested,ok:=gfx.storage_get(&r.graphics,packet.phases[i].pipeline);ok { capture_pipeline_expect(r,requested^.object,requested^.layout,.GRAPHICS) }
        r.table.CmdBindPipeline(slot.command,.GRAPHICS,phase.pipeline.object)
        capture_pipeline_binding(r,phase.pipeline.object,phase.pipeline.layout,.GRAPHICS,"vkCmdBindPipeline graphics")
        if len(phase.pipeline.set_layouts)>0 { r.table.CmdBindDescriptorSets(slot.command,.GRAPHICS,phase.pipeline.layout,0,u32(len(phase.pipeline.set_layouts)),raw_data(phase.sets[:]),0,nil);capture_descriptor_tables(r,phase.sets[:len(phase.pipeline.set_layouts)],phase.pipeline.layout) }
        viewport:=vk.Viewport{0,0,f32(width),f32(height),0,1}
        if phase.viewport.enabled { viewport={f32(phase.viewport.x),f32(phase.viewport.y),f32(phase.viewport.width),f32(phase.viewport.height),f32(phase.viewport.min_depth),f32(phase.viewport.max_depth)} }
        scissor:=vk.Rect2D{{0,0},{width,height}}
        if phase.scissor.enabled { scissor={{i32(phase.scissor.x),i32(phase.scissor.y)},{phase.scissor.width,phase.scissor.height}} }
        r.table.CmdSetViewport(slot.command,0,1,&viewport); r.table.CmdSetScissor(slot.command,0,1,&scissor)
        for draw in phase.draws {
            error:=encode_draw(r,slot,prepared,draw)
            if error!=.None { r.table.CmdEndRendering(slot.command); return error }
        }
    }
    r.table.CmdEndRendering(slot.command)
    r.capture_phase=-1
    for attachment in packet.colors {
        texture,_:=resolve_texture(r,prepared,attachment.access.resource)
        image_mark_contents(recording,r,texture,attachment.access.range,attachment.store==.Store && attachment.load!=.Discard)
    }
    if packet.depth.enabled {
        texture,_:=resolve_texture(r,prepared,packet.depth.access.resource)
        image_mark_contents(recording,r,texture,packet.depth.access.range,packet.depth.store==.Store && packet.depth.load!=.Discard)
    }
    for phase in packet.phases { mark_bound_images(r,recording,prepared,phase.images) }
    return .None
}
