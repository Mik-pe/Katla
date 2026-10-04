//! Transfer diagnostics use driver operands and separately translated packet requirements.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
capture_transfer_buffer :: proc(r:^Renderer,buffer:^Native_Buffer,index:int,range:gfx.Buffer_Range,role,ordinal:u32,region:gfx.Image_Region={},value:u32=0,expected:bool=false) {
    if !r.capture.recording { return }
    event:=gfx.Capture_Event{kind=.Bind_Buffer,pass_index=r.capture_pass,phase_index=r.capture_phase,resource_kind=.Buffer if index>=0 else .Auxiliary,resource_index=index,binding_path=.Transfer,native_index=role,array_index=ordinal,buffer_range=range,offset=range.offset,size=range.size,transfer_region=region,transfer_value=value,emitted=true,label="native transfer buffer operand"}
    if expected { gfx.capture_expect(&r.capture,event) } else { event.object=capture_handle(r,5,u64(buffer.object));event.heap=gfx.capture_object(&r.capture,buffer.heap);event.encoder=r.capture_encoder;gfx.capture_record(&r.capture,event) }
}
@(private="package")
capture_transfer_image :: proc(r:^Renderer,texture:^Native_Texture,index:int,range:gfx.Image_Range,region:gfx.Image_Region,role,ordinal:u32,expected:bool=false) {
    if !r.capture.recording { return }
    event:=gfx.Capture_Event{kind=.Bind_Image,pass_index=r.capture_pass,phase_index=r.capture_phase,resource_kind=.Image if index>=0 else .Auxiliary,resource_index=index,binding_path=.Transfer,native_index=role,array_index=ordinal,image_range=range,transfer_region=region,emitted=true,label="native transfer image operand"}
    if expected { gfx.capture_expect(&r.capture,event) } else { event.object=capture_handle(r,4,u64(texture.allocation.object));event.heap=gfx.capture_object(&r.capture,texture.allocation.heap);event.encoder=r.capture_encoder;gfx.capture_record(&r.capture,event) }
}
@(private="package")
capture_copy_operands :: proc(r:^Renderer,texture:^Native_Texture,buffer:^Native_Buffer,image_index,buffer_index:int,region:gfx.Image_Region,offset:u64,image_role,buffer_role:u32,copy_ordinal:u32,expected:bool) {
    if !r.capture.recording { return }
    layout,valid:=gfx.image_region_layout(region,texture.desc);assert(valid)
    capture_transfer_image(r,texture,image_index,gfx.image_region_range(region),region,image_role,copy_ordinal,expected)
    for slice in 0..<region.depth { for row in 0..<layout.block_rows {
        ordinal:=copy_ordinal*region.depth*u32(layout.block_rows)+slice*u32(layout.block_rows)+u32(row)
        capture_transfer_buffer(r,buffer,buffer_index,{offset+u64(slice)*layout.bytes_per_image+row*layout.bytes_per_row,layout.row_bytes},buffer_role,ordinal,region,expected=expected)
    } }
}
@(private="package")
capture_image_copy_expect :: proc(r:^Renderer,texture:^Native_Texture,buffer:^Native_Buffer,image_index,buffer_index:int,region:gfx.Image_Region,offset:u64,image_role,buffer_role:u32) {
    if !r.capture.recording { return }
    layout,valid:=gfx.image_region_layout(region,texture.desc);assert(valid)
    regular:=layout.bytes_per_image%layout.bytes_per_row==0
    count:=u32(1) if regular else region.depth
    for ordinal in 0..<count {
        native_region:=region;native_region.bytes_per_row=layout.bytes_per_row;native_region.bytes_per_image=layout.bytes_per_image if regular else layout.bytes_per_row*layout.block_rows
        if !regular { native_region.z+=ordinal;native_region.depth=1 }
        capture_copy_operands(r,texture,buffer,image_index,buffer_index,native_region,offset+u64(ordinal)*layout.bytes_per_image,image_role,buffer_role,ordinal,true)
    }
}
@(private="package")
capture_image_copy_observe :: proc(r:^Renderer,texture:^Native_Texture,buffer:^Native_Buffer,image_index,buffer_index:int,copies:[]vk.BufferImageCopy,image_role,buffer_role:u32) {
    if !r.capture.recording { return }
    bw,bh,bytes:=gfx.texture_block_layout(texture.desc.format)
    for copy,ordinal in copies {
        aspect:gfx.Image_Aspect=.Color
        if .DEPTH in copy.imageSubresource.aspectMask { aspect=.Depth } else if .STENCIL in copy.imageSubresource.aspectMask { aspect=.Stencil }
        if aspect!=.Color { bw=1;bh=1;bytes=gfx.image_region_pixel_size(texture.desc.format,aspect) }
        row_texels:=copy.bufferRowLength;if row_texels==0 { row_texels=copy.imageExtent.width }
        image_rows:=copy.bufferImageHeight;if image_rows==0 { image_rows=copy.imageExtent.height }
        row_pitch:=(u64(row_texels)+u64(bw)-1)/u64(bw)*u64(bytes)
        image_pitch:=(u64(image_rows)+u64(bh)-1)/u64(bh)*row_pitch
        region:=gfx.Image_Region{mip=copy.imageSubresource.mipLevel,layer=copy.imageSubresource.baseArrayLayer,x=u32(copy.imageOffset.x),y=u32(copy.imageOffset.y),z=u32(copy.imageOffset.z),width=copy.imageExtent.width,height=copy.imageExtent.height,depth=copy.imageExtent.depth,aspect=aspect,bytes_per_row=row_pitch,bytes_per_image=image_pitch}
        capture_copy_operands(r,texture,buffer,image_index,buffer_index,region,u64(copy.bufferOffset),image_role,buffer_role,u32(ordinal),false)
    }
}
@(private="package")
capture_mip_expect :: proc(r:^Renderer,texture:^Native_Texture,mip,layer,layers,width,height,depth,role,ordinal:u32) {
    if !r.capture.recording { return }
    region:=gfx.Image_Region{mip=mip,layer=layer,width=width,height=height,depth=depth,aspect=.Color}
    capture_transfer_image(r,texture,capture_image_id(r,texture),{base_mip=mip,mip_count=1,base_layer=layer,layer_count=layers,aspects={.Color}},region,role,ordinal,true)
}
@(private="package")
capture_mip_observe :: proc(r:^Renderer,texture:^Native_Texture,subresource:vk.ImageSubresourceLayers,offset:vk.Offset3D,extent:vk.Extent3D,role,ordinal:u32) {
    if !r.capture.recording { return }
    region:=gfx.Image_Region{mip=subresource.mipLevel,layer=subresource.baseArrayLayer,x=u32(offset.x),y=u32(offset.y),z=u32(offset.z),width=extent.width,height=extent.height,depth=extent.depth,aspect=.Color}
    capture_transfer_image(r,texture,capture_image_id(r,texture),{base_mip=subresource.mipLevel,mip_count=1,base_layer=subresource.baseArrayLayer,layer_count=subresource.layerCount,aspects={.Color}},region,role,ordinal)
}
