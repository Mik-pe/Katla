//! Bounded scheduled commands and deterministic sound-cue selection.
package katla_audio
import "core:math"
import "core:mem"
import "core:slice"
import "core:sync"
Event_Kind :: enum {Play,Stop,Volume}
Scheduled_Event :: struct {kind:Event_Kind,frame:u64,clip:^Clip,desc:Play_Desc,voice:Voice_Handle,volume:f32}
/// Schedule against the engine sample clock; commands execute at the first render block at or after the requested time.
engine_schedule :: proc(engine:^Engine,event:Scheduled_Event,time_seconds:f64)->Error {
    if time_seconds<0||time_seconds!=time_seconds||time_seconds>1e12 {return .Invalid_Parameter}
    if event.kind==.Play&&(event.clip==nil||!valid_play(event.desc))||event.kind==.Volume&&(!finite(event.volume)||event.volume<0) {return .Invalid_Parameter}
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    if engine.event_count==len(engine.events) {return .Queue_Full}
    e:=event;e.frame=u64(time_seconds*f64(engine.sample_rate));clip_retain(e.clip)
    index:=engine.event_count
    for index>0&&engine.events[index-1].frame>e.frame {engine.events[index]=engine.events[index-1];index-=1}
    engine.events[index]=e;engine.event_count+=1;return .None
}
@(private)
scheduled_process :: proc(engine:^Engine) {
    for engine.event_count>0&&engine.events[0].frame<=engine.clock_frames {
        event:=engine.events[0]
        if event.clip!=nil&&engine.retired_count+2>len(engine.retired) {break}
        switch event.kind {
        case .Play: _,error:=voice_allocate(engine,event.clip,nil,event.desc,true);if error!=.None {engine.scheduled_failures+=1;engine.last_error=error}
        case .Stop: v:=voice_find(engine,event.voice);if v!=nil {v.fading_out=true;v.fade_position=0}else {engine.last_error=.Stale_Handle;engine.scheduled_failures+=1}
        case .Volume: v:=voice_find(engine,event.voice);if v!=nil {v.volume=event.volume;v.volume_target=event.volume}else {engine.last_error=.Stale_Handle;engine.scheduled_failures+=1}
        }
        if event.clip!=nil {engine.retired[engine.retired_count]=event.clip;engine.retired_count+=1}
        for i in 1..<engine.event_count {engine.events[i-1]=engine.events[i]};engine.event_count-=1
    }
}
Cue_Mode :: enum {Random,Sequential,Shuffle}
/// Owns retained clips and a shuffle permutation; cue operations belong to the main thread.
Sound_Cue :: struct {clips:[]^Clip,order:[]int,mode:Cue_Mode,category:Category,pitch_semitones,volume_db:f32,index:int,random:u64,allocator:mem.Allocator}
cue_init :: proc(cue:^Sound_Cue,clips:[]^Clip,category:Category=.Sfx,mode:Cue_Mode=.Random,seed:u64=1,allocator:=context.allocator)->Error {
    if len(cue.clips)!=0 {return .Already_Initialized}
    if len(clips)==0||len(clips)>4096||category<.Sfx||category>.Ambient {return .Invalid_Parameter}
    for clip in clips {if clip==nil {return .Invalid_Parameter}}
    cue^={clips=slice.clone(clips,allocator),order=make([]int,len(clips),allocator),mode=mode,category=category,random=max(seed,1),allocator=allocator}
    for clip,i in cue.clips {clip_retain(clip);cue.order[i]=i};return .None
}
cue_destroy :: proc(cue:^Sound_Cue) {for clip in cue.clips {clip_release(clip)};delete(cue.clips,cue.allocator);delete(cue.order,cue.allocator);cue^={}}
@(private)
cue_random :: proc(cue:^Sound_Cue)->u64 {x:=cue.random;x~=x<<13;x~=x>>7;x~=x<<17;cue.random=x;return x}
/// Random/sequential/shuffled selection and independent pitch-semitone/volume-decibel variations.
cue_play :: proc(cue:^Sound_Cue,engine:^Engine)->(Voice_Handle,Error) {
    if len(cue.clips)==0||!finite(cue.pitch_semitones)||!finite(cue.volume_db)||cue.pitch_semitones<0||cue.pitch_semitones>48||cue.volume_db<0||cue.volume_db>60 {return {},.Invalid_Parameter}
    index:=0
    switch cue.mode {
    case .Random: index=int(cue_random(cue)%u64(len(cue.clips)))
    case .Sequential: index=cue.index;cue.index=(cue.index+1)%len(cue.clips)
    case .Shuffle:
        if cue.index==0 {for i:=len(cue.order)-1;i>0;i-=1 {j:=int(cue_random(cue)%u64(i+1));cue.order[i],cue.order[j]=cue.order[j],cue.order[i]}}
        index=cue.order[cue.index];cue.index=(cue.index+1)%len(cue.clips)
    }
    desc:=DEFAULT_PLAY;desc.category=cue.category
    a:=f32(cue_random(cue)&0xffffff)/f32(0xffffff)*2-1;b:=f32(cue_random(cue)&0xffffff)/f32(0xffffff)*2-1
    desc.pitch=math.pow(f32(2),a*cue.pitch_semitones/12);desc.volume=db_to_linear(b*cue.volume_db)
    return engine_play(engine,cue.clips[index],desc)
}
