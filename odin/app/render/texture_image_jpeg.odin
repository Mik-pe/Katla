//! JPEG marker framing rejects missing scan/end markers before native decoding.
package render

@(private="package")
texture_jpeg_validate :: proc(encoded:[]byte,width,height:u32)->Texture_Image_Error {
    offset:=2; in_scan,found_frame,found_scan:bool
    for offset<len(encoded) {
        if in_scan {
            for offset<len(encoded) && encoded[offset]!=0xff { offset+=1 }
        }
        if offset>=len(encoded) || encoded[offset]!=0xff { return .Invalid_Data }
        for offset<len(encoded) && encoded[offset]==0xff { offset+=1 }
        if offset>=len(encoded) { return .Invalid_Data }
        marker:=encoded[offset]; offset+=1
        if in_scan && (marker==0 || marker>=0xd0 && marker<=0xd7) { continue }
        in_scan=false
        if marker==0xd9 { return .None if found_scan && found_frame && offset==len(encoded) else .Invalid_Data }
        if marker==0 || marker==0xd8 || marker>=0xd0 && marker<=0xd7 || marker==1 { return .Invalid_Data }
        if len(encoded)-offset<2 { return .Invalid_Data }
        size:=int(encoded[offset])<<8|int(encoded[offset+1]); if size<2 || size>len(encoded)-offset { return .Invalid_Data }
        if marker>=0xc0 && marker<=0xcf && marker!=0xc4 && marker!=0xc8 && marker!=0xcc {
            if marker!=0xc0 && marker!=0xc1 && marker!=0xc2 { return .Unsupported }
            if found_frame || size<8 { return .Invalid_Data }; found_frame=true
            h:=u32(encoded[offset+3])<<8|u32(encoded[offset+4]); w:=u32(encoded[offset+5])<<8|u32(encoded[offset+6])
            if w!=width || h!=height { return .Invalid_Data }
        }
        if marker==0xda { if !found_frame { return .Invalid_Data }; found_scan=true; in_scan=true }
        offset+=size
    }
    return .Invalid_Data
}
