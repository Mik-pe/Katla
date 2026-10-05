//! Bounded image decoding preserves native precision; GPU transfer functions stay in composition.
package katla_image
import "core:mem"
import "core:bytes"
import native "core:image"
import png "core:image/png"

TEXTURE_IMAGE_MAX_DIMENSION :: 8192
TEXTURE_IMAGE_MAX_PIXELS :: 16*1024*1024
TEXTURE_IMAGE_MAX_BYTES :: TEXTURE_IMAGE_MAX_PIXELS*4
TEXTURE_IMAGE_MAX_ENCODED_BYTES :: 32*1024*1024
Image_Format :: enum { RGBA8,RGBA16,RGBA32_Float }
/// Owns native-endian, tightly packed RGBA samples without applying a transfer function.
Texture_Image :: struct { width,height:u32,pixels:[]byte,allocator:mem.Allocator,format:Image_Format }
/// Unsupported formats, invalid streams and exceeded budgets remain distinct failures.
Texture_Image_Error :: enum { None, Unsupported, Invalid_Data, Limit, Allocation }

@(private="package")
texture_image_allocate :: proc(width,height:u32,format:Image_Format,allocator:mem.Allocator)->(Texture_Image,Texture_Image_Error) {
    if width==0 || height==0 { return {},.Invalid_Data }
    count:=u64(width)*u64(height)
    size:=count*4*(1 if format==.RGBA8 else 2 if format==.RGBA16 else 4)
    if width>TEXTURE_IMAGE_MAX_DIMENSION || height>TEXTURE_IMAGE_MAX_DIMENSION || count>TEXTURE_IMAGE_MAX_PIXELS || size>TEXTURE_IMAGE_MAX_BYTES { return {},.Limit }
    pixels,error:=mem.make([]byte,int(size),allocator)
    if error!=nil || pixels==nil { return {},.Allocation }
    return {width,height,pixels,allocator,format},.None
}

/// Admits dimensions and compressed expansion before decoding; output always owns its allocator.
texture_image_decode :: proc(encoded:[]byte,allocator:mem.Allocator=context.allocator)->(Texture_Image,Texture_Image_Error) {
    if len(encoded)==0 { return {},.Invalid_Data }
    if len(encoded)>TEXTURE_IMAGE_MAX_ENCODED_BYTES { return {},.Limit }
    if len(encoded)>=4 && (string(encoded[:4])=="II\x2a\x00" || string(encoded[:4])=="MM\x00\x2a" || string(encoded[:4])=="II\x2b\x00" || string(encoded[:4])=="MM\x00\x2b") { return texture_tiff_decode(encoded,allocator) }
    is_png:=len(encoded)>=8 && string(encoded[:8])=="\x89PNG\r\n\x1a\n"
    is_jpeg:=len(encoded)>=2 && encoded[0]==0xff && encoded[1]==0xd8
    is_bmp:=len(encoded)>=2 && string(encoded[:2])=="BM"
    if !is_png && !is_jpeg && !is_bmp { return {},.Unsupported }
    if is_jpeg { return texture_jpeg_decode(encoded,allocator) }
    width,height:u32; format:=Image_Format.RGBA8
    if is_png {
        if len(encoded)<33 { return {},.Invalid_Data }
        width=texture_be32(encoded[16:20]); height=texture_be32(encoded[20:24])
        if encoded[24]==16 { format=.RGBA16 }
    } else if is_bmp {
        if len(encoded)<26 { return {},.Invalid_Data }
        header:=bmp_u32(encoded[14:])
        if header==12 { width=bmp_u16(encoded[18:]); height=bmp_u16(encoded[20:]) }
        else { width=bmp_u32(encoded[18:]); signed_height:=i32(bmp_u32(encoded[22:])); if signed_height==min(i32) { return {},.Invalid_Data }; height=u32(abs(signed_height)) }
    }
    // Validate size without allocating before metadata/payload admission.
    size:=u64(width)*u64(height)*4*(2 if format==.RGBA16 else 1)
    if width==0 || height==0 { return {},.Invalid_Data }
    if width>TEXTURE_IMAGE_MAX_DIMENSION || height>TEXTURE_IMAGE_MAX_DIMENSION || u64(width)*u64(height)>TEXTURE_IMAGE_MAX_PIXELS || size>TEXTURE_IMAGE_MAX_BYTES { return {},.Limit }
    validation:Texture_Image_Error
    if is_png { validation=texture_png_validate(encoded,width,height,allocator) }
    else { validation=texture_bmp_validate(encoded,width,height) }
    if validation!=.None { return {},validation }
    if is_bmp { return texture_bmp_decode(encoded,width,height,allocator) }
    result,error:=texture_image_allocate(width,height,format,allocator); if error!=.None { return {},error }
    accepted:=false; defer if !accepted { texture_image_destroy(&result) }
    {
        decoded:^native.Image; decode_error:native.Error
        decoded,decode_error=png.load_from_bytes(encoded,{.alpha_add_if_missing},allocator)
        defer native.destroy(decoded,allocator)
        if decode_error!=nil || decoded==nil { return {},.Invalid_Data }
        pixels:=bytes.buffer_to_bytes(&decoded.pixels)
        if decoded.width!=int(width) || decoded.height!=int(height) || decoded.channels!=4 || decoded.depth!=(16 if format==.RGBA16 else 8) || len(pixels)!=len(result.pixels) { return {},.Invalid_Data }
        copy(result.pixels,pixels)
    }
    accepted=true
    return result,.None
}
/// Releases pixels with their captured allocator even when the calling context has changed.
texture_image_destroy :: proc(value:^Texture_Image) { delete(value.pixels,value.allocator); value^={} }
