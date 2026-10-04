//! BMP framing bounds palette/mask headers and exact padded rows before native pixel allocation.
package render

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
