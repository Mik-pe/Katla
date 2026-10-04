//! Image layout changes remain private until the queue accepts the complete recording.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
Image_Journal :: struct { texture:^Native_Texture, layouts:[]vk.ImageLayout, initialized:[]bool }
@(private="package")
Image_Recording :: struct { journals:[dynamic]Image_Journal }
@(private="package")
image_recording_init :: proc(recording:^Image_Recording,r:^Renderer) { recording.journals=make([dynamic]Image_Journal,r.allocator) }
@(private="package")
image_recording_destroy :: proc(recording:^Image_Recording,r:^Renderer) {
    for journal in recording.journals { delete(journal.layouts,r.allocator); delete(journal.initialized,r.allocator) }
    delete(recording.journals); recording^={}
}
@(private="package")
image_journal :: proc(recording:^Image_Recording,r:^Renderer,texture:^Native_Texture)->^Image_Journal {
    for &journal in recording.journals { if journal.texture==texture { return &journal } }
    layouts:=make([]vk.ImageLayout,len(texture.layouts),r.allocator); copy(layouts,texture.layouts)
    initialized:=make([]bool,len(texture.initialized),r.allocator); copy(initialized,texture.initialized)
    append(&recording.journals,Image_Journal{texture,layouts,initialized})
    return &recording.journals[len(recording.journals)-1]
}
@(private="package")
image_recording_commit :: proc(recording:^Image_Recording,submission:u64) {
    for journal in recording.journals {
        copy(journal.texture.layouts,journal.layouts)
        copy(journal.texture.initialized,journal.initialized)
        journal.texture.latest_submission=submission
    }
}
@(private="package")
resolve_texture :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph,id:gfx.Image_Id)->(^Native_Texture,bool) {
    for input in prepared.textures {
        if input.resource!=id { continue }
        entry,ok:=gfx.storage_get(&r.textures,input.handle)
        if !ok { return nil,false }
        return entry^,true
    }
    return nil,false
}
@(private="package")
image_layout :: proc(state:gfx.Image_State)->vk.ImageLayout {
    switch state {
    case .Undefined: return .UNDEFINED
    case .Shader_Read: return .SHADER_READ_ONLY_OPTIMAL
    case .Storage: return .GENERAL
    case .Color_Attachment: return .COLOR_ATTACHMENT_OPTIMAL
    case .Depth_Attachment: return .DEPTH_STENCIL_ATTACHMENT_OPTIMAL
    case .Transfer_Source: return .TRANSFER_SRC_OPTIMAL
    case .Transfer_Destination: return .TRANSFER_DST_OPTIMAL
    case .Present: return .PRESENT_SRC_KHR
    }
    return .UNDEFINED
}
@(private="package")
image_use_state :: proc(usage:gfx.Texture_Usage)->gfx.Image_State {
    switch usage {
    case .Sampled: return .Shader_Read
    case .Storage: return .Storage
    case .Color_Attachment: return .Color_Attachment
    case .Depth_Attachment: return .Depth_Attachment
    case .Transfer_Source: return .Transfer_Source
    case .Transfer_Destination: return .Transfer_Destination
    case .Present: return .Present
    }
    return .Undefined
}
@(private="package")
image_access_mask :: proc(access:gfx.Image_Access)->vk.AccessFlags2 {
    flags:vk.AccessFlags2
    switch access.usage {
    case .Sampled: return {.SHADER_SAMPLED_READ}
    case .Storage:
        if gfx.access_reads(access.mode) { flags|={.SHADER_STORAGE_READ} }
        if gfx.access_writes(access.mode) { flags|={.SHADER_STORAGE_WRITE} }
    case .Color_Attachment:
        if gfx.access_reads(access.mode) { flags|={.COLOR_ATTACHMENT_READ} }
        if gfx.access_writes(access.mode) { flags|={.COLOR_ATTACHMENT_WRITE} }
    case .Depth_Attachment:
        if gfx.access_reads(access.mode) { flags|={.DEPTH_STENCIL_ATTACHMENT_READ} }
        if gfx.access_writes(access.mode) { flags|={.DEPTH_STENCIL_ATTACHMENT_WRITE} }
    case .Transfer_Source: flags={.TRANSFER_READ}
    case .Transfer_Destination: flags={.TRANSFER_WRITE}
    case .Present: flags={.MEMORY_READ}
    }
    return flags
}
@(private="package")
image_stage :: proc(usage:gfx.Texture_Usage)->vk.PipelineStageFlags2 {
    switch usage {
    case .Sampled,.Storage: return {.VERTEX_SHADER,.FRAGMENT_SHADER,.COMPUTE_SHADER}
    case .Color_Attachment: return {.COLOR_ATTACHMENT_OUTPUT}
    case .Depth_Attachment: return {.EARLY_FRAGMENT_TESTS,.LATE_FRAGMENT_TESTS}
    case .Transfer_Source,.Transfer_Destination: return {.COPY}
    case .Present: return {.ALL_COMMANDS}
    }
    return {.ALL_COMMANDS}
}
@(private="package")
transition_image :: proc(r:^Renderer,command:vk.CommandBuffer,recording:^Image_Recording,texture:^Native_Texture,range:gfx.Image_Range,state:gfx.Image_State,destination_stage:vk.PipelineStageFlags2,destination_access:vk.AccessFlags2)->gfx.Gpu_Error {
    if !gfx.image_range_valid(range,texture.desc) { return .Invalid_Range }
    journal:=image_journal(recording,r,texture)
    layout:=image_layout(state)
    if layout==.UNDEFINED { return .Invalid_Graph }
    for layer in range.base_layer..<range.base_layer+range.layer_count {
        for mip in range.base_mip..<range.base_mip+range.mip_count {
            for aspect in range.aspects {
                index:=texture_state_index(texture,mip,layer,aspect)
                old:=journal.layouts[index]
                barrier:=vk.ImageMemoryBarrier2{sType=.IMAGE_MEMORY_BARRIER_2,srcStageMask={.ALL_COMMANDS},srcAccessMask={.MEMORY_READ,.MEMORY_WRITE} if old!=.UNDEFINED else {},dstStageMask=destination_stage,dstAccessMask=destination_access,oldLayout=old,newLayout=layout,srcQueueFamilyIndex=vk.QUEUE_FAMILY_IGNORED,dstQueueFamilyIndex=vk.QUEUE_FAMILY_IGNORED,image=texture.allocation.object,subresourceRange=image_range({mip,1,layer,1,{aspect}})}
                dependency:=vk.DependencyInfo{sType=.DEPENDENCY_INFO,imageMemoryBarrierCount=1,pImageMemoryBarriers=&barrier}
                r.table.CmdPipelineBarrier2(command,&dependency)
                journal.layouts[index]=layout
            }
        }
    }
    return .None
}
@(private="package")
image_contents :: proc(recording:^Image_Recording,r:^Renderer,texture:^Native_Texture,range:gfx.Image_Range)->bool {
    journal:=image_journal(recording,r,texture)
    for layer in range.base_layer..<range.base_layer+range.layer_count {
        for mip in range.base_mip..<range.base_mip+range.mip_count {
            for aspect in range.aspects { if !journal.initialized[texture_state_index(texture,mip,layer,aspect)] { return false } }
        }
    }
    return true
}
@(private="package")
image_mark_contents :: proc(recording:^Image_Recording,r:^Renderer,texture:^Native_Texture,range:gfx.Image_Range,initialized:bool) {
    journal:=image_journal(recording,r,texture)
    for layer in range.base_layer..<range.base_layer+range.layer_count {
        for mip in range.base_mip..<range.base_mip+range.mip_count {
            for aspect in range.aspects { journal.initialized[texture_state_index(texture,mip,layer,aspect)]=initialized }
        }
    }
}
@(private="package")
encode_image_copy :: proc(r:^Renderer,slot:^Native_Frame,recording:^Image_Recording,prepared:^gfx.Prepared_Graph,packet:gfx.Copy_Image_Buffer)->gfx.Gpu_Error {
    texture,texture_ok:=resolve_texture(r,prepared,packet.source)
    buffer,buffer_ok:=resolve_buffer(r,prepared,packet.destination)
    if !texture_ok || !buffer_ok { return .Invalid_Resource }
    range:=gfx.image_region_range(packet.region)
    if !image_contents(recording,r,texture,range) { return .Invalid_Graph }
    err:=transition_image(r,slot.command,recording,texture,range,.Transfer_Source,{.COPY},{.TRANSFER_READ})
    if err!=.None { return err }
    copies,copy_error:=image_copy_regions(r,texture.desc,packet.region,packet.destination_offset)
    if copy_error!=.None { return copy_error }; defer delete(copies,r.allocator)
    r.table.CmdCopyImageToBuffer(slot.command,texture.allocation.object,.TRANSFER_SRC_OPTIMAL,buffer.object,u32(len(copies)),raw_data(copies))
    return .None
}
@(private="package")
image_recording_imports :: proc(r:^Renderer,recording:^Image_Recording,prepared:^gfx.Prepared_Graph)->gfx.Gpu_Error {
    for image in prepared.images {
        texture,ok:=resolve_texture(r,prepared,image.input.resource)
        if !ok { return .Invalid_Resource }
        range:=gfx.image_full_range(image.desc)
        journal:=image_journal(recording,r,texture)
        if image.imported && image.contract.initial!=.Undefined {
            expected:=image_layout(image.contract.initial)
            for layer in 0..<image.desc.layers {
                for mip in 0..<image.desc.mip_levels {
                    for aspect in range.aspects {
                        index:=texture_state_index(texture,mip,layer,aspect)
                        if journal.layouts[index]!=expected || (image.contract.initialized && !journal.initialized[index]) { return .Invalid_Graph }
                    }
                }
            }
        }
        if !image.imported || !image.contract.initialized { image_mark_contents(recording,r,texture,range,false) }
    }
    return .None
}
@(private="package")
image_alias_handoff :: proc(r:^Renderer,command:vk.CommandBuffer,recording:^Image_Recording,prepared:^gfx.Prepared_Graph,alias:gfx.Alias_Handoff)->gfx.Gpu_Error {
    memory:=vk.MemoryBarrier2{sType=.MEMORY_BARRIER_2,srcStageMask={.ALL_COMMANDS},srcAccessMask={.MEMORY_READ,.MEMORY_WRITE},dstStageMask={.ALL_COMMANDS},dstAccessMask={.MEMORY_READ,.MEMORY_WRITE}}
    dependency:=vk.DependencyInfo{sType=.DEPENDENCY_INFO,memoryBarrierCount=1,pMemoryBarriers=&memory}
    r.table.CmdPipelineBarrier2(command,&dependency)
    #partial switch next in alias.next {
    case gfx.Image_Id:
        texture,ok:=resolve_texture(r,prepared,next)
        if !ok { return .Invalid_Resource }
        journal:=image_journal(recording,r,texture)
        for &layout in journal.layouts { layout=.UNDEFINED }
        image_mark_contents(recording,r,texture,gfx.image_full_range(texture.desc),false)
    }
    return .None
}
@(private="package")
encode_buffer_image_copy :: proc(r:^Renderer,slot:^Native_Frame,recording:^Image_Recording,prepared:^gfx.Prepared_Graph,packet:gfx.Copy_Buffer_Image)->gfx.Gpu_Error {
    buffer,buffer_ok:=resolve_buffer(r,prepared,packet.source)
    texture,texture_ok:=resolve_texture(r,prepared,packet.destination)
    if !buffer_ok || !texture_ok { return .Invalid_Resource }
    range:=gfx.image_region_range(packet.region)
    width,height:=gfx.texture_mip_extent(texture.desc,packet.region.mip)
    full:=packet.region.x==0 && packet.region.y==0 && packet.region.width==width && packet.region.height==height && packet.region.z==0 && packet.region.depth==max(u32(1),texture.desc.depth>>packet.region.mip)
    if !full && !image_contents(recording,r,texture,range) { return .Invalid_Graph }
    error:=transition_image(r,slot.command,recording,texture,range,.Transfer_Destination,{.COPY},{.TRANSFER_WRITE})
    if error!=.None { return error }
    copies,copy_error:=image_copy_regions(r,texture.desc,packet.region,packet.source_offset)
    if copy_error!=.None { return copy_error }; defer delete(copies,r.allocator)
    r.table.CmdCopyBufferToImage(slot.command,buffer.object,texture.allocation.object,.TRANSFER_DST_OPTIMAL,u32(len(copies)),raw_data(copies))
    image_mark_contents(recording,r,texture,range,true)
    return .None
}
