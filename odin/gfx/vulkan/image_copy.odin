//! Transfer regions preserve caller row and slice pitches without repacking their buffers.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
image_copy_regions :: proc(r:^Renderer,desc:gfx.Texture_Desc,region:gfx.Image_Region,offset:u64)->([]vk.BufferImageCopy,gfx.Gpu_Error) {
    layout,valid:=gfx.image_region_layout(region,desc)
    if !valid { return nil,.Invalid_Range }
    bw,bh,bytes:=gfx.texture_block_layout(desc.format)
    if region.aspect!=.Color { bw=1; bh=1; bytes=gfx.image_region_pixel_size(desc.format,region.aspect) }
    alignment:=u64(bytes) if region.aspect==.Color else u64(4)
    if offset%alignment!=0 || layout.required_bytes>max(u64)-offset || layout.bytes_per_row>0x7fff_ffff { return nil,.Invalid_Range }
    row_length:=layout.bytes_per_row/u64(bytes)*u64(bw)
    if row_length>u64(max(u32)) { return nil,.Invalid_Range }
    regular:=layout.bytes_per_image%layout.bytes_per_row==0
    image_height:=layout.bytes_per_image/layout.bytes_per_row*u64(bh)
    if regular && image_height>u64(max(u32)) { return nil,.Invalid_Range }
    count:=1 if regular else int(region.depth)
    copies:=make([]vk.BufferImageCopy,count,r.allocator)
    for index in 0..<count {
        slice_offset:=offset+u64(index)*layout.bytes_per_image
        if slice_offset%alignment!=0 { delete(copies,r.allocator); return nil,.Invalid_Range }
        copies[index]={bufferOffset=vk.DeviceSize(slice_offset),bufferRowLength=u32(row_length),bufferImageHeight=u32(image_height) if regular else 0,imageSubresource={image_aspects({region.aspect}),region.mip,region.layer,1},imageOffset={i32(region.x),i32(region.y),i32(region.z)+i32(index)},imageExtent={region.width,region.height,region.depth if regular else 1}}
    }
    return copies,.None
}
@(private="package")
encode_generate_mips :: proc(r:^Renderer,slot:^Native_Frame,recording:^Image_Recording,prepared:^gfx.Prepared_Graph,packet:gfx.Generate_Mips)->gfx.Gpu_Error {
    texture,present:=resolve_texture(r,prepared,packet.resource)
    if !present { return .Invalid_Resource }
    properties:vk.FormatProperties
    r.instance_api.GetPhysicalDeviceFormatProperties(r.physical,texture_format(texture.desc.format),&properties)
    required:vk.FormatFeatureFlags={.BLIT_SRC,.BLIT_DST,.SAMPLED_IMAGE_FILTER_LINEAR}
    if properties.optimalTilingFeatures&required!=required { return .Unsupported }
    first:=packet.range; first.mip_count=1
    if !image_contents(recording,r,texture,first) { return .Invalid_Graph }
    for level in packet.range.base_mip+1..<packet.range.base_mip+packet.range.mip_count {
        source:=packet.range; source.base_mip=level-1; source.mip_count=1
        destination:=source; destination.base_mip=level
        error:=transition_image(r,slot.command,recording,texture,source,.Transfer_Source,{.ALL_TRANSFER},{.TRANSFER_READ})
        if error!=.None { return error }
        error=transition_image(r,slot.command,recording,texture,destination,.Transfer_Destination,{.ALL_TRANSFER},{.TRANSFER_WRITE})
        if error!=.None { return error }
        sw,sh,sd:=gfx.texture_mip_volume(texture.desc,level-1)
        dw,dh,dd:=gfx.texture_mip_volume(texture.desc,level)
        if texture.desc.depth>1 {
            error=encode_volume_mip(r,slot,recording,texture,packet.range,level)
            if error!=.None { return error }; continue
        }
        region:=vk.ImageBlit2{sType=.IMAGE_BLIT_2,srcSubresource={{.COLOR},level-1,packet.range.base_layer,packet.range.layer_count},srcOffsets={{0,0,0},{i32(sw),i32(sh),i32(sd)}},dstSubresource={{.COLOR},level,packet.range.base_layer,packet.range.layer_count},dstOffsets={{0,0,0},{i32(dw),i32(dh),i32(dd)}}}
        info:=vk.BlitImageInfo2{sType=.BLIT_IMAGE_INFO_2,srcImage=texture.allocation.object,srcImageLayout=.TRANSFER_SRC_OPTIMAL,dstImage=texture.allocation.object,dstImageLayout=.TRANSFER_DST_OPTIMAL,regionCount=1,pRegions=&region,filter=.LINEAR}
        capture_mip_expect(r,texture,level-1,packet.range.base_layer,packet.range.layer_count,sw,sh,sd,0,level)
        capture_mip_expect(r,texture,level,packet.range.base_layer,packet.range.layer_count,dw,dh,dd,1,level)
        r.table.CmdBlitImage2(slot.command,&info)
        capture_mip_observe(r,texture,region.srcSubresource,region.srcOffsets[0],{u32(region.srcOffsets[1].x),u32(region.srcOffsets[1].y),u32(region.srcOffsets[1].z)},0,level)
        capture_mip_observe(r,texture,region.dstSubresource,region.dstOffsets[0],{u32(region.dstOffsets[1].x),u32(region.dstOffsets[1].y),u32(region.dstOffsets[1].z)},1,level)
        image_mark_contents(recording,r,texture,destination,true)
    }
    return .None
}
