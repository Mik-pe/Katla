//! Bounded image decoding preserves native precision; GPU transfer functions stay in composition.
package image

import "core:mem"
import "core:c"
import "core:math"
import image "../deps/stb_image"

TEXTURE_IMAGE_MAX_DIMENSION :: 8192
TEXTURE_IMAGE_MAX_PIXELS :: 16*1024*1024
TEXTURE_IMAGE_MAX_BYTES :: TEXTURE_IMAGE_MAX_PIXELS*4
TEXTURE_IMAGE_MAX_ENCODED_BYTES :: 32*1024*1024
Image_Format :: enum { RGBA8,RGBA16,RGBA32_Float }
/// Owns native-endian, tightly packed RGBA samples without applying a transfer function.
Texture_Image :: struct { width,height:u32,pixels:[]byte,allocator:mem.Allocator,format:Image_Format }
/// Unsupported formats, invalid streams and exceeded budgets remain distinct failures.
Texture_Image_Error :: enum { None, Unsupported, Invalid_Data, Limit, Allocation }
/// Preflights actual dimensions and compressed PNG expansion before invoking the native decoder.
texture_image_decode :: proc(encoded:[]byte,allocator:mem.Allocator=context.allocator)->(Texture_Image,Texture_Image_Error) {
    if len(encoded)==0 { return {},.Invalid_Data }
    if len(encoded)>TEXTURE_IMAGE_MAX_ENCODED_BYTES { return {},.Limit }
    png:=len(encoded)>=8 && string(encoded[:8])=="\x89PNG\r\n\x1a\n"
    jpeg:=len(encoded)>=2 && encoded[0]==0xff && encoded[1]==0xd8
    bmp:=len(encoded)>=2 && string(encoded[:2])=="BM"
    tiff:=len(encoded)>=4 && (string(encoded[:4])=="II\x2a\x00" || string(encoded[:4])=="MM\x00\x2a" || string(encoded[:4])=="II\x2b\x00" || string(encoded[:4])=="MM\x00\x2b")
    if !png && !jpeg && !bmp && !tiff { return {},.Unsupported }
    width,height,channels:c.int
    info:=image.info_from_memory(raw_data(encoded),c.int(len(encoded)),&width,&height,&channels)
    if info==-2 { return {},.Limit }; if info!=1 { return {},.Invalid_Data }
    if width<=0 || height<=0 || channels<1 || channels>4 { return {},.Invalid_Data }
    pixels:=u64(width)*u64(height)
    if width>TEXTURE_IMAGE_MAX_DIMENSION || height>TEXTURE_IMAGE_MAX_DIMENSION || pixels>TEXTURE_IMAGE_MAX_PIXELS || pixels*4>TEXTURE_IMAGE_MAX_BYTES { return {},.Limit }
    validation:Texture_Image_Error
    if png { validation=texture_png_validate(encoded,u32(width),u32(height),allocator) }
    else if jpeg { validation=texture_jpeg_validate(encoded,u32(width),u32(height)) }
    else if bmp { validation=texture_bmp_validate(encoded,u32(width),u32(height)) }
    if validation!=.None { return {},validation }
    precision:=image.precision_from_memory(raw_data(encoded),c.int(len(encoded)))
    if precision==-2 { return {},.Limit }; if precision==-1 { return {},.Unsupported }; if precision!=1 && precision!=2 && precision!=4 { return {},.Invalid_Data }
    byte_count:=pixels*4*u64(precision); if byte_count>TEXTURE_IMAGE_MAX_BYTES { return {},.Limit }
    image.set_flip_vertically_on_load_thread(false)
    decoded_width,decoded_height,decoded_channels:c.int
    decoded:rawptr; format:=Image_Format.RGBA8
    switch precision {
    case 1: decoded=image.load_from_memory(raw_data(encoded),c.int(len(encoded)),&decoded_width,&decoded_height,&decoded_channels,4)
    case 2: decoded=image.load_16_from_memory(raw_data(encoded),c.int(len(encoded)),&decoded_width,&decoded_height,&decoded_channels,4); format=.RGBA16
    case 4: decoded=image.load_float_from_memory(raw_data(encoded),c.int(len(encoded)),&decoded_width,&decoded_height,&decoded_channels,4); format=.RGBA32_Float
    }
    if decoded==nil { return {},.Invalid_Data }; defer image.image_free(decoded)
    if decoded_width!=width || decoded_height!=height || decoded_channels!=channels { return {},.Invalid_Data }
    owned,allocation_error:=mem.make([]byte,int(byte_count),allocator)
    if allocation_error!=nil || raw_data(owned)==nil { return {},.Allocation }
    copy(owned,(cast([^]byte)decoded)[:len(owned)])
    result:=Texture_Image{u32(width),u32(height),owned,allocator,format}
    if format==.RGBA32_Float {
        for i in 0..<int(pixels) {
            for value in texture_image_sample(&result,i) { if math.is_nan(value) || math.is_inf(value) || math.abs(value)>65504 { texture_image_destroy(&result); return {},.Invalid_Data } }
        }
    }
    return result,.None
}
/// Releases pixels with their captured allocator even when the calling context has changed.
texture_image_destroy :: proc(value:^Texture_Image) { delete(value.pixels,value.allocator); value^={} }
