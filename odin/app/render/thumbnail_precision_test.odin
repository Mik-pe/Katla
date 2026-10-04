#+test
package render

import image_api "../../image"
import "core:testing"
import "core:mem"

@(test)
test_thumbnail_precision_conversion_and_resize_preserve_source_samples :: proc(t:^testing.T) {
    for format in ([2]image_api.Image_Format{.RGBA16,.RGBA32_Float}) {
        stride:=8 if format==.RGBA16 else 16
        source:=Texture_Image{width=4,height=2,format=format,allocator=context.allocator,pixels=make([]byte,8*stride)}
        defer texture_image_destroy(&source)
        expected:[4]byte
        if format==.RGBA16 {
            values:=[4]u16{10000,32768,65535,16384}; expected={39,128,255,64}
            for i in 0..<8 { for value,c in values { encoded:=transmute([2]byte)value; copy(source.pixels[i*stride+c*2:i*stride+c*2+2],encoded[:]) } }
        } else {
            values:=[4]f32{0.25,0.5,2,0.5}; expected={137,188,255,128}
            for i in 0..<8 { for value,c in values { encoded:=transmute([4]byte)value; copy(source.pixels[i*stride+c*4:i*stride+c*4+4],encoded[:]) } }
        }
        original:=image_api.texture_image_sample(&source,7)
        for max_edge in ([2]int{4,2}) {
            preview,error:=thumbnail_resize(&source,max_edge,context.allocator); defer texture_image_destroy(&preview)
            testing.expect_value(t,error,Texture_Image_Error.None); if error!=.None { continue }
            testing.expect_value(t,preview.format,image_api.Image_Format.RGBA8)
            testing.expect_value(t,preview.width,u32(max_edge)); testing.expect_value(t,preview.height,u32(max_edge/2))
            testing.expect(t,raw_data(preview.pixels)!=raw_data(source.pixels))
            for i in 0..<len(preview.pixels)/4 { for channel,c in expected { testing.expect_value(t,preview.pixels[i*4+c],channel) } }
            testing.expect_value(t,image_api.texture_image_sample(&source,7),original)
        }
        rejected,error:=thumbnail_resize(&source,2,mem.nil_allocator())
        testing.expect_value(t,error,Texture_Image_Error.Allocation); testing.expect(t,len(rejected.pixels)==0)
    }
}
