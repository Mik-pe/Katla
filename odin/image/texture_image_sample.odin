//! Preview conversion is explicit and never replaces precise material image ownership.
package katla_image
import "core:mem"
import "core:math"

/// Reads one expanded RGBA texel as normalized integers or original linear float samples.
texture_image_sample :: #force_inline proc(image:^Texture_Image,index:int)->[4]f32 {
    result:[4]f32
    switch image.format {
    case .RGBA8:
        for &channel,c in result { channel=f32(image.pixels[index*4+c])/255 }
    case .RGBA16:
        for &channel,c in result { offset:=(index*4+c)*2; value:=transmute(u16)([2]byte{image.pixels[offset],image.pixels[offset+1]}); channel=f32(value)/65535 }
    case .RGBA32_Float:
        for &channel,c in result { offset:=(index*4+c)*4; channel=transmute(f32)([4]byte{image.pixels[offset],image.pixels[offset+1],image.pixels[offset+2],image.pixels[offset+3]}) }
    }
    return result
}

/// Produces an independently owned RGBA8 display preview; float RGB is encoded from linear light.
texture_image_rgba8 :: proc(source:^Texture_Image,allocator:mem.Allocator=context.allocator)->(Texture_Image,Texture_Image_Error) {
    size:=u64(source.width)*u64(source.height)*4
    if size==0 || size>TEXTURE_IMAGE_MAX_BYTES { return {},.Limit }
    bytes,error:=mem.make([]byte,int(size),allocator)
    if error!=nil || len(bytes)!=int(size) { return {},.Allocation }
    result:=Texture_Image{source.width,source.height,bytes,allocator,.RGBA8}
    for i in 0..<int(size/4) {
        values:=texture_image_sample(source,i)
        for value,c in values {
            normalized:=clamp(value,0,1)
            if source.format==.RGBA32_Float && c<3 { normalized=12.92*normalized if normalized<=0.0031308 else 1.055*math.pow(normalized,f32(1.0/2.4))-0.055 }
            result.pixels[i*4+c]=u8(math.round(normalized*255))
        }
    }
    return result,.None
}
