//! Encoded model textures decode to bounded owned RGBA8; GPU color-space policy stays in composition.
package render

import "core:mem"
import "core:c"
import image "../../deps/stb_image"

TEXTURE_IMAGE_MAX_DIMENSION :: 8192
TEXTURE_IMAGE_MAX_PIXELS :: 16*1024*1024
TEXTURE_IMAGE_MAX_BYTES :: TEXTURE_IMAGE_MAX_PIXELS*4
TEXTURE_IMAGE_MAX_ENCODED_BYTES :: 32*1024*1024
/// Owns top-to-bottom tightly packed RGBA8 samples without applying a color-space conversion.
Texture_Image :: struct { width,height:u32, pixels:[]byte, allocator:mem.Allocator }
/// Unsupported formats, invalid streams and exceeded budgets remain distinct failures.
Texture_Image_Error :: enum { None, Unsupported, Invalid_Data, Limit, Allocation }
/// Preflights actual dimensions and compressed PNG expansion before invoking the native decoder.
texture_image_decode :: proc(encoded:[]byte,allocator:mem.Allocator=context.allocator)->(Texture_Image,Texture_Image_Error) {
    if len(encoded)==0 { return {},.Invalid_Data }
    if len(encoded)>TEXTURE_IMAGE_MAX_ENCODED_BYTES { return {},.Limit }
    png:=len(encoded)>=8 && string(encoded[:8])=="\x89PNG\r\n\x1a\n"
    jpeg:=len(encoded)>=2 && encoded[0]==0xff && encoded[1]==0xd8
    if !png && !jpeg { return {},.Unsupported }
    width,height,channels:c.int
    if image.info_from_memory(raw_data(encoded),c.int(len(encoded)),&width,&height,&channels)==0 { return {},.Invalid_Data }
    if width<=0 || height<=0 || channels<1 || channels>4 { return {},.Invalid_Data }
    pixels:=u64(width)*u64(height)
    if width>TEXTURE_IMAGE_MAX_DIMENSION || height>TEXTURE_IMAGE_MAX_DIMENSION || pixels>TEXTURE_IMAGE_MAX_PIXELS || pixels*4>TEXTURE_IMAGE_MAX_BYTES { return {},.Limit }
    validation:Texture_Image_Error
    if png { validation=texture_png_validate(encoded,u32(width),u32(height),allocator) }
    else { validation=texture_jpeg_validate(encoded,u32(width),u32(height)) }
    if validation!=.None { return {},validation }
    image.set_flip_vertically_on_load_thread(false)
    decoded_width,decoded_height,decoded_channels:c.int
    decoded:=image.load_from_memory(raw_data(encoded),c.int(len(encoded)),&decoded_width,&decoded_height,&decoded_channels,4)
    if decoded==nil { return {},.Invalid_Data }; defer image.image_free(decoded)
    if decoded_width!=width || decoded_height!=height || decoded_channels!=channels { return {},.Invalid_Data }
    owned,allocation_error:=mem.make([]byte,int(pixels*4),allocator)
    if allocation_error!=nil || raw_data(owned)==nil { return {},.Allocation }
    copy(owned,decoded[:len(owned)])
    return Texture_Image{u32(width),u32(height),owned,allocator},.None
}
/// Releases pixels with their captured allocator even when the calling context has changed.
texture_image_destroy :: proc(value:^Texture_Image) { delete(value.pixels,value.allocator); value^={} }
