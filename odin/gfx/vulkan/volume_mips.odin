//! Volume filtering uses mip-zero scratch images with correctly normalized three-dimensional coordinates.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
Native_Mip_Scratch :: struct { texture:^Native_Texture, used:bool }
@(private="package")
mip_scratch :: proc(r:^Renderer,slot:^Native_Frame,desc:gfx.Texture_Desc)->(^Native_Texture,gfx.Gpu_Error) {
    for &scratch in slot.mip_scratch {
        if scratch.used || scratch.texture.desc!=desc { continue }
        scratch.used=true; scratch.texture.refs+=1; append(&slot.textures,scratch.texture)
        return scratch.texture,.None
    }
    info,error:=texture_create_info(r,desc)
    if error!=.None { return nil,error }
    info.imageType=.D3
    allocation,allocation_error:=image_memory_allocate(r,&info)
    if allocation_error!=.None { return nil,allocation_error }
    texture:=new(Native_Texture,r.allocator); texture^={allocation=allocation,desc=desc,refs=2}
    texture.views=make([dynamic]Native_Image_View,r.allocator)
    texture.layouts=make([]vk.ImageLayout,3,r.allocator); texture.initialized=make([]bool,3,r.allocator)
    append(&slot.mip_scratch,Native_Mip_Scratch{texture,true}); append(&slot.textures,texture)
    return texture,.None
}
@(private="package")
copy_mip_image :: proc(r:^Renderer,command:vk.CommandBuffer,source,destination:^Native_Texture,source_mip,destination_mip:u32,width,height,depth:u32) {
    region:=vk.ImageCopy2{sType=.IMAGE_COPY_2,srcSubresource={{.COLOR},source_mip,0,1},dstSubresource={{.COLOR},destination_mip,0,1},extent={width,height,depth}}
    info:=vk.CopyImageInfo2{sType=.COPY_IMAGE_INFO_2,srcImage=source.allocation.object,srcImageLayout=.TRANSFER_SRC_OPTIMAL,dstImage=destination.allocation.object,dstImageLayout=.TRANSFER_DST_OPTIMAL,regionCount=1,pRegions=&region}
    r.table.CmdCopyImage2(command,&info)
}
@(private="package")
encode_volume_mip :: proc(r:^Renderer,slot:^Native_Frame,recording:^Image_Recording,texture:^Native_Texture,range:gfx.Image_Range,level:u32)->gfx.Gpu_Error {
    sw,sh,sd:=gfx.texture_mip_volume(texture.desc,level-1)
    dw,dh,dd:=gfx.texture_mip_volume(texture.desc,level)
    source_desc:=gfx.Texture_Desc{sw,sh,1,1,texture.desc.format,{.Transfer_Source,.Transfer_Destination},sd}
    destination_desc:=gfx.Texture_Desc{dw,dh,1,1,texture.desc.format,{.Transfer_Source,.Transfer_Destination},dd}
    source,error:=mip_scratch(r,slot,source_desc)
    if error!=.None { return error }
    destination,destination_error:=mip_scratch(r,slot,destination_desc)
    if destination_error!=.None { return destination_error }
    source_range:=gfx.image_full_range(source.desc)
    destination_range:=gfx.image_full_range(destination.desc)
    error=transition_image(r,slot.command,recording,source,source_range,.Transfer_Destination,{.COPY},{.TRANSFER_WRITE})
    if error!=.None { return error }
    copy_mip_image(r,slot.command,texture,source,level-1,0,sw,sh,sd)
    image_mark_contents(recording,r,source,source_range,true)
    error=transition_image(r,slot.command,recording,source,source_range,.Transfer_Source,{.BLIT},{.TRANSFER_READ})
    if error!=.None { return error }
    error=transition_image(r,slot.command,recording,destination,destination_range,.Transfer_Destination,{.BLIT},{.TRANSFER_WRITE})
    if error!=.None { return error }
    region:=vk.ImageBlit2{sType=.IMAGE_BLIT_2,srcSubresource={{.COLOR},0,0,1},srcOffsets={{0,0,0},{i32(sw),i32(sh),i32(sd)}},dstSubresource={{.COLOR},0,0,1},dstOffsets={{0,0,0},{i32(dw),i32(dh),i32(dd)}}}
    info:=vk.BlitImageInfo2{sType=.BLIT_IMAGE_INFO_2,srcImage=source.allocation.object,srcImageLayout=.TRANSFER_SRC_OPTIMAL,dstImage=destination.allocation.object,dstImageLayout=.TRANSFER_DST_OPTIMAL,regionCount=1,pRegions=&region,filter=.LINEAR}
    r.table.CmdBlitImage2(slot.command,&info)
    image_mark_contents(recording,r,destination,destination_range,true)
    error=transition_image(r,slot.command,recording,destination,destination_range,.Transfer_Source,{.COPY},{.TRANSFER_READ})
    if error!=.None { return error }
    copy_mip_image(r,slot.command,destination,texture,0,level,dw,dh,dd)
    target:=range; target.base_mip=level; target.mip_count=1
    image_mark_contents(recording,r,texture,target,true)
    return .None
}
