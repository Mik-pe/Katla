//! PNG framing, integrity and fixed-output inflation prevent compressed payloads exceeding their header.
package render

import image "../../deps/stb_image"
import "core:mem"
import "core:hash"
import "core:c"

@(private="package")
texture_be32 :: proc(data:[]byte)->u32 { return u32(data[0])<<24|u32(data[1])<<16|u32(data[2])<<8|u32(data[3]) }
@(private="package")
texture_png_scanline_bytes :: proc(width,height:u32,depth,channels:u8,interlaced:bool)->u64 {
    if !interlaced { return ((u64(width)*u64(channels)*u64(depth)+7)/8+1)*u64(height) }
    starts_x:=[7]u32{0,4,0,2,0,1,0}; starts_y:=[7]u32{0,0,4,0,2,0,1}
    steps_x:=[7]u32{8,8,4,4,2,2,1}; steps_y:=[7]u32{8,8,8,4,4,2,2}
    total:u64
    for start_x,i in starts_x {
        if width<=start_x || height<=starts_y[i] { continue }
        w:=(width-start_x+steps_x[i]-1)/steps_x[i]; h:=(height-starts_y[i]+steps_y[i]-1)/steps_y[i]
        total+=((u64(w)*u64(channels)*u64(depth)+7)/8+1)*u64(h)
    }
    return total
}
@(private="package")
texture_png_validate :: proc(encoded:[]byte,width,height:u32,allocator:mem.Allocator)->Texture_Image_Error {
    if len(encoded)<33 || texture_be32(encoded[8:12])!=13 || string(encoded[12:16])!="IHDR" { return .Invalid_Data }
    if texture_be32(encoded[16:20])!=width || texture_be32(encoded[20:24])!=height { return .Invalid_Data }
    depth,color:=encoded[24],encoded[25]; channels:u8
    switch color {
    case 0: channels=1
    case 2: channels=3
    case 3: channels=1
    case 4: channels=2
    case 6: channels=4
    case: return .Unsupported
    }
    if depth!=1 && depth!=2 && depth!=4 && depth!=8 && depth!=16 { return .Invalid_Data }
    if (color==2 || color==4 || color==6) && depth<8 || color==3 && depth==16 { return .Invalid_Data }
    if encoded[26]!=0 || encoded[27]!=0 || encoded[28]>1 { return .Unsupported }
    scanline_bytes:=texture_png_scanline_bytes(width,height,depth,channels,encoded[28]==1)
    if scanline_bytes==0 || scanline_bytes>TEXTURE_IMAGE_MAX_BYTES+7*TEXTURE_IMAGE_MAX_DIMENSION { return .Limit }
    offset:=8; compressed_size:=0; found_data,ended_data,found_end:bool
    for offset<len(encoded) {
        if len(encoded)-offset<12 { return .Invalid_Data }
        size:=u64(texture_be32(encoded[offset:offset+4]))
        if size>u64(len(encoded)-offset-12) { return .Invalid_Data }
        end:=offset+12+int(size); kind:=string(encoded[offset+4:offset+8])
        if hash.crc32(encoded[offset+4:end-4])!=texture_be32(encoded[end-4:end]) { return .Invalid_Data }
        switch kind {
        case "IHDR": if offset!=8 { return .Invalid_Data }
        case "IDAT":
            if ended_data { return .Invalid_Data }; found_data=true; compressed_size+=int(size)
        case "IEND":
            if size!=0 || !found_data || end!=len(encoded) { return .Invalid_Data }; found_end=true
        case "PLTE": if found_data { return .Invalid_Data }
        case:
            if encoded[offset+4]&32==0 { return .Unsupported }
        }
        if found_data && kind!="IDAT" { ended_data=true }
        offset=end
    }
    if !found_end || compressed_size<6 { return .Invalid_Data }
    compressed,allocation_error:=mem.make([]byte,compressed_size,allocator)
    if allocation_error!=nil || raw_data(compressed)==nil { return .Allocation }; defer delete(compressed,allocator)
    offset=8; destination:=0
    for offset<len(encoded) {
        size:=int(texture_be32(encoded[offset:offset+4])); end:=offset+12+size
        if string(encoded[offset+4:offset+8])=="IDAT" { copy(compressed[destination:destination+size],encoded[offset+8:end-4]); destination+=size }
        offset=end
    }
    filtered,filter_error:=mem.make([]byte,int(scanline_bytes),allocator)
    if filter_error!=nil || raw_data(filtered)==nil { return .Allocation }; defer delete(filtered,allocator)
    written:=image.zlib_decode_buffer(raw_data(filtered),c.int(len(filtered)),raw_data(compressed),c.int(len(compressed)))
    if written!=c.int(len(filtered)) { return .Invalid_Data }
    if hash.adler32(filtered)!=texture_be32(compressed[len(compressed)-4:]) { return .Invalid_Data }
    return .None
}
