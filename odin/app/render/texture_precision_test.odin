#+test
package render

import image "../../image"
import km "../../math"
import "core:testing"
import "core:mem"

@(test)
test_texture_native_role_preserves_integer_precision_and_linear_hdr :: proc(t:^testing.T) {
    integers:=[4]u16{32769,12345,65535,16385}
    decoded:=Texture_Image{width=1,height=1,pixels=mem.slice_to_bytes(integers[:]),format=.RGBA16}
    format,data,owned,error:=native_texture_samples(&decoded,false)
    testing.expect(t,error=={} && format==.RGBA16_Unorm && !owned && raw_data(data)==raw_data(decoded.pixels))
    format,data,owned,error=native_texture_samples(&decoded,true)
    testing.expect(t,error=={} && format==.RGBA16_Float && owned); defer { if owned { delete(data) } }
    halves:=(cast(^[4]f16)raw_data(data))^
    testing.expect(t,halves[0]==f16(km.color_to_linear({f32(integers[0])/65535,0,0,1}).r) && halves[3]==f16(f32(integers[3])/65535))
    hdr:=[4]f32{4.5,-.25,65504,.125}
    floating:=Texture_Image{width=1,height=1,pixels=mem.slice_to_bytes(hdr[:]),format=.RGBA32_Float}
    hdr_format,hdr_data,hdr_owned,hdr_error:=native_texture_samples(&floating,true)
    testing.expect(t,hdr_error=={} && hdr_format==.RGBA16_Float && hdr_owned); defer { if hdr_owned { delete(hdr_data) } }
    result:=(cast(^[4]f16)raw_data(hdr_data))^; for value,index in hdr { testing.expect(t,result[index]==f16(value)) }
    _,_,_,allocation_failure:=native_texture_samples(&floating,false,mem.nil_allocator()); testing.expect(t,allocation_failure.gpu==.Allocation_Failed)
    hdr[0]=65505; _,_,_,invalid:=native_texture_samples(&floating,false); testing.expect(t,invalid.scene==.Invalid_Material)
    testing.expect(t,image.Image_Format.RGBA16!=image.Image_Format.RGBA8)
}
