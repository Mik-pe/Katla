//! BMP framing bounds palette/mask headers and exact padded rows before native pixel allocation.
package katla_image
import "core:mem"

@(private="package")
bmp_u16 :: proc(bytes:[]byte)->u32 { return u32(bytes[0])|u32(bytes[1])<<8 }
@(private="package")
bmp_u32 :: proc(bytes:[]byte)->u32 { return bmp_u16(bytes)|bmp_u16(bytes[2:])<<16 }
@(private="package")
texture_bmp_validate :: proc(bytes:[]byte,width,height:u32)->Texture_Image_Error {
    if len(bytes)<26 || string(bytes[:2])!="BM" { return .Invalid_Data }
    file_size:=bmp_u32(bytes[2:]); offset:=u64(bmp_u32(bytes[10:])); header:=bmp_u32(bytes[14:])
    if file_size!=0 && u64(file_size)>u64(len(bytes)) { return .Invalid_Data }
    if header!=12 && header!=40 && header!=56 && header!=108 && header!=124 { return .Unsupported }
    if u64(header)+14>u64(len(bytes)) || offset<u64(header)+14 || offset>u64(len(bytes)) { return .Invalid_Data }
    bits,planes,compression,colors:u32
    if header==12 { planes=bmp_u16(bytes[22:]); bits=bmp_u16(bytes[24:]) }
    else { planes=bmp_u16(bytes[26:]); bits=bmp_u16(bytes[28:]); compression=bmp_u32(bytes[30:]); colors=bmp_u32(bytes[46:]) }
    if planes!=1 || (bits!=1 && bits!=4 && bits!=8 && bits!=16 && bits!=24 && bits!=32) { return .Invalid_Data }
    if compression!=0 && compression!=3 { return .Unsupported }
    if compression==3 && bits!=16 && bits!=32 { return .Invalid_Data }
    metadata:=u64(header)+14
    if bits<=8 {
        palette:=u64(colors); if palette==0 { palette=u64(1)<<bits }; if palette>u64(1)<<bits { return .Invalid_Data }
        entry_size:u64=4; if header==12 { entry_size=3 }; metadata+=palette*entry_size
    }
    if compression==3 && header==40 { metadata+=12 }
    if offset<metadata { return .Invalid_Data }
    row:=(u64(width)*u64(bits)+31)/32*4
    if row*u64(height)>u64(len(bytes))-offset { return .Invalid_Data }
    return .None
}

@(private="package")
bmp_sample :: proc(value,mask:u32)->byte {
    if mask==0 { return 255 }
    shifted:=mask; bits:=value&mask
    for shifted&1==0 { shifted>>=1; bits>>=1 }
    return byte((u64(bits)*255+u64(shifted)/2)/u64(shifted))
}
@(private="package")
texture_bmp_decode :: proc(encoded:[]byte,width,height:u32,allocator:mem.Allocator)->(Texture_Image,Texture_Image_Error) {
    header:=bmp_u32(encoded[14:]); bits:=bmp_u16(encoded[24:] if header==12 else encoded[28:])
    compression:u32; if header!=12 { compression=bmp_u32(encoded[30:]) }
    masks:=[4]u32{0x7c00,0x3e0,0x1f,0}
    if bits==32 { masks={0xff0000,0xff00,0xff,0xff000000} }
    if compression==3 {
        for i in 0..<3 { masks[i]=bmp_u32(encoded[54+i*4:]) }
        masks[3]=0; if header>=56 { masks[3]=bmp_u32(encoded[66:]) }
        occupied:u32
        for mask,i in masks {
            if i<3 && mask==0 || mask&occupied!=0 || bits==16 && mask>>16!=0 { return {},.Invalid_Data }
            if mask!=0 { shifted:=mask; for shifted&1==0 { shifted>>=1 }; if shifted&(shifted+1)!=0 { return {},.Unsupported } }
            occupied|=mask
        }
    }
    image,error:=texture_image_allocate(width,height,.RGBA8,allocator); if error!=.None { return {},error }
    accepted:=false; defer if !accepted { texture_image_destroy(&image) }
    row:=(int(width)*int(bits)+31)/32*4; offset:=int(bmp_u32(encoded[10:]))
    top_down:=header!=12 && i32(bmp_u32(encoded[22:]))<0
    palette_size:=1<<bits; if header!=12 && bits<=8 && bmp_u32(encoded[46:])!=0 { palette_size=int(bmp_u32(encoded[46:])) }
    palette_offset:=int(header)+14; palette_stride:=3 if header==12 else 4
    alpha_seen:=false
    for y in 0..<int(height) {
        input:=encoded[offset+row*(y if top_down else int(height)-1-y):][:row]
        for x in 0..<int(width) {
            pixel:=image.pixels[(y*int(width)+x)*4:][:4]
            if bits<=8 {
                index:=int(input[x*int(bits)/8]>>u32(8-int(bits)-(x*int(bits)%8)))&((1<<bits)-1)
                if index>=palette_size { return {},.Invalid_Data }
                entry:=encoded[palette_offset+index*palette_stride:][:palette_stride]
                pixel[0]=entry[2]; pixel[1]=entry[1]; pixel[2]=entry[0]; pixel[3]=255
            } else if bits==24 {
                pixel[0]=input[x*3+2]; pixel[1]=input[x*3+1]; pixel[2]=input[x*3]; pixel[3]=255
            } else {
                value:=bmp_u16(input[x*2:]) if bits==16 else bmp_u32(input[x*4:])
                for channel in 0..<4 { pixel[channel]=bmp_sample(value,masks[channel]) }
                alpha_seen=alpha_seen || pixel[3]!=0
            }
        }
    }
    if bits==32 && compression==0 && !alpha_seen { for i in 0..<int(width)*int(height) { image.pixels[i*4+3]=255 } }
    accepted=true; return image,.None
}
