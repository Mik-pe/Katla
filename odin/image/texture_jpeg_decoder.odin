//! One bounded JPEG decoder handles sequential and progressive Huffman scans in Odin.
package katla_image
import "core:mem"
import "core:math"

@(private="package")
JPEG_ZIGZAG := [64]int{0,1,8,16,9,2,3,10,17,24,32,25,18,11,4,5,12,19,26,33,40,48,41,34,27,20,13,6,7,14,21,28,35,42,49,56,57,50,43,36,29,22,15,23,30,37,44,51,58,59,52,45,38,31,39,46,53,60,61,54,47,55,62,63}
@(private="package")
Jpeg_Huffman :: struct { counts:[17]int,first:[17]int,start:[17]int,values:[256]byte,valid:bool }
@(private="package")
Jpeg_Component :: struct { id,h,v,quant:int,columns,rows,stride,padded_rows:int,coefficients:[][64]i32,plane:[]byte,progress:[64]i8,predictor:i32 }
@(private="package")
Jpeg_Decoder :: struct { width,height,hmax,vmax,mcux,mcuy:int,components:[4]Jpeg_Component,count:int,quant:[4][64]u16,have_quant:[4]bool,huffman:[2][4]Jpeg_Huffman,restart:int,progressive:bool,adobe,transform:int,allocator:mem.Allocator }
@(private="package")
Jpeg_Bits :: struct { encoded:[]byte,offset:int,bits:u8,value:byte,failed:bool }
@(private="package")
jpeg_bits :: #force_inline proc(b:^Jpeg_Bits,count:int)->(value:u32) {
    for _ in 0..<count {
        if b.bits==0 {
            if b.offset>=len(b.encoded) { b.failed=true; return 0 }
            b.value=b.encoded[b.offset]; b.offset+=1
            if b.value==255 {
                if b.offset>=len(b.encoded) || b.encoded[b.offset]!=0 { b.failed=true; return 0 }; b.offset+=1
            }
            b.bits=8
        }
        b.bits-=1; value=value<<1|u32(b.value>>b.bits&1)
    }
    return
}
@(private="package")
jpeg_symbol :: #force_inline proc(b:^Jpeg_Bits,table:^Jpeg_Huffman)->int {
    if !table.valid { b.failed=true; return 0 }
    code:=0
    for length in 1..=16 {
        code=code<<1|int(jpeg_bits(b,1)); if b.failed { return 0 }
        delta:=code-table.first[length]
        if delta>=0 && delta<table.counts[length] { return int(table.values[table.start[length]+delta]) }
    }
    b.failed=true; return 0
}
@(private="package")
jpeg_extend :: #force_inline proc(b:^Jpeg_Bits,count:int)->i32 {
    if count==0 { return 0 }; value:=i32(jpeg_bits(b,count)); threshold:=i32(1)<<u32(count-1)
    return value if value>=threshold else value-(i32(1)<<u32(count))+1
}
@(private="package")
jpeg_refine :: #force_inline proc(b:^Jpeg_Bits,value:^i32,bit:i32) {
    if jpeg_bits(b,1)!=0 && abs(value^)&bit==0 { value^+=bit if value^>0 else -bit }
}
@(private="package")
jpeg_predict :: proc(b:^Jpeg_Bits,component:^Jpeg_Component,length:int)->bool {
    value:=i64(component.predictor)+i64(jpeg_extend(b,length))
    if b.failed || value < -2047 || value > 2047 { return false }
    component.predictor=i32(value); return true
}
@(private="package")
jpeg_block :: proc(d:^Jpeg_Decoder,b:^Jpeg_Bits,component:^Jpeg_Component,block:^[64]i32,dc,ac,ss,se,ah,al:int,eob:^int)->bool {
    shift:=u32(al); step:=i32(1)<<shift
    if !d.progressive {
        length:=jpeg_symbol(b,&d.huffman[0][dc]); if length>11 { return false }
        if !jpeg_predict(b,component,length) { return false }; block[0]=component.predictor
        k:=1
        for k<64 {
            symbol:=jpeg_symbol(b,&d.huffman[1][ac]); run,size:=symbol>>4,symbol&15
            if size==0 { if run==0 { break }; if run!=15 { return false }; k+=16; continue }
            if size>10 { return false }; k+=run; if k>=64 { return false }
            block[JPEG_ZIGZAG[k]]=jpeg_extend(b,size); k+=1
        }
        return !b.failed && k<=64
    }
    if ss==0 {
        if ah==0 {
            length:=jpeg_symbol(b,&d.huffman[0][dc]); if length>11 { return false }
            if !jpeg_predict(b,component,length) { return false }; block[0]=component.predictor<<shift
        } else { block[0]|=i32(jpeg_bits(b,1))<<shift }
        return !b.failed
    }
    k:=ss
    if ah==0 {
        if eob^>0 { eob^-=1; return true }
        for k<=se {
            symbol:=jpeg_symbol(b,&d.huffman[1][ac]); run,size:=symbol>>4,symbol&15
            if size==0 {
                if run!=15 { eob^=(1<<u32(run))+int(jpeg_bits(b,run))-1; break }
                k+=16; if k>se+1 { return false }; continue
            }
            if size>10 { return false }; k+=run; if k>se { return false }
            block[JPEG_ZIGZAG[k]]=jpeg_extend(b,size)<<shift; k+=1
        }
    } else {
        if eob^==0 {
            for k<=se {
                symbol:=jpeg_symbol(b,&d.huffman[1][ac]); run,size:=symbol>>4,symbol&15
                inserted:i32
                if size==0 && run!=15 { eob^=(1<<u32(run))+int(jpeg_bits(b,run)); break }
                if size!=0 { if size!=1 { return false }; inserted=step if jpeg_bits(b,1)!=0 else -step }
                else { run=16 }
                for k<=se {
                    value:=&block[JPEG_ZIGZAG[k]]
                    if value^!=0 { jpeg_refine(b,value,step) }
                    else {
                        if run==0 { break }; run-=1
                        if run==0 && inserted==0 { k+=1; break }
                    }
                    k+=1
                }
                if inserted!=0 { if k>se { return false }; block[JPEG_ZIGZAG[k]]=inserted; k+=1 }
                else if run!=0 { return false }
            }
        }
        if eob^>0 {
            for k<=se { value:=&block[JPEG_ZIGZAG[k]]; if value^!=0 { jpeg_refine(b,value,step) }; k+=1 }
            eob^-=1
        }
    }
    return !b.failed
}
@(private="package")
jpeg_scan :: proc(d:^Jpeg_Decoder,header:[]byte,encoded:[]byte,offset:^int)->bool {
    if len(header)<4 { return false }; count:=int(header[0])
    if count<1 || count>d.count || len(header)!=1+count*2+3 { return false }
    indices,dc,ac:[4]int; seen:[4]bool
    for i in 0..<count {
        index:=-1; for component,c in d.components[:d.count] { if component.id==int(header[1+i*2]) { index=c } }
        if index<0 || seen[index] { return false }; seen[index]=true; indices[i]=index
        dc[i]=int(header[2+i*2]>>4); ac[i]=int(header[2+i*2]&15); if dc[i]>3 || ac[i]>3 { return false }
    }
    ss,se,ah,al:=int(header[1+count*2]),int(header[2+count*2]),int(header[3+count*2]>>4),int(header[3+count*2]&15)
    if ss>se || se>63 || ah>13 || al>13 { return false }
    if d.progressive {
        if ss==0 && se!=0 || ss!=0 && count!=1 || ah!=0 && al!=ah-1 { return false }
    } else if ss!=0 || se!=63 || ah!=0 || al!=0 { return false }
    for i in 0..<count {
        component:=&d.components[indices[i]]
        if !d.have_quant[component.quant] { return false }
        for k in ss..=se {
            if ah==0 && component.progress[k]!=-1 || ah!=0 && component.progress[k]!=i8(ah) { return false }
            component.progress[k]=i8(al)
        }
        component.predictor=0
    }
    b:=Jpeg_Bits{encoded=encoded,offset=offset^}
    across,down:=d.mcux,d.mcuy
    if count==1 { component:=&d.components[indices[0]]; across=component.columns; down=component.rows }
    eob,units,restart_index:int
    for y in 0..<down { for x in 0..<across {
        if d.restart>0 && units>0 && units%d.restart==0 {
            if eob!=0 { return false }; b.bits=0
            if b.offset+2>len(encoded) || encoded[b.offset]!=255 { return false }
            for b.offset<len(encoded) && encoded[b.offset]==255 { b.offset+=1 }
            if b.offset>=len(encoded) || encoded[b.offset]!=byte(0xd0+restart_index) { return false }
            b.offset+=1; restart_index=(restart_index+1)%8
            for &component in d.components[:d.count] { component.predictor=0 }
        }
        for i in 0..<count {
            component:=&d.components[indices[i]]; bh,bv:=1,1
            if count>1 { bh,bv=component.h,component.v }
            for v in 0..<bv { for h in 0..<bh {
                index:=(y*bv+v)*component.stride+x*bh+h
                if !jpeg_block(d,&b,component,&component.coefficients[index],dc[i],ac[i],ss,se,ah,al,&eob) { return false }
            } }
        }
        units+=1
    } }
    if eob!=0 || b.failed { return false }
    offset^=b.offset; return true
}
@(private="package")
jpeg_frame :: proc(d:^Jpeg_Decoder,header:[]byte)->Texture_Image_Error {
    if d.count!=0 || len(header)<6 || header[0]!=8 { return .Unsupported }
    d.height=int(header[1])<<8|int(header[2]); d.width=int(header[3])<<8|int(header[4]); d.count=int(header[5])
    if d.count<1 || d.count>4 || len(header)!=6+d.count*3 || d.width==0 || d.height==0 { return .Invalid_Data }
    if d.width>TEXTURE_IMAGE_MAX_DIMENSION || d.height>TEXTURE_IMAGE_MAX_DIMENSION || u64(d.width)*u64(d.height)>TEXTURE_IMAGE_MAX_PIXELS { return .Limit }
    sampling:=0
    for &component,i in d.components[:d.count] {
        component.id=int(header[6+i*3]); component.h=int(header[7+i*3]>>4); component.v=int(header[7+i*3]&15); component.quant=int(header[8+i*3])
        for previous in d.components[:i] { if previous.id==component.id { return .Invalid_Data } }
        if component.h<1 || component.h>4 || component.v<1 || component.v>4 || component.quant>3 { return .Invalid_Data }
        d.hmax=max(d.hmax,component.h); d.vmax=max(d.vmax,component.v); sampling+=component.h*component.v
        for &value in component.progress { value=-1 }
    }
    if sampling>10 { return .Unsupported }
    d.mcux=(d.width+8*d.hmax-1)/(8*d.hmax); d.mcuy=(d.height+8*d.vmax-1)/(8*d.vmax)
    working:u64
    for &component in d.components[:d.count] {
        component.columns=(d.width*component.h+8*d.hmax-1)/(8*d.hmax); component.rows=(d.height*component.v+8*d.vmax-1)/(8*d.vmax)
        component.stride=d.mcux*component.h; component.padded_rows=d.mcuy*component.v
        working+=u64(component.stride)*u64(component.padded_rows)*(64*4+64)
    }
    if working>256*1024*1024 { return .Limit }
    for &component in d.components[:d.count] {
        allocation:mem.Allocator_Error
        component.coefficients,allocation=mem.make([][64]i32,component.stride*component.padded_rows,d.allocator)
        if allocation!=nil || component.coefficients==nil { return .Allocation }
        component.plane,allocation=mem.make([]byte,component.stride*component.padded_rows*64,d.allocator)
        if allocation!=nil || component.plane==nil { return .Allocation }
    }
    return .None
}
@(private="package")
jpeg_idct :: proc(block:^[64]i32,quant:^[64]u16,basis:^[8][8]f64,out:[]byte,stride:int) {
    dc_only:=true; for value in block[1:] { if value!=0 { dc_only=false; break } }
    if dc_only {
        value:=byte(clamp(math.round(f64(block[0])*f64(quant[0])/8+128),0,255))
        for y in 0..<8 { for x in 0..<8 { out[y*stride+x]=value } }; return
    }
    intermediate:[8][8]f64
    for v in 0..<8 { for x in 0..<8 {
        sum:f64; for u in 0..<8 { sum+=basis[x][u]*f64(block[v*8+u])*f64(quant[v*8+u]) }; intermediate[v][x]=sum
    } }
    for y in 0..<8 { for x in 0..<8 {
        sum:f64; for v in 0..<8 { sum+=basis[y][v]*intermediate[v][x] }
        out[y*stride+x]=byte(clamp(math.round(sum/4+128),0,255))
    } }
}
@(private="package")
jpeg_channel :: proc(d:^Jpeg_Decoder,component:^Jpeg_Component,x,y:int)->f64 {
    px:=(f64(x)+0.5)*f64(component.h)/f64(d.hmax)-0.5
    py:=(f64(y)+0.5)*f64(component.v)/f64(d.vmax)-0.5
    width:=(d.width*component.h+d.hmax-1)/d.hmax; height:=(d.height*component.v+d.vmax-1)/d.vmax; stride:=component.stride*8
    px=clamp(px,0,f64(width-1)); py=clamp(py,0,f64(height-1))
    x0,y0:=int(px),int(py); x1,y1:=min(x0+1,width-1),min(y0+1,height-1); fx,fy:=px-f64(x0),py-f64(y0)
    top:=f64(component.plane[y0*stride+x0])*(1-fx)+f64(component.plane[y0*stride+x1])*fx
    bottom:=f64(component.plane[y1*stride+x0])*(1-fx)+f64(component.plane[y1*stride+x1])*fx
    return top*(1-fy)+bottom*fy
}
@(private="package")
texture_jpeg_decode :: proc(encoded:[]byte,allocator:mem.Allocator)->(Texture_Image,Texture_Image_Error) {
    d:=Jpeg_Decoder{allocator=allocator,adobe=-1}
    defer for &component in d.components { delete(component.coefficients,allocator); delete(component.plane,allocator) }
    offset:=2; scans:=0; ended:=false
    for offset<len(encoded) {
        if encoded[offset]!=255 { return {},.Invalid_Data }
        for offset<len(encoded) && encoded[offset]==255 { offset+=1 }
        if offset>=len(encoded) { return {},.Invalid_Data }; marker:=encoded[offset]; offset+=1
        if marker==0xd9 { if scans==0 || offset!=len(encoded) { return {},.Invalid_Data }; ended=true; break }
        if marker==0 || marker==0xd8 || marker>=0xd0 && marker<=0xd7 { return {},.Invalid_Data }
        if offset+2>len(encoded) { return {},.Invalid_Data }
        size:=int(encoded[offset])<<8|int(encoded[offset+1]); if size<2 || size>len(encoded)-offset { return {},.Invalid_Data }
        header:=encoded[offset+2:offset+size]; offset+=size
        switch marker {
        case 0xc0,0xc1,0xc2:
            d.progressive=marker==0xc2
            if error:=jpeg_frame(&d,header); error!=.None { return {},error }
        case 0xdb:
            pos:=0
            for pos<len(header) {
                precision,index:=int(header[pos]>>4),int(header[pos]&15); pos+=1
                if precision>1 || index>3 || len(header)-pos<64*(precision+1) { return {},.Invalid_Data }
                if d.have_quant[index] && scans>0 { return {},.Unsupported }
                for k in 0..<64 {
                    value:=u16(header[pos]); pos+=1
                    if precision==1 { value=value<<8|u16(header[pos]); pos+=1 }
                    if value==0 { return {},.Invalid_Data }; d.quant[index][JPEG_ZIGZAG[k]]=value
                }
                d.have_quant[index]=true
            }
        case 0xc4:
            pos:=0
            for pos<len(header) {
                if len(header)-pos<17 { return {},.Invalid_Data }
                kind,index:=int(header[pos]>>4),int(header[pos]&15); pos+=1
                if kind>1 || index>3 { return {},.Invalid_Data }
                table:=&d.huffman[kind][index]; table^={}; total,code:=0,0
                for length in 1..=16 {
                    count:=int(header[pos]); pos+=1; table.counts[length]=count; table.first[length]=code; table.start[length]=total
                    if code+count>1<<u32(length) { return {},.Invalid_Data }; code=(code+count)<<1; total+=count
                }
                if total==0 || total>256 || total>len(header)-pos { return {},.Invalid_Data }
                copy(table.values[:total],header[pos:pos+total]); pos+=total; table.valid=true
            }
        case 0xdd: if len(header)!=2 { return {},.Invalid_Data }; d.restart=int(header[0])<<8|int(header[1])
        case 0xda:
            scans+=1; if scans>1024 || d.count==0 || !jpeg_scan(&d,header,encoded,&offset) { return {},.Invalid_Data }
        case 0xee:
            if len(header)>=12 && string(header[:5])=="Adobe" { d.adobe=1; d.transform=int(header[11]) }
        case 0xe0..=0xef,0xfe:
        case: return {},.Unsupported
        }
    }
    if !ended || d.count==2 { return {},.Invalid_Data }
    basis:[8][8]f64
    for x in 0..<8 { for u in 0..<8 { basis[x][u]=(1/math.sqrt(f64(2)) if u==0 else 1)*math.cos(f64((2*x+1)*u)*math.PI/16) } }
    for &component in d.components[:d.count] {
        if component.progress[0]<0 { return {},.Invalid_Data }
        for y in 0..<component.padded_rows { for x in 0..<component.stride {
            jpeg_idct(&component.coefficients[y*component.stride+x],&d.quant[component.quant],&basis,component.plane[(y*8*component.stride+x)*8:],component.stride*8)
        } }
    }
    result,error:=texture_image_allocate(u32(d.width),u32(d.height),.RGBA8,allocator); if error!=.None { return {},error }
    for y in 0..<d.height { for x in 0..<d.width {
        channels:[4]f64
        for &component,c in d.components[:d.count] { channels[c]=jpeg_channel(&d,&component,x,y) }
        rgb:[3]f64
        if d.count==1 { rgb={channels[0],channels[0],channels[0]} }
        else if d.count==3 && (d.adobe==1 && d.transform==0 || d.components[0].id=='R' && d.components[1].id=='G' && d.components[2].id=='B') { rgb={channels[0],channels[1],channels[2]} }
        else {
            yy,cb,cr:=channels[0],channels[1]-128,channels[2]-128
            rgb={yy+1.402*cr,yy-0.344136*cb-0.714136*cr,yy+1.772*cb}
            if d.count==4 && d.transform==2 { for &value in rgb { value=(255-clamp(value,0,255))*channels[3]/255 } }
        }
        if d.count==4 && d.transform!=2 {
            for &value,c in rgb { value=channels[c]*channels[3]/255 if d.adobe==1 else (255-channels[c])*(255-channels[3])/255 }
        }
        pos:=(y*d.width+x)*4
        for value,c in rgb { result.pixels[pos+c]=byte(clamp(math.round(value),0,255)) }; result.pixels[pos+3]=255
    } }
    return result,.None
}
