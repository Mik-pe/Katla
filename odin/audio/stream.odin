//! Encoded byte ownership and a decoding worker feeding a bounded stereo-consumed ring.
package katla_audio
import native "../deps/audio"
import "core:mem"
import "core:slice"
import "core:sync"
import "core:thread"
import "core:time"
import "core:math"
/// The worker alone accesses the decoder; callback and worker only copy ring samples under a short mutex.
Stream :: struct {
    encoded:[]u8,decoder:native.Decoder,metadata:Metadata,ring:[]f32,scratch,head,loop_pcm:[]f32,
    mutex:sync.Mutex,worker:^thread.Thread,allocator:mem.Allocator,
    read_frame,write_frame,decode_frame,seek_frame:u64,fraction,underrun_frames:u64,
    looping,exhausted,closing,seek_pending:bool,error:Error,
}
@(private)
stream_create :: proc(bytes:[]u8,looping:bool,allocator:mem.Allocator)->(^Stream,Error) {
    if len(bytes)==0||len(bytes)>MAX_ENCODED_BYTES {return nil,.Invalid_Parameter}
    format,err:=codec_format(bytes);if err!=.None {return nil,err}
    stream:=new(Stream,allocator);stream.allocator=allocator;stream.encoded=slice.clone(bytes,allocator);stream.looping=looping
    info:native.Metadata
    if native.decoder_open(raw_data(stream.encoded),uintptr(len(bytes)),u32(format),&stream.decoder,&info)!=0 {delete(stream.encoded,allocator);free(stream,allocator);return nil,.Decode_Failed}
    stream.metadata=metadata_from_native(info);channels:=int(info.channels)
    capacity:=int(min(u64(info.sample_rate)*4,u64(1048576)))
    stream.ring=make([]f32,capacity*channels,allocator);stream.scratch=make([]f32,4096*channels,allocator);stream.head=make([]f32,256*channels,allocator)
    // A bounded small loop is decoded once so one-frame assets can fill a full callback block.
    if looping&&info.frames<=4096 {
        stream.loop_pcm=make([]f32,int(info.frames)*channels,allocator)
        read:u64
        if native.decoder_read(stream.decoder,raw_data(stream.loop_pcm),info.frames,&read)!=0||read!=info.frames {stream_destroy(stream);return nil,.Decode_Failed}
        for sample in stream.loop_pcm {if !finite(sample) {stream_destroy(stream);return nil,.Decode_Failed}}
    }
    stream_fill(stream)
    if stream.error!=.None {error:=stream.error;stream_destroy(stream);return nil,error}
    context.allocator=allocator
    worker:=thread.create(stream_worker,name="Audio decoder")
    if worker==nil {stream_destroy(stream);return nil,.Capacity_Exceeded}
    stream.worker=worker;worker.data=stream;thread.start(worker);return stream,.None
}
@(private)
stream_destroy :: proc(stream:^Stream) {
    if stream==nil {return}
    sync.mutex_lock(&stream.mutex);stream.closing=true;sync.mutex_unlock(&stream.mutex)
    if stream.worker!=nil {thread.join(stream.worker);thread.destroy(stream.worker)}
    native.decoder_close(stream.decoder);allocator:=stream.allocator
    delete(stream.encoded,allocator);delete(stream.ring,allocator);delete(stream.scratch,allocator);delete(stream.head,allocator);delete(stream.loop_pcm,allocator);free(stream,allocator)
}
@(private)
stream_worker :: proc(worker:^thread.Thread) {
    stream:=cast(^Stream)worker.data
    for {
        sync.mutex_lock(&stream.mutex);closing:=stream.closing;sync.mutex_unlock(&stream.mutex)
        if closing {break}
        stream_fill(stream)
        time.sleep(2*time.Millisecond)
    }
}
@(private)
stream_fill :: proc(stream:^Stream) {
    channels:=int(stream.metadata.channels);capacity:=u64(len(stream.ring)/channels)
    sync.mutex_lock(&stream.mutex)
    seek:=stream.seek_pending;target:=stream.seek_frame
    if seek {stream.seek_pending=false;stream.read_frame=0;stream.write_frame=0;stream.fraction=0;stream.exhausted=false}
    space:=capacity-2-(stream.write_frame-stream.read_frame)
    done:=stream.exhausted
    sync.mutex_unlock(&stream.mutex)
    if seek {
        if len(stream.loop_pcm)==0&&native.decoder_seek(stream.decoder,target)!=0 {sync.mutex_lock(&stream.mutex);stream.error=.Decode_Failed;stream.exhausted=true;sync.mutex_unlock(&stream.mutex);return}
        stream.decode_frame=target
    }
    if done&&!seek||space<4096 {return}
    read:u64
    count:=u64(4096)
    tail:=min(u64(256),stream.metadata.frames/2)
    if len(stream.loop_pcm)>0 {
        for frame in 0..<4096 {
            source:=stream.decode_frame
            for channel in 0..<channels {
                value:=stream.loop_pcm[int(source)*channels+channel]
                if tail>0&&source>=stream.metadata.frames-tail {
                    offset:=source-(stream.metadata.frames-tail);t:=f32(offset)/f32(tail)
                    value=value*math.cos(t*math.PI*.5)+stream.loop_pcm[int(offset)*channels+channel]*math.sin(t*math.PI*.5)
                }
                stream.scratch[frame*channels+channel]=value
            }
            stream.decode_frame+=1
            if stream.decode_frame==stream.metadata.frames {stream.decode_frame=tail}
        }
        read=4096
    } else {
        remaining:=stream.metadata.frames-stream.decode_frame
        count=min(count,remaining)
        if stream.looping&&stream.decode_frame<stream.metadata.frames-tail {count=min(count,stream.metadata.frames-tail-stream.decode_frame)}
        status:=native.decoder_read(stream.decoder,raw_data(stream.scratch),count,&read)
        if status!=0||read!=count {sync.mutex_lock(&stream.mutex);stream.error=.Decode_Failed;stream.exhausted=true;sync.mutex_unlock(&stream.mutex);return}
        stream.decode_frame+=read
        if stream.looping&&stream.decode_frame==stream.metadata.frames {
            head_read:u64
            if native.decoder_seek(stream.decoder,0)!=0||native.decoder_read(stream.decoder,raw_data(stream.head),tail,&head_read)!=0||head_read!=tail {sync.mutex_lock(&stream.mutex);stream.error=.Decode_Failed;stream.exhausted=true;sync.mutex_unlock(&stream.mutex);return}
            for sample in stream.head[:int(tail)*channels] {if !finite(sample) {sync.mutex_lock(&stream.mutex);stream.error=.Decode_Failed;stream.exhausted=true;sync.mutex_unlock(&stream.mutex);return}}
            for frame in 0..<int(min(read,tail)) {
                t:=f32(frame)/f32(tail);a,b:=math.cos(t*math.PI*.5),math.sin(t*math.PI*.5)
                for channel in 0..<channels {index:=frame*channels+channel;stream.scratch[index]=stream.scratch[index]*a+stream.head[index]*b}
            }
            stream.decode_frame=tail
        }
    }
    for sample in stream.scratch[:int(read)*channels] {if !finite(sample) {sync.mutex_lock(&stream.mutex);stream.error=.Decode_Failed;stream.exhausted=true;sync.mutex_unlock(&stream.mutex);return}}
    sync.mutex_lock(&stream.mutex);defer sync.mutex_unlock(&stream.mutex)
    // An accepted seek supersedes an in-flight decode block without exposing old samples.
    if stream.seek_pending {return}
    for frame in 0..<read {for channel in 0..<channels {stream.ring[int((stream.write_frame+frame)%capacity)*channels+channel]=stream.scratch[int(frame)*channels+channel]}}
    stream.write_frame+=read
    if !stream.looping&&(read<count||stream.decode_frame==stream.metadata.frames) {stream.exhausted=true}
}
@(private)
stream_samples_locked :: proc(stream:^Stream,step:u64)->([2]f32,bool) {
    if stream.seek_pending||stream.write_frame-stream.read_frame<3&&!stream.exhausted {stream.underrun_frames+=1;return {},false}
    if stream.write_frame==stream.read_frame {return {},false}
    channels:=int(stream.metadata.channels);capacity:=u64(len(stream.ring)/channels);t:=f32(stream.fraction)/f32(FIXED_ONE)
    samples:[2]f32
    for channel in 0..<channels {
        positions:=[4]u64{stream.read_frame,stream.read_frame,min(stream.read_frame+1,stream.write_frame-1),min(stream.read_frame+2,stream.write_frame-1)}
        if stream.read_frame>0 {positions[0]=stream.read_frame-1}
        values:[4]f32;for position,i in positions {values[i]=stream.ring[int(position%capacity)*channels+channel]}
        value:=catmull_rom(values[0],values[1],values[2],values[3],t)
        if channels==1 {samples={value,value}}else if channels==2 {samples[channel]=value}else {samples[0]+=value/f32(channels);samples[1]=samples[0]}
    }
    next:=stream.fraction+step;consume:=next>>24
    if consume>stream.write_frame-stream.read_frame {if stream.exhausted {consume=stream.write_frame-stream.read_frame;next=0}else {stream.underrun_frames+=1;return {},false}}
    stream.read_frame+=consume;stream.fraction=next&(FIXED_ONE-1);return samples,true
}
/// Own encoded bytes and a decoder worker instead of decoding the whole clip into memory.
engine_play_stream :: proc(engine:^Engine,bytes:[]u8,desc:Play_Desc=Play_Desc{category=.Music,priority=.Medium,volume=1,pitch=1})->(Voice_Handle,Error) {
    if engine==nil||!valid_play(desc) {return {},.Invalid_Parameter}
    stream,err:=stream_create(bytes,desc.looping,engine.allocator);if err!=.None {return {},err}
    sync.mutex_lock(&engine.mutex);handle,play_error:=voice_allocate(engine,nil,stream,desc);sync.mutex_unlock(&engine.mutex)
    if play_error!=.None {stream_destroy(stream)};return handle,play_error
}
/// Seek is accepted on the main thread and executed by the decoding worker; old queued samples are discarded.
voice_seek :: proc(engine:^Engine,handle:Voice_Handle,seconds:f64)->Error {
    if seconds<0||seconds!=seconds {return .Invalid_Parameter}
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    voice:=voice_find(engine,handle);if voice==nil {return .Stale_Handle}
    if voice.stream==nil {return .Unsupported}
    stream:=voice.stream;if seconds>=stream.metadata.duration_seconds {return .Invalid_Parameter}
    frame:=u64(seconds*f64(stream.metadata.sample_rate))
    sync.mutex_lock(&stream.mutex);stream.seek_frame=frame;stream.seek_pending=true;sync.mutex_unlock(&stream.mutex)
    voice.position=frame*FIXED_ONE;voice.fade_position=0;voice.fading_out=false;voice.active=true;return .None
}
