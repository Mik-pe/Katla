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
        used:=false; for requirement in pipeline.buffers { if requirement.group==binding.group && requirement.slot==binding.slot { used=true; break } }
        if !used { continue }
        buffer,present:=resolve_buffer(r,prepared,binding.access.resource)
        if !present { return {},.Invalid_Resource }
        info:=vk.DescriptorBufferInfo{buffer.object,vk.DeviceSize(binding.access.range.offset),vk.DeviceSize(binding.access.range.size)}
        descriptor:=vk.DescriptorType.STORAGE_BUFFER if binding.access.usage==.Storage else vk.DescriptorType.UNIFORM_BUFFER
        write:=vk.WriteDescriptorSet{sType=.WRITE_DESCRIPTOR_SET,dstSet=sets[binding.group],dstBinding=binding.slot,descriptorCount=1,descriptorType=descriptor,pBufferInfo=&info}
        r.table.UpdateDescriptorSets(r.device,1,&write,0,nil)
    }
    for binding in packet.images {
        for requirement in pipeline.images {
            if requirement.group!=binding.group || requirement.slot!=binding.slot { continue }
            error:=write_image_descriptors(r,prepared,binding,requirement,sets[binding.group])
            if error!=.None { return {},error }
            break
        }
    }
    for binding in packet.samplers {
        used:=false; for requirement in pipeline.samplers { if requirement.group==binding.group && requirement.slot==binding.slot { used=true; break } }
        if !used { continue }
        sampler_entry,present:=gfx.storage_get(&r.samplers,binding.handle)
        if !present { return {},.Invalid_Resource }
        sampler:=sampler_entry^
        retain_sampler(slot,sampler)
        info:=vk.DescriptorImageInfo{sampler=sampler.object}
        write:=vk.WriteDescriptorSet{sType=.WRITE_DESCRIPTOR_SET,dstSet=sets[binding.group],dstBinding=binding.slot,descriptorCount=1,descriptorType=.SAMPLER,pImageInfo=&info}
        r.table.UpdateDescriptorSets(r.device,1,&write,0,nil)
    }
    for constant in phase.constants {
        used:=false; for requirement in pipeline.buffers { if requirement.group==constant.group && requirement.slot==constant.slot { used=true; break } }
        if !used { continue }
        buffer,offset,error:=upload_constant(r,slot,constant.bytes)
        if error!=.None { return {},error }
        info:=vk.DescriptorBufferInfo{buffer.object,vk.DeviceSize(offset),vk.DeviceSize(len(constant.bytes))}
        descriptor:=vk.DescriptorType.STORAGE_BUFFER if constant.usage==.Storage else vk.DescriptorType.UNIFORM_BUFFER
        write:=vk.WriteDescriptorSet{sType=.WRITE_DESCRIPTOR_SET,dstSet=sets[constant.group],dstBinding=constant.slot,descriptorCount=1,descriptorType=descriptor,pBufferInfo=&info}
        r.table.UpdateDescriptorSets(r.device,1,&write,0,nil)
    }
    return {pipeline,sets,phase.viewport,phase.scissor,phase.draws},.None
}
@(private="package")
Native_Phase :: struct { pipeline:^Native_Graphics_Pipeline, sets:[32]vk.DescriptorSet, viewport:gfx.Viewport, scissor:gfx.Scissor, draws:[]gfx.Draw_Op }
@(private="package")
encode_render :: proc(r:^Renderer,slot:^Native_Frame,recording:^Image_Recording,prepared:^gfx.Prepared_Graph,packet:gfx.Render)->gfx.Gpu_Error {
    transition_error:=transition_bound_images(r,slot,recording,prepared,packet.images,false)
    if transition_error!=.None { return transition_error }
    phases:=make([]Native_Phase,len(packet.phases),r.allocator); defer delete(phases,r.allocator)
    for phase,i in packet.phases {
        native,error:=phase_descriptors(r,slot,prepared,packet,phase)
        if error!=.None { return error }
        phases[i]=native
    }
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
    for &phase in phases {
        r.table.CmdBindPipeline(slot.command,.GRAPHICS,phase.pipeline.object)
        if len(phase.pipeline.set_layouts)>0 { r.table.CmdBindDescriptorSets(slot.command,.GRAPHICS,phase.pipeline.layout,0,u32(len(phase.pipeline.set_layouts)),raw_data(phase.sets[:]),0,nil) }
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
    for attachment in packet.colors {
        texture,_:=resolve_texture(r,prepared,attachment.access.resource)
        image_mark_contents(recording,r,texture,attachment.access.range,attachment.store==.Store && attachment.load!=.Discard)
    }
    if packet.depth.enabled {
        texture,_:=resolve_texture(r,prepared,packet.depth.access.resource)
        image_mark_contents(recording,r,texture,packet.depth.access.range,packet.depth.store==.Store && packet.depth.load!=.Discard)
    }
    mark_bound_images(r,recording,prepared,packet.images)
    return .None
}
