//! Bounded PNG serialization reads only immutable completed native rows.
package editor_app
import render "../render"
import "core:mem"
import "core:hash"

/// Encodes exact RGBA/BGRA8 native image rows without an OS or vendor codec dependency.
capture_png :: proc(snapshot:^render.Picking_Snapshot,allocator:mem.Allocator=context.allocator)->([]byte,bool) {
    width,height:=snapshot.metadata.width,snapshot.metadata.height
    data:=snapshot.color
    if width==0 || height==0 || width>8192 || height>8192 || u64(width)*u64(height)>16*1024*1024 || data.row_pitch<u64(width)*4 || u64(len(data.bytes))<u64(height-1)*data.row_pitch+u64(width)*4 || (data.source.desc.format!=.RGBA8_Unorm && data.source.desc.format!=.BGRA8_Unorm) { return nil,false }
    rows:=make([]byte,int(u64(height)*(u64(width)*4+1)),allocator); defer delete(rows,allocator)
    for y in 0..<height { start:=int(y)*(int(width)*4+1); rows[start]=0; source:=int(u64(y)*data.row_pitch); copy(rows[start+1:start+1+int(width)*4],data.bytes[source:source+int(width)*4]); if data.source.desc.format==.BGRA8_Unorm { for x in 0..<width { pixel:=start+1+int(x)*4; rows[pixel],rows[pixel+2]=rows[pixel+2],rows[pixel] } } }
    compressed:=make([dynamic]byte,allocator); defer delete(compressed); append(&compressed,0x78,0x01)
    offset:=0
    for offset<len(rows) { length:=min(65535,len(rows)-offset); append(&compressed,u8(offset+length==len(rows)),u8(length&255),u8(length>>8),u8((~length)&255),u8(((~length)>>8)&255)); append(&compressed,..rows[offset:offset+length]); offset+=length }
    png_u32(&compressed,hash.adler32(rows))
    result:=make([dynamic]byte,allocator); defer delete(result); append(&result,..([]byte{137,80,78,71,13,10,26,10}))
    header:=make([dynamic]byte,allocator); defer delete(header); png_u32(&header,width); png_u32(&header,height); append(&header,8,6,0,0,0)
    png_chunk(&result,"IHDR",header[:]); png_chunk(&result,"IDAT",compressed[:]); png_chunk(&result,"IEND",nil)
    output:=make([]byte,len(result),allocator); copy(output,result[:]); return output,true
}
@(private="package")
png_u32 :: proc(target:^[dynamic]byte,value:u32) { append(target,u8(value>>24),u8((value>>16)&255),u8((value>>8)&255),u8(value&255)) }
@(private="package")
png_chunk :: proc(target:^[dynamic]byte,name:string,data:[]byte) { png_u32(target,u32(len(data))); start:=len(target^); append(target,name); append(target,..data); png_u32(target,hash.crc32(target^[start:])) }
