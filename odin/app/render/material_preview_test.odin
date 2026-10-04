#+test
package render

import "core:mem"
import "core:testing"

@(test)
test_material_preview_sampling_bounds_preserve_native_precision_and_alpha :: proc(t:^testing.T) {
    words:=make([]u16,256*128*4);defer delete(words)
    for y in 0..<128 { for x in 0..<256 { offset:=(y*256+x)*4;words[offset]=u16(x*257);words[offset+1]=12345;words[offset+2]=54321;words[offset+3]=u16(y*511) } }
    source:=Texture_Image{width=256,height=128,pixels=mem.slice_to_bytes(words),format=.RGBA16}
    resized,error:=material_preview_resize(&source,context.allocator);defer texture_image_destroy(&resized)
    testing.expect_value(t,error,Texture_Image_Error.None);testing.expect_value(t,resized.width,u32(128));testing.expect_value(t,resized.height,u32(64));testing.expect_value(t,resized.format,source.format)
    result:=mem.slice_data_cast([]u16,resized.pixels)
    testing.expect_value(t,result[0],u16(257));testing.expect_value(t,result[1],u16(12345));testing.expect_value(t,result[2],u16(54321));testing.expect_value(t,result[3],u16(511))
    floats:=[4]f32{.5,.25,4,.125};hdr:=Texture_Image{width=1,height=1,pixels=mem.slice_to_bytes(floats[:]),format=.RGBA32_Float}
    copied,copy_error:=material_preview_resize(&hdr,context.allocator);defer texture_image_destroy(&copied)
    testing.expect_value(t,copy_error,Texture_Image_Error.None);testing.expect_value(t,mem.slice_data_cast([]f32,copied.pixels)[2],f32(4));testing.expect(t,raw_data(copied.pixels)!=raw_data(hdr.pixels))
    failed,rejection:=material_preview_resize(&hdr,mem.nil_allocator());testing.expect_value(t,rejection,Texture_Image_Error.Allocation);testing.expect_value(t,len(failed.pixels),0)
    hdr.pixels=hdr.pixels[:4];_,malformed:=material_preview_resize(&hdr,context.allocator);testing.expect_value(t,malformed,Texture_Image_Error.Invalid_Data)
}
