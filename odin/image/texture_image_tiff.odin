//! Classic/BigTIFF parsing owns bounded tags, compression, precision and orientation in Odin.
package katla_image
import "core:mem"
import "core:math"

@(private="package")
Tiff_Field :: struct { tag:u16,kind:u16,count:u64,data:[]byte }
@(private="package")
Tiff_Directory :: struct { encoded:[]byte,entries:[]byte,count:int,big,little:bool }
@(private="package")
tiff_integer :: #force_inline proc(data:[]byte,little:bool)->u64 {
    result:u64
    for value,i in data { shift:=i if little else len(data)-1-i; result|=u64(value)<<u32(shift*8) }
    return result
}
@(private="package")
tiff_type_bytes :: proc(kind:u16)->u64 {
    switch kind {
    case 1,2,6,7: return 1
    case 3,8: return 2
    case 4,9,11,13: return 4
    case 5,10,12,16,17,18: return 8
    }
    return 0
}
@(private="package")
tiff_directory :: proc(encoded:[]byte)->(Tiff_Directory,Texture_Image_Error) {
    if len(encoded)<8 { return {},.Invalid_Data }
    d:=Tiff_Directory{encoded=encoded,little=encoded[0]=='I'}
    magic:=tiff_integer(encoded[2:4],d.little); d.big=magic==43
    offset:u64; count_bytes:u64=2; entry_bytes:u64=12
    if d.big {
        if len(encoded)<16 || tiff_integer(encoded[4:6],d.little)!=8 || tiff_integer(encoded[6:8],d.little)!=0 { return {},.Invalid_Data }
        offset=tiff_integer(encoded[8:16],d.little); count_bytes=8; entry_bytes=20
    } else { offset=tiff_integer(encoded[4:8],d.little) }
    if offset<8 || offset>u64(len(encoded)) || count_bytes>u64(len(encoded))-offset { return {},.Invalid_Data }
    count:=tiff_integer(encoded[int(offset):int(offset+count_bytes)],d.little)
    if count>4096 { return {},.Limit }
    size:=count*entry_bytes; offset+=count_bytes
    if size>u64(len(encoded))-offset { return {},.Invalid_Data }
    d.entries=encoded[int(offset):int(offset+size)]; d.count=int(count)
    metadata:=size
    previous:u16
    for i in 0..<d.count {
        field,ok:=tiff_entry(&d,i); if !ok { return {},.Invalid_Data }
        if i>0 && field.tag<=previous { return {},.Invalid_Data }; previous=field.tag
        metadata+=u64(len(field.data)); if metadata>4*1024*1024 { return {},.Limit }
    }
    return d,.None
}
@(private="package")
tiff_entry :: proc(d:^Tiff_Directory,index:int)->(Tiff_Field,bool) {
    stride:=20 if d.big else 12; entry:=d.entries[index*stride:(index+1)*stride]
    count_end:=12 if d.big else 8; inline_size:=8 if d.big else 4
    field:=Tiff_Field{tag=u16(tiff_integer(entry[:2],d.little)),kind=u16(tiff_integer(entry[2:4],d.little)),count=tiff_integer(entry[4:count_end],d.little)}
    element:=tiff_type_bytes(field.kind)
    if element==0 || field.count>u64(len(d.encoded))/element { return {},false }
    size:=field.count*element
    if size<=u64(inline_size) { field.data=entry[count_end:count_end+int(size)] }
    else {
        offset:=tiff_integer(entry[count_end:],d.little)
        if offset>u64(len(d.encoded)) || size>u64(len(d.encoded))-offset { return {},false }
        field.data=d.encoded[int(offset):int(offset+size)]
    }
    return field,true
}
@(private="package")
tiff_field :: proc(d:^Tiff_Directory,tag:u16)->(Tiff_Field,bool) {
    for i in 0..<d.count { field,ok:=tiff_entry(d,i); if ok && field.tag==tag { return field,true } }
    return {},false
}
@(private="package")
tiff_value :: proc(d:^Tiff_Directory,field:Tiff_Field,index:u64)->(u64,bool) {
    size:=tiff_type_bytes(field.kind)
    if index>=field.count || (field.kind!=1 && field.kind!=3 && field.kind!=4 && field.kind!=16) { return 0,false }
    return tiff_integer(field.data[int(index*size):int((index+1)*size)],d.little),true
}
@(private="package")
tiff_scalar :: proc(d:^Tiff_Directory,tag:u16,fallback:u64)->u64 {
    field,found:=tiff_field(d,tag); if !found { return fallback }
    value,ok:=tiff_value(d,field,0); if !ok || field.count!=1 { return max(u64) }
    return value
}
@(private="package")
tiff_uniform :: proc(d:^Tiff_Directory,tag:u16,samples:u64,fallback:u64)->u64 {
    field,found:=tiff_field(d,tag); if !found { return fallback }
    if field.count!=1 && field.count!=samples { return max(u64) }
    first,ok:=tiff_value(d,field,0); if !ok { return max(u64) }
    for i:u64=1; i<field.count; i+=1 { value,valid:=tiff_value(d,field,i); if !valid || value!=first { return max(u64) } }
    return first
}
@(private="package")
tiff_destination :: proc(x,y,w,h,orientation:u32)->int {
    dx,dy,out_w:=x,y,w
    switch orientation {
    case 2: dx=w-1-x
    case 3: dx=w-1-x; dy=h-1-y
    case 4: dy=h-1-y
    case 5: dx=y; dy=x; out_w=h
    case 6: dx=h-1-y; dy=x; out_w=h
    case 7: dx=h-1-y; dy=w-1-x; out_w=h
    case 8: dx=y; dy=w-1-x; out_w=h
    }
    return int(u64(dy)*u64(out_w)+u64(dx))*4
}
@(private="package")
tiff_packbits :: proc(input,out:[]byte)->bool {
    read,write:=0,0
    for read<len(input) && write<len(out) {
        code:=int(i8(input[read])); read+=1
        if code>=0 {
            count:=code+1; if count>len(input)-read || count>len(out)-write { return false }
            copy(out[write:write+count],input[read:read+count]); read+=count; write+=count
        } else if code!=-128 {
            count:=1-code; if read>=len(input) || count>len(out)-write { return false }
            for &value in out[write:write+count] { value=input[read] }; read+=1; write+=count
        }
    }
    return write==len(out)
}
@(private="package")
tiff_lzw :: proc(input,out:[]byte)->bool {
    prefix:[4096]u16; suffix:[4096]byte; stack:[4096]byte
    bit,write,width,next,previous:=0,0,9,258,-1; first:byte
    for bit+width<=len(input)*8 {
        code:=0
        for _ in 0..<width { code=code<<1|int((input[bit/8]>>u32(7-bit%8))&1); bit+=1 }
        if code==256 { width=9; next=258; previous=-1; continue }
        if code==257 { return write==len(out) }
        if previous==-1 {
            if code>255 || write>=len(out) { return false }; out[write]=byte(code); write+=1; previous=code; first=byte(code); continue
        }
        if code>next || code>=4096 { return false }
        current:=code; count:=0
        if current==next { stack[count]=first; count+=1; current=previous }
        for current>=256 {
            if current>=next || count>=len(stack)-1 { return false }
            stack[count]=suffix[current]; count+=1; current=int(prefix[current])
        }
        first=byte(current); stack[count]=first; count+=1
        if count>len(out)-write { return false }
        for i:=count-1; i>=0; i-=1 { out[write]=stack[i]; write+=1 }
        if next<4096 {
            prefix[next]=u16(previous); suffix[next]=first; next+=1
            if width<12 && next==(1<<u32(width))-1 { width+=1 }
        }
        previous=code
    }
    return false
}
@(private="package")
tiff_unpack :: proc(input,out:[]byte,compression:u64)->bool {
    switch compression {
    case 1: if len(input)<len(out) { return false }; copy(out,input); return true
    case 5: return tiff_lzw(input,out)
    case 8,32946: return texture_inflate(input,out)==.None
    case 32773: return tiff_packbits(input,out)
    }
    return false
}
@(private="package")
tiff_sample :: proc(data:[]byte,little:bool,format:Image_Format)->f32 {
    switch format {
    case .RGBA8: return f32(data[0])/255
    case .RGBA16: return f32(tiff_integer(data[:2],little))/65535
    case .RGBA32_Float: return transmute(f32)u32(tiff_integer(data[:4],little))
    }
    return 0
}
@(private="package")
tiff_store :: proc(data:[]byte,value:f32,format:Image_Format)->bool {
    if math.is_nan(value) || math.is_inf(value) || math.abs(value)>65504 { return false }
    switch format {
    case .RGBA8: data[0]=byte(math.round(clamp(value,0,1)*255))
    case .RGBA16: encoded:=transmute([2]byte)u16(math.round(clamp(value,0,1)*65535)); copy(data,encoded[:])
    case .RGBA32_Float: encoded:=transmute([4]byte)value; copy(data,encoded[:])
    }
    return true
}
@(private="package")
texture_tiff_decode :: proc(encoded:[]byte,allocator:mem.Allocator)->(Texture_Image,Texture_Image_Error) {
    d,error:=tiff_directory(encoded); if error!=.None { return {},error }
    w:=tiff_scalar(&d,256,0); h:=tiff_scalar(&d,257,0)
    if w==0 || h==0 { return {},.Invalid_Data }
    if w>TEXTURE_IMAGE_MAX_DIMENSION || h>TEXTURE_IMAGE_MAX_DIMENSION || w*h>TEXTURE_IMAGE_MAX_PIXELS { return {},.Limit }
    samples:=tiff_scalar(&d,277,1); bits:=tiff_uniform(&d,258,samples,1); sample_format:=tiff_uniform(&d,339,samples,1)
    compression:=tiff_scalar(&d,259,1); photo:=tiff_scalar(&d,262,0); planar:=tiff_scalar(&d,284,1)
    orientation:=tiff_scalar(&d,274,1); predictor:=tiff_scalar(&d,317,1)
    if orientation<1 || orientation>8 || planar<1 || planar>2 { return {},.Invalid_Data }
    if bits!=8 && bits!=16 && bits!=32 || sample_format!=1 && sample_format!=3 || sample_format==3 && bits!=32 || sample_format==1 && bits==32 { return {},.Unsupported }
    if compression!=1 && compression!=5 && compression!=8 && compression!=32946 && compression!=32773 && compression!=7 { return {},.Unsupported }
    base:u64=1
    if photo==2 || photo==6 { base=3 } else if photo==5 { base=4 } else if photo>3 { return {},.Unsupported }
    if samples<base || samples>base+1 || samples>5 { return {},.Invalid_Data }
    if photo==5 && tiff_scalar(&d,332,1)!=1 { return {},.Unsupported }
    if predictor!=1 && predictor!=2 && predictor!=3 || predictor==2 && sample_format==3 || predictor==3 && sample_format!=3 { return {},.Unsupported }
    format:=Image_Format.RGBA8; if bits==16 { format=.RGBA16 } else if bits==32 { format=.RGBA32_Float }
    channel_bytes:=int(bits/8)
    out_w,out_h:=u32(w),u32(h); if orientation>=5 { out_w,out_h=out_h,out_w }
    // Block geometry and source ranges are validated before any pixel allocation.
    tile_w:=tiff_scalar(&d,322,0); tile_h:=tiff_scalar(&d,323,0); tiled:=tile_w!=0 || tile_h!=0
    block_w,block_h:=w,min(h,tiff_scalar(&d,278,h))
    offsets,have_offsets:=tiff_field(&d,273); counts,have_counts:=tiff_field(&d,279)
    if tiled { block_w,block_h=tile_w,tile_h; offsets,have_offsets=tiff_field(&d,324); counts,have_counts=tiff_field(&d,325) }
    if block_w==0 || block_h==0 || block_w>TEXTURE_IMAGE_MAX_DIMENSION || block_h>TEXTURE_IMAGE_MAX_DIMENSION || !have_offsets || !have_counts { return {},.Invalid_Data }
    across:=(w+block_w-1)/block_w; down:=(h+block_h-1)/block_h
    planes:u64=1; if planar==2 { planes=samples }
    blocks:=across*down
    if offsets.count!=blocks*planes || counts.count!=offsets.count { return {},.Invalid_Data }
    for i:u64=0; i<offsets.count; i+=1 {
        offset,ok:=tiff_value(&d,offsets,i); size,valid:=tiff_value(&d,counts,i)
        if !ok || !valid || offset>u64(len(encoded)) || size>u64(len(encoded))-offset || size==0 { return {},.Invalid_Data }
    }
    extra:=tiff_scalar(&d,338,0)
    palette,has_palette:=tiff_field(&d,320)
    if photo==3 && (!has_palette || bits>16 || palette.kind!=3 || palette.count!=3*(u64(1)<<u32(bits))) { return {},.Invalid_Data }
    result,allocation:=texture_image_allocate(out_w,out_h,format,allocator); if allocation!=.None { return {},allocation }
    accepted:=false; defer if !accepted { texture_image_destroy(&result) }
    stored_channels:=samples if planar==1 else 1
    stride:=block_w*stored_channels*u64(channel_bytes)
    block_bytes:=stride*block_h
    if block_bytes>TEXTURE_IMAGE_MAX_BYTES/planes { return {},.Limit }
    block,block_error:=mem.make([]byte,int(block_bytes*planes),allocator)
    if block_error!=nil || block==nil { return {},.Allocation }; defer delete(block,allocator)
    for by:u64=0; by<down; by+=1 { for bx:u64=0; bx<across; bx+=1 {
        x,y:=bx*block_w,by*block_h; rows:=block_h if tiled else min(block_h,h-y)
        for plane:u64=0; plane<planes; plane+=1 {
            index:=plane*blocks+by*across+bx
            offset,_:=tiff_value(&d,offsets,index); size,_:=tiff_value(&d,counts,index)
            input:=encoded[int(offset):int(offset+size)]
            destination:=block[int(plane*block_bytes):int(plane*block_bytes+stride*rows)]
            if compression==7 {
                if bits!=8 || planar!=1 || predictor!=1 { return {},.Unsupported }
                tables,has_tables:=tiff_field(&d,347)
                joined:[]byte
                if has_tables {
                    if len(tables.data)<4 || len(input)<2 || string(tables.data[:2])!="\xff\xd8" || string(tables.data[len(tables.data)-2:])!="\xff\xd9" || string(input[:2])!="\xff\xd8" { return {},.Invalid_Data }
                    joined,block_error=mem.make([]byte,len(tables.data)+len(input)-4,allocator); if block_error!=nil { return {},.Allocation }; defer delete(joined,allocator)
                    copy(joined,tables.data[:len(tables.data)-2]); copy(joined[len(tables.data)-2:],input[2:]); input=joined
                }
                if len(input)<2 || input[0]!=255 || input[1]!=0xd8 { return {},.Invalid_Data }
                decoded,jpeg_error:=texture_jpeg_decode(input,allocator); if jpeg_error!=.None { return {},jpeg_error }; defer texture_image_destroy(&decoded)
                if decoded.width!=u32(block_w) || decoded.height!=u32(rows) { return {},.Invalid_Data }
                for py:u64=0; py<rows && y+py<h; py+=1 { for px:u64=0; px<block_w && x+px<w; px+=1 {
                    target:=tiff_destination(u32(x+px),u32(y+py),u32(w),u32(h),u32(orientation))
                    pixel:=int(py*block_w+px)*4; copy(result.pixels[target:target+4],decoded.pixels[pixel:pixel+4])
                } }
                continue
            }
            if photo==6 { return {},.Unsupported }
            if !tiff_unpack(input,destination,compression) { return {},.Invalid_Data }
            if predictor==2 {
                modulus:=u64(1)<<u32(bits)
                for row:u64=0; row<rows; row+=1 { for sample:=stored_channels; sample<block_w*stored_channels; sample+=1 {
                    pos:=int(row*stride+sample*u64(channel_bytes)); previous:=pos-int(stored_channels)*channel_bytes
                    value:=(tiff_integer(destination[pos:pos+channel_bytes],d.little)+tiff_integer(destination[previous:previous+channel_bytes],d.little))%modulus
                    for c in 0..<channel_bytes { shift:=c if d.little else channel_bytes-1-c; destination[pos+c]=byte(value>>u32(shift*8)) }
                } }
            } else if predictor==3 {
                scratch,scratch_error:=mem.make([]byte,int(stride),allocator); if scratch_error!=nil { return {},.Allocation }; defer delete(scratch,allocator)
                for row:u64=0; row<rows; row+=1 {
                    line:=destination[int(row*stride):int((row+1)*stride)]
                    for pos:=int(stored_channels); pos<len(line); pos+=1 { line[pos]+=line[pos-int(stored_channels)] }
                    count:=int(block_w*stored_channels)
                    for sample in 0..<count { for c in 0..<channel_bytes { target_c:=channel_bytes-1-c if d.little else c; scratch[sample*channel_bytes+target_c]=line[c*count+sample] } }
                    copy(line,scratch)
                }
            }
        }
        if compression==7 { continue }
        for py:u64=0; py<rows && y+py<h; py+=1 { for px:u64=0; px<block_w && x+px<w; px+=1 {
            values:=[5]f32{0,0,0,1,1}
            for channel:u64=0; channel<samples; channel+=1 {
                pos:=py*stride+(px*samples+channel)*u64(channel_bytes) if planar==1 else channel*block_bytes+py*stride+px*u64(channel_bytes)
                values[channel]=tiff_sample(block[int(pos):],d.little,format)
                if math.is_nan(values[channel]) || math.is_inf(values[channel]) || math.abs(values[channel])>65504 { return {},.Invalid_Data }
            }
            rgba:=[4]f32{0,0,0,1}
            if photo==2 { rgba[0],rgba[1],rgba[2]=values[0],values[1],values[2] }
            else if photo==5 { for c in 0..<3 { rgba[c]=(1-values[c])*(1-values[3]) } }
            else if photo==3 {
                palette_index:=u64(math.round(values[0]*f32((u64(1)<<u32(bits))-1)))
                for c in 0..<3 { pos:=int((u64(c)*(u64(1)<<u32(bits))+palette_index)*2); rgba[c]=f32(tiff_integer(palette.data[pos:pos+2],d.little))/65535 }
            } else { gray:=1-values[0] if photo==0 else values[0]; rgba[0],rgba[1],rgba[2]=gray,gray,gray }
            if samples>base { rgba[3]=values[base] }
            if extra==1 && rgba[3]!=0 { for c in 0..<3 { rgba[c]/=rgba[3] } }
            target:=tiff_destination(u32(x+px),u32(y+py),u32(w),u32(h),u32(orientation))
            for value,c in rgba { if !tiff_store(result.pixels[(target+c)*channel_bytes:],value,format) { return {},.Invalid_Data } }
        } }
    } }
    accepted=true; return result,.None
}
