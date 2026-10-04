package katla_audio
import "core:testing"
import "core:os"
import "core:math"
import "core:time"
import "core:strings"

@(test)
test_real_codecs_exact_metadata_and_owned_pcm :: proc(t:^testing.T) {
    for name in ([]string{"tone.wav","tone.ogg","tone.mp3","tone.flac"}) {
        path:=strings.concatenate({"odin/audio/testdata/",name});defer delete(path)
        bytes,read_error:=os.read_entire_file(path,context.allocator);testing.expect(t,read_error==nil);defer delete(bytes)
        info,error:=metadata(bytes);testing.expect_value(t,error,Error.None);if error!=.None {continue}
        testing.expect_value(t,info.sample_rate,24000);testing.expect(t,info.channels>=1&&info.channels<=2);testing.expect(t,info.frames>=4800&&info.frames<6500)
        clip,decode_error:=clip_decode(bytes);testing.expect_value(t,decode_error,Error.None);if decode_error!=.None {continue}
        testing.expect_value(t,u64(len(clip.samples)),info.sample_count);testing.expect(t,compute_levels(clip.samples).rms>.1)
        clip_retain(clip);clip_release(clip);testing.expect(t,clip.references==1);clip_release(clip)
    }
    bad:=[8]u8{1,2,3,4,5,6,7,8};_,err:=clip_decode(bad[:]);testing.expect_value(t,err,Error.Unsupported)
}
@(test)
test_resampling_pan_fade_priority_and_stale_handles :: proc(t:^testing.T) {
    engine,error:=engine_create(48000,false);testing.expect_value(t,error,Error.None);defer engine_destroy(engine)
    samples:=make([]f32,2400);defer delete(samples);for &s,i in samples {s=math.sin(f32(i)*.07)}
    clip,err:=clip_from_pcm(samples,1,24000);testing.expect_value(t,err,Error.None);defer clip_release(clip)
    desc:=DEFAULT_PLAY;desc.pan=-1;desc.priority=.Low;desc.looping=true
    first,play_error:=engine_play(engine,clip,desc);testing.expect_value(t,play_error,Error.None)
    block:[1024]f32;testing.expect_value(t,engine_render(engine,block[:]),Error.None)
    for i in 0..<512 {testing.expect_value(t,block[i*2+1],0)}
    position,_:=voice_position(engine,first);testing.expect(t,abs(position-f64(512)/48000)<1e-8)
    for _ in 1..<MAX_VOICES {_,e:=engine_play(engine,clip,desc);testing.expect_value(t,e,Error.None)}
    _,full:=engine_play(engine,clip,desc);testing.expect_value(t,full,Error.Pool_Full)
    desc.priority=.High;replacement,replaced:=engine_play(engine,clip,desc);testing.expect_value(t,replaced,Error.None);testing.expect_value(t,replacement.slot,first.slot)
    testing.expect_value(t,voice_controls(engine,first,0,0,1,0),Error.Stale_Handle);testing.expect_value(t,voice_state(engine,first),Voice_State.Stopped)
    testing.expect_value(t,voice_stop(engine,replacement),Error.None);engine_render(engine,block[:]);testing.expect_value(t,voice_state(engine,replacement),Voice_State.Stopped)
    engine_stop_all(engine);engine_collect(engine);testing.expect_value(t,clip.references,1)
}
@(test)
test_category_meter_aux_zone_and_scheduled_cue_ownership :: proc(t:^testing.T) {
    engine,_:=engine_create(48000,false);defer engine_destroy(engine)
    samples:=[2048]f32{};for &s in samples {s=.5}
    clip,_:=clip_from_pcm(samples[:],1,48000);defer clip_release(clip)
    settings:=DEFAULT_SETTINGS;settings.music_volume=.25;testing.expect_value(t,engine_settings(engine,settings),Error.None)
    desc:=DEFAULT_PLAY;desc.category=.Music
    testing.expect_value(t,engine_schedule(engine,{kind=.Play,clip=clip,desc=desc},512.0/48000),Error.None)
    block:[1024]f32;engine_render(engine,block[:]);testing.expect_value(t,compute_levels(block[:]).peak,0)
    engine_render(engine,block[:]);testing.expect(t,engine_read_levels(engine).music.peak>.08&&engine_read_levels(engine).sfx.peak==0)
    id,zone_error:=engine_create_zone_reverb(engine);testing.expect_value(t,zone_error,Error.None);testing.expect(t,id>0);testing.expect_value(t,engine_zone_reverb(engine,.7,.4,.3),Error.None)
    cue:Sound_Cue;testing.expect_value(t,cue_init(&cue,{clip,clip,clip},.Sfx,.Shuffle,42),Error.None);defer cue_destroy(&cue)
    for _ in 0..<3 {_,cue_error:=cue_play(&cue,engine);testing.expect_value(t,cue_error,Error.None)}
    engine_render(engine,block[:]);testing.expect(t,engine_read_levels(engine).sfx.peak>0);engine_collect(engine)
}
@(test)
test_stream_worker_seek_loop_and_release :: proc(t:^testing.T) {
    for name in ([]string{"tone.wav","tone.ogg","tone.mp3","tone.flac"}) {
        path:=strings.concatenate({"odin/audio/testdata/",name});defer delete(path)
        bytes,read_error:=os.read_entire_file(path,context.allocator);testing.expect(t,read_error==nil);defer delete(bytes)
        engine,_:=engine_create(48000,false);defer engine_destroy(engine)
        desc:=DEFAULT_PLAY;desc.category=.Music;desc.looping=true
        handle,error:=engine_play_stream(engine,bytes,desc);testing.expect_value(t,error,Error.None);if error!=.None {continue}
        time.sleep(20*time.Millisecond);block:[1024]f32;engine_render(engine,block[:]);testing.expect(t,compute_levels(block[:]).rms>.01)
        testing.expect_value(t,voice_seek(engine,handle,.1),Error.None);time.sleep(10*time.Millisecond);engine_render(engine,block[:]);position,_:=voice_position(engine,handle);testing.expect(t,position>.1)
        for _ in 0..<25 {engine_render(engine,block[:]);time.sleep(time.Millisecond)}
        testing.expect_value(t,voice_state(engine,handle),Voice_State.Playing);voice_stop(engine,handle);engine_render(engine,block[:]);testing.expect_value(t,voice_state(engine,handle),Voice_State.Stopped);engine_collect(engine)
    }
}

@(test)
test_incremental_decoder_exact_seek_raw_formats_and_multichannel_downmix :: proc(t:^testing.T) {
    bytes,read_error:=os.read_entire_file("odin/audio/testdata/tone.wav",context.allocator);testing.expect(t,read_error==nil);defer delete(bytes)
    decoder,error:=decoder_open(bytes);testing.expect(t,error==.None);defer decoder_destroy(decoder)
    clip,chunk_error:=decoder_read_chunk(decoder,1024);testing.expect(t,chunk_error==.None&&clip.metadata.frames==1024);defer clip_release(clip)
    testing.expect(t,decoder_seek(decoder,.1)==.None&&decoder.frame==2400)
    next,next_error:=decoder_read_chunk(decoder,1024);testing.expect(t,next_error==.None&&next.metadata.frames==1024&&decoder.frame==3424);defer clip_release(next)
    engine,_:=engine_create(48000,false);defer engine_destroy(engine)
    pcm:=[4096]i16{};for &s in pcm {s=16384};raw,_:=clip_from_i16(pcm[:],4,48000);defer clip_release(raw)
    testing.expect(t,raw.metadata.format==.PCM&&abs(raw.samples[0]-.50001526)<1e-7)
    handle,_:=engine_play(engine,raw);block:[1024]f32;engine_render(engine,block[:]);for i in 0..<512 {testing.expect(t,block[i*2]==block[i*2+1])}
    engine_stop_all(engine);engine_collect(engine);position,_:=voice_position(engine,handle);testing.expect(t,abs(position-512.0/48000)<1e-8)
}

@(test)
test_cue_sequential_shuffle_variation_and_tween_targets :: proc(t:^testing.T) {
    engine,_:=engine_create(48000,false);defer engine_destroy(engine)
    clips:[3]^Clip
    for &clip,i in clips {samples:=[2048]f32{};for &s in samples {s=f32(i+1)*.1};clip,_=clip_from_pcm(samples[:],1,48000)}
    defer {for clip in clips {clip_release(clip)}}
    cue:Sound_Cue;testing.expect(t,cue_init(&cue,clips[:],.Ambient,.Sequential,123)==.None);defer cue_destroy(&cue)
    for i in 0..<4 {handle,error:=cue_play(&cue,engine);testing.expect(t,error==.None&&engine.voices[handle.slot].clip==clips[i%3])}
    cue.mode=.Shuffle;cue.index=0;cue.pitch_semitones=12;cue.volume_db=3
    for _ in 0..<2 {
        seen:[3]bool
        for _ in 0..<3 {handle,error:=cue_play(&cue,engine);testing.expect(t,error==.None);v:=&engine.voices[handle.slot];testing.expect(t,v.pitch>=.5&&v.pitch<=2&&v.volume>=db_to_linear(-3)&&v.volume<=db_to_linear(3));for clip,j in clips {if v.clip==clip {testing.expect(t,!seen[j]);seen[j]=true}}}
        testing.expect(t,seen[0]&&seen[1]&&seen[2])
    }
    handle,_:=engine_play(engine,clips[0]);testing.expect(t,voice_controls(engine,handle,0,1,2,.5,true,.3)==.None)
    block:[1024]f32;engine_render(engine,block[:]);v:=&engine.voices[handle.slot];testing.expect(t,abs(v.volume-.7)<1e-6&&abs(v.pan-.3)<1e-6&&abs(v.pitch-1.3)<1e-6)
}

@(test)
test_stream_noninteger_tail_completes_at_high_pitch_without_stall :: proc(t:^testing.T) {
    bytes,read_error:=os.read_entire_file("odin/audio/testdata/tone.wav",context.allocator);testing.expect(t,read_error==nil);defer delete(bytes)
    engine,_:=engine_create(48000,false);defer engine_destroy(engine)
    desc:=DEFAULT_PLAY;desc.pitch=3.7
    handle,error:=engine_play_stream(engine,bytes,desc);testing.expect(t,error==.None)
    time.sleep(30*time.Millisecond)
    block:[1024]f32;for _ in 0..<8 {engine_render(engine,block[:])}
    testing.expect(t,voice_state(engine,handle)==.Stopped)
    position,_:=voice_position(engine,handle);testing.expect(t,abs(position-.2)<1e-7)
    engine_collect(engine)
}

@(test)
test_single_frame_stream_loop_and_nonfinite_pcm_rejection :: proc(t:^testing.T) {
    wav:=[48]u8{'R','I','F','F',40,0,0,0,'W','A','V','E','f','m','t',' ',16,0,0,0,3,0,1,0,0xc0,0x5d,0,0,0,0x77,1,0,4,0,32,0,'d','a','t','a',4,0,0,0,0,0,0,0x3f}
    engine,_:=engine_create(48000,false);defer engine_destroy(engine)
    desc:=DEFAULT_PLAY;desc.looping=true
    handle,error:=engine_play_stream(engine,wav[:],desc);testing.expect_value(t,error,Error.None)
    if error!=.None {return}
    block:[1024]f32
    for _ in 0..<8 {testing.expect_value(t,engine_render(engine,block[:]),Error.None);testing.expect(t,compute_levels(block[:]).rms>.2)}
    snapshots:[MAX_VOICES+MAX_STREAMS]Voice_Info;count:=engine_voices(engine,snapshots[:]);testing.expect_value(t,count,1);testing.expect_value(t,snapshots[0].stream_underruns,0)
    testing.expect_value(t,voice_state(engine,handle),Voice_State.Playing)
    wav[46]=0xc0;wav[47]=0x7f
    invalid,decode_error:=clip_decode(wav[:]);testing.expect_value(t,decode_error,Error.Decode_Failed);testing.expect(t,invalid==nil)
    _,stream_error:=engine_play_stream(engine,wav[:],desc);testing.expect_value(t,stream_error,Error.Decode_Failed)
}
