//! Owned immutable PCM and bounded byte decoders. Filesystem confinement belongs to the caller.
package katla_audio
import native "../deps/audio"
import "core:mem"
import "core:slice"
import "core:sync"

Error :: enum {None,Invalid_Parameter,Unsupported,Decode_Failed,Capacity_Exceeded,Device_Not_Found,Device_Failed,Stale_Handle,Pool_Full,Not_Initialized,Already_Initialized,IO,Queue_Full,Unconfigured}
Format :: enum u32 {Wav,Ogg,Mp3,Flac,PCM}
Metadata :: struct {format:Format,channels,sample_rate:u32,frames,sample_count:u64,duration_seconds:f64}
/// Immutable shared PCM. Retain/release every owner, including each playing voice.
Clip :: struct {samples:[]f32,metadata:Metadata,allocator:mem.Allocator,references:u32}
MAX_ENCODED_BYTES :: 64*1024*1024
MAX_PCM_SAMPLES :: 64*1024*1024

@(private)
codec_format :: proc(bytes:[]u8)->(Format,Error) {
    if len(bytes)>=12 && string(bytes[:4])=="RIFF" && string(bytes[8:12])=="WAVE" {return .Wav,.None}
    if len(bytes)>=4 && string(bytes[:4])=="OggS" {return .Ogg,.None}
    if len(bytes)>=4 && string(bytes[:4])=="fLaC" {return .Flac,.None}
    if len(bytes)>=3 && string(bytes[:3])=="ID3" || len(bytes)>=2 && bytes[0]==0xff && bytes[1]&0xe0==0xe0 {return .Mp3,.None}
    return {},.Unsupported
}
@(private)
metadata_from_native :: proc(info:native.Metadata)->Metadata {return {Format(info.format),info.channels,info.sample_rate,info.frames,info.frames*u64(info.channels),f64(info.frames)/f64(info.sample_rate)}}
/// Parse exact source frame count without allocating decoded PCM.
metadata :: proc(bytes:[]u8)->(Metadata,Error) {
    if native.abi_version()!=1 {return {},.Unsupported}
    if len(bytes)==0 || len(bytes)>MAX_ENCODED_BYTES {return {},.Invalid_Parameter}
    format,err:=codec_format(bytes);if err!=.None {return {},err}
    decoder:native.Decoder;info:native.Metadata
    if native.decoder_open(raw_data(bytes),uintptr(len(bytes)),u32(format),&decoder,&info)!=0 {return {},.Decode_Failed}
    defer native.decoder_close(decoder)
    return metadata_from_native(info),.None
}
/// Decode bounded immutable interleaved f32 PCM using the originating allocator.
clip_decode :: proc(bytes:[]u8,allocator:=context.allocator)->(^Clip,Error) {
    if native.abi_version()!=1 {return nil,.Unsupported}
    if len(bytes)==0 || len(bytes)>MAX_ENCODED_BYTES {return nil,.Invalid_Parameter}
    format,err:=codec_format(bytes);if err!=.None {return nil,err}
    decoder:native.Decoder;info:native.Metadata
    if native.decoder_open(raw_data(bytes),uintptr(len(bytes)),u32(format),&decoder,&info)!=0 {return nil,.Decode_Failed}
    defer native.decoder_close(decoder)
    meta:=metadata_from_native(info)
    if meta.sample_count>MAX_PCM_SAMPLES {return nil,.Capacity_Exceeded}
    samples:=make([]f32,int(meta.sample_count),allocator)
    offset:u64
    for offset<meta.frames {
        count:=min(u64(65536),meta.frames-offset);read:u64
        if native.decoder_read(decoder,raw_data(samples[int(offset*u64(meta.channels)):]),count,&read)!=0 || read!=count {delete(samples,allocator);return nil,.Decode_Failed}
        offset+=read
    }
    for sample in samples {if !finite(sample) {delete(samples,allocator);return nil,.Decode_Failed}}
    clip:=new(Clip,allocator);clip^={samples,meta,allocator,1}
    return clip,.None
}
/// Copy caller PCM into an immutable clip. Frame cardinality and all samples must be valid.
clip_from_pcm :: proc(samples:[]f32,channels,sample_rate:u32,allocator:=context.allocator)->(^Clip,Error) {
    if len(samples)==0 || len(samples)>MAX_PCM_SAMPLES || channels==0 || channels>16 || sample_rate==0 || sample_rate>384000 || len(samples)%int(channels)!=0 {return nil,.Invalid_Parameter}
    for sample in samples {if !finite(sample) {return nil,.Invalid_Parameter}}
    clip:=new(Clip,allocator);frames:=u64(len(samples)/int(channels))
    clip^={slice.clone(samples,allocator),{.PCM,channels,sample_rate,frames,u64(len(samples)),f64(frames)/f64(sample_rate)},allocator,1}
    return clip,.None
}
/// Retain a stable immutable clip before handing ownership to another consumer.
clip_retain :: proc(clip:^Clip) {if clip!=nil {sync.atomic_add(&clip.references,1)}}
/// Release outside the audio callback. The final owner frees samples and clip together.
clip_release :: proc(clip:^Clip) {if clip!=nil && sync.atomic_sub(&clip.references,1)==1 {allocator:=clip.allocator;delete(clip.samples,allocator);free(clip,allocator)}}
@(private)
finite :: proc(value:f32)->bool {return value==value && value>=-3.4028234e38 && value<=3.4028234e38}

/// Convert signed 16-bit PCM to the canonical immutable f32 representation.
clip_from_i16 :: proc(samples:[]i16,channels,sample_rate:u32,allocator:=context.allocator)->(^Clip,Error) {
    if len(samples)==0||len(samples)>MAX_PCM_SAMPLES {return nil,.Invalid_Parameter}
    decoded:=make([]f32,len(samples),allocator);defer delete(decoded,allocator)
    for sample,i in samples {decoded[i]=f32(sample)/32767}
    return clip_from_pcm(decoded,channels,sample_rate,allocator)
}
