#+build darwin, arm64
//! Transfer operands describe native command arguments without inventing shader descriptors.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"

@(private="package")
capture_transfer_record :: proc(r:^Renderer,slot:^Native_Frame,object:^NS.Object,event:gfx.Capture_Event) {
    if !r.capture.recording { return }
    value:=event;value.binding_path=.Transfer;value.pass_index=slot.capture_pass;value.phase_index= -1
    value.encoder=slot.capture_encoder;value.object=gfx.capture_object(&r.capture,object);value.emitted=true
    gfx.capture_record(&r.capture,value)
}
@(private="package")
capture_transfer_buffer :: proc(r:^Renderer,slot:^Native_Frame,object:^NS.Object,resource:gfx.Resource_Id,range:gfx.Buffer_Range,role:u32,value:u32=0,region:gfx.Image_Region={},row:u32=0) {
    capture_transfer_record(r,slot,object,{kind=.Bind_Buffer,resource_kind=.Buffer,resource_index=resource.index,buffer_range=range,native_index=role,array_index=row,transfer_value=value,transfer_region=region,label="native transfer buffer operand"})
}
@(private="package")
capture_transfer_image :: proc(r:^Renderer,slot:^Native_Frame,object:^NS.Object,resource:gfx.Image_Id,range:gfx.Image_Range,role:u32,region:gfx.Image_Region={}) {
    capture_transfer_record(r,slot,object,{kind=.Bind_Image,resource_kind=.Image,resource_index=resource.index,image_range=range,native_index=role,transfer_region=region,label="native transfer texture operand"})
}
@(private="package")
capture_transfer_rows :: proc(r:^Renderer,slot:^Native_Frame,object:^NS.Object,resource:gfx.Resource_Id,offset:u64,role:u32,region:gfx.Image_Region,layout:gfx.Image_Layout) {
    if !r.capture.recording { return }
    resolved:=region;resolved.bytes_per_row=layout.bytes_per_row;resolved.bytes_per_image=layout.bytes_per_image
    for z in 0..<u64(region.depth) { for row in 0..<layout.block_rows {
        capture_transfer_buffer(r,slot,object,resource,{offset+z*layout.bytes_per_image+row*layout.bytes_per_row,layout.row_bytes},role,region=resolved,row=u32(z*layout.block_rows+row))
    } }
}
