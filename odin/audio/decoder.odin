//! Main-thread incremental decoding shares the same bounded native codec ABI as worker streams.
package katla_audio
import native "../deps/audio"
import "core:mem"
import "core:slice"
/// Owns encoded bytes; the native decoder borrows them until decoder_destroy.
Decoder :: struct {encoded:[]u8,native:native.Decoder,metadata:Metadata,frame:u64,allocator:mem.Allocator}
/// Open immutable bytes for exact metadata, incremental reads and seeks. No filenames cross this boundary.
decoder_open :: proc(bytes:[]u8,allocator:=context.allocator)->(^Decoder,Error) {
    if len(bytes)==0||len(bytes)>MAX_ENCODED_BYTES {return nil,.Invalid_Parameter}
    if native.abi_version()!=1 {return nil,.Unsupported}
    format,error:=codec_format(bytes);if error!=.None {return nil,error}
    decoder:=new(Decoder,allocator);decoder.allocator=allocator;decoder.encoded=slice.clone(bytes,allocator)
    info:native.Metadata
    if native.decoder_open(raw_data(decoder.encoded),uintptr(len(bytes)),u32(format),&decoder.native,&info)!=0 {delete(decoder.encoded,allocator);free(decoder,allocator);return nil,.Decode_Failed}
    decoder.metadata=metadata_from_native(info);return decoder,.None
}
/// Close the decoder before releasing its retained encoded allocation.
decoder_destroy :: proc(decoder:^Decoder) {if decoder!=nil {native.decoder_close(decoder.native);allocator:=decoder.allocator;delete(decoder.encoded,allocator);free(decoder,allocator)}}
/// Fill caller PCM storage, returning actual frames read; zero frames marks normal end of stream.
decoder_read :: proc(decoder:^Decoder,output:[]f32)->(u64,Error) {
    if decoder==nil||len(output)%int(decoder.metadata.channels)!=0||len(output)/int(decoder.metadata.channels)>65536 {return 0,.Invalid_Parameter}
    if len(output)==0||decoder.frame>=decoder.metadata.frames {return 0,.None}
    frames:u64
    if native.decoder_read(decoder.native,raw_data(output),u64(len(output)/int(decoder.metadata.channels)),&frames)!=0 {return 0,.Decode_Failed}
    for sample in output[:int(frames)*int(decoder.metadata.channels)] {if !finite(sample) {return 0,.Decode_Failed}}
    decoder.frame+=frames;return frames,.None
}
/// Return one independently owned PCM block; an exhausted decoder returns nil and None.
decoder_read_chunk :: proc(decoder:^Decoder,max_frames:u32=4096)->(^Clip,Error) {
    if decoder==nil||max_frames==0||max_frames>65536 {return nil,.Invalid_Parameter}
    if decoder.frame>=decoder.metadata.frames {return nil,.None}
    frames:=min(u64(max_frames),decoder.metadata.frames-decoder.frame);samples:=make([]f32,int(frames)*int(decoder.metadata.channels),decoder.allocator)
    read,error:=decoder_read(decoder,samples)
    if error!=.None||read!=frames {delete(samples,decoder.allocator);return nil,.Decode_Failed}
    clip:=new(Clip,decoder.allocator);meta:=decoder.metadata;meta.frames=read;meta.sample_count=u64(len(samples));meta.duration_seconds=f64(read)/f64(meta.sample_rate)
    clip^={samples,meta,decoder.allocator,1};return clip,.None
}
/// Seek to an exact source frame, including the terminal frame; callers hold sole decoder ownership.
decoder_seek :: proc(decoder:^Decoder,seconds:f64)->Error {
    if decoder==nil||seconds!=seconds||seconds<0||seconds>decoder.metadata.duration_seconds {return .Invalid_Parameter}
    frame:=min(u64(seconds*f64(decoder.metadata.sample_rate)),decoder.metadata.frames)
    if frame==decoder.metadata.frames {decoder.frame=frame;return .None}
    if native.decoder_seek(decoder.native,frame)!=0 {return .Decode_Failed};decoder.frame=frame;return .None
}
