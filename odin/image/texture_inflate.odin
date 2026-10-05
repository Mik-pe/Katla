//! RFC 1950/1951 inflation writes directly into an exact preflighted output slice.
package katla_image
import "core:hash"

@(private="package")
Deflate_Bits :: struct { input:[]byte,bit:int,failed:bool }
@(private="package")
Deflate_Huffman :: struct { counts:[16]int,first:[16]int,start:[16]int,values:[288]u16,valid:bool }
@(private="package")
deflate_bits :: #force_inline proc(b:^Deflate_Bits,count:int)->u32 {
    if count>len(b.input)*8-b.bit { b.failed=true; return 0 }
    value:u32
    taken:=0
    for taken<count {
        available:=min(count-taken,8-b.bit%8); mask:=(u32(1)<<u32(available))-1
        value|=(u32(b.input[b.bit/8])>>u32(b.bit%8)&mask)<<u32(taken)
        b.bit+=available; taken+=available
    }
    return value
}
@(private="package")
deflate_huffman :: proc(lengths:[]u8)->(Deflate_Huffman,bool) {
    table:Deflate_Huffman
    for length in lengths { if length>15 { return {},false }; if length!=0 { table.counts[length]+=1 } }
    code,total:=0,0
    for length in 1..=15 {
        table.first[length]=code; table.start[length]=total
        if code+table.counts[length]>1<<u32(length) { return {},false }
        code=(code+table.counts[length])<<1; total+=table.counts[length]
    }
    if total==0 { return table,false }
    positions:=table.start
    for length,symbol in lengths { if length!=0 { table.values[positions[length]]=u16(symbol); positions[length]+=1 } }
    table.valid=true; return table,true
}
@(private="package")
deflate_symbol :: #force_inline proc(b:^Deflate_Bits,table:^Deflate_Huffman)->int {
    if !table.valid { b.failed=true; return 0 }; code:=0
    for length in 1..=15 {
        code=code<<1|int(deflate_bits(b,1)); if b.failed { return 0 }
        delta:=code-table.first[length]
        if delta>=0 && delta<table.counts[length] { return int(table.values[table.start[length]+delta]) }
    }
    b.failed=true; return 0
}
@(private="package")
deflate_tables :: proc(b:^Deflate_Bits,kind:u32)->(Deflate_Huffman,Deflate_Huffman,bool) {
    lengths:[288]u8; distances:[32]u8
    if kind==1 {
        for &length,i in lengths { length=8 if i<144 else 9 if i<256 else 7 if i<280 else 8 }
        for &length in distances { length=5 }
        literal,ok:=deflate_huffman(lengths[:]); distance,valid:=deflate_huffman(distances[:]); return literal,distance,ok && valid
    }
    literal_count:=int(deflate_bits(b,5))+257; distance_count:=int(deflate_bits(b,5))+1; code_count:=int(deflate_bits(b,4))+4
    if b.failed || literal_count>286 || distance_count>32 { return {},{},false }
    order:=[19]int{16,17,18,0,8,7,9,6,10,5,11,4,12,3,13,2,14,1,15}; code_lengths:[19]u8
    for i in 0..<code_count { code_lengths[order[i]]=u8(deflate_bits(b,3)) }
    codes,ok:=deflate_huffman(code_lengths[:]); if !ok { return {},{},false }
    all:[318]u8; pos:=0; total:=literal_count+distance_count
    for pos<total {
        symbol:=deflate_symbol(b,&codes); if b.failed { return {},{},false }
        if symbol<=15 { all[pos]=u8(symbol); pos+=1; continue }
        count:int; value:u8
        if symbol==16 { if pos==0 { return {},{},false }; value=all[pos-1]; count=int(deflate_bits(b,2))+3 }
        else if symbol==17 { count=int(deflate_bits(b,3))+3 }
        else if symbol==18 { count=int(deflate_bits(b,7))+11 }
        else { return {},{},false }
        if b.failed || count>total-pos { return {},{},false }
        for &length in all[pos:pos+count] { length=value }; pos+=count
    }
    if all[256]==0 { return {},{},false }
    literal,valid:=deflate_huffman(all[:literal_count]); distance,_:=deflate_huffman(all[literal_count:total])
    return literal,distance,valid
}
@(private="package")
texture_inflate :: proc(encoded,out:[]byte)->Texture_Image_Error {
    if len(encoded)<6 || encoded[0]&15!=8 || encoded[0]>>4>7 || (u32(encoded[0])<<8|u32(encoded[1]))%31!=0 || encoded[1]&32!=0 { return .Invalid_Data }
    bits:=Deflate_Bits{input=encoded[2:len(encoded)-4]}; written:=0; final:bool
    length_base:=[29]int{3,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,67,83,99,115,131,163,195,227,258}
    length_extra:=[29]int{0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,0}
    distance_base:=[30]int{1,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,1025,1537,2049,3073,4097,6145,8193,12289,16385,24577}
    distance_extra:=[30]int{0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13}
    for !final {
        final=deflate_bits(&bits,1)!=0; kind:=deflate_bits(&bits,2); if bits.failed || kind==3 { return .Invalid_Data }
        if kind==0 {
            bits.bit=(bits.bit+7)/8*8
            count:=int(deflate_bits(&bits,16)); inverse:=int(deflate_bits(&bits,16))
            if bits.failed || count~inverse!=65535 || count>len(out)-written || count>(len(bits.input)*8-bits.bit)/8 { return .Invalid_Data }
            offset:=bits.bit/8; copy(out[written:written+count],bits.input[offset:offset+count]); written+=count; bits.bit+=count*8
            continue
        }
        literal,distance,ok:=deflate_tables(&bits,kind); if !ok { return .Invalid_Data }
        for {
            symbol:=deflate_symbol(&bits,&literal); if bits.failed { return .Invalid_Data }
            if symbol<256 { if written>=len(out) { return .Invalid_Data }; out[written]=byte(symbol); written+=1; continue }
            if symbol==256 { break }
            if symbol>285 { return .Invalid_Data }
            index:=symbol-257; count:=length_base[index]+int(deflate_bits(&bits,length_extra[index]))
            dist_symbol:=deflate_symbol(&bits,&distance); if bits.failed || dist_symbol>29 { return .Invalid_Data }
            back:=distance_base[dist_symbol]+int(deflate_bits(&bits,distance_extra[dist_symbol]))
            if bits.failed || back>written || count>len(out)-written { return .Invalid_Data }
            for _ in 0..<count { out[written]=out[written-back]; written+=1 }
        }
    }
    if written!=len(out) || (bits.bit+7)/8!=len(bits.input) || hash.adler32(out)!=texture_be32(encoded[len(encoded)-4:]) { return .Invalid_Data }
    return .None
}
