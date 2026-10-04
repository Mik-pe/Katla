//! Bounded generational voice controls, interpolation, loop transitions and category mixing.
package katla_audio
import "core:math"
import "core:sync"
import dsp "dsp"

@(private)
valid_play :: proc(desc:Play_Desc)->bool {return desc.category>=.Sfx&&desc.category<=.Ambient && finite(desc.volume)&&desc.volume>=0 && finite(desc.pan)&&desc.pan>=-1&&desc.pan<=1 && finite(desc.pitch)&&desc.pitch>=0.01&&desc.pitch<=16}
@(private)
voice_allocate :: proc(engine:^Engine,clip:^Clip,stream:^Stream,desc:Play_Desc,callback:bool=false)->(Voice_Handle,Error) {
    first,last:=0,MAX_VOICES;if stream!=nil {first=MAX_VOICES;last=MAX_VOICES+MAX_STREAMS}
    slot:=-1;priority:=desc.priority
    for i in first..<last {v:=&engine.voices[i];if !v.active {slot=i;break};if v.priority<priority {slot=i;priority=v.priority}}
    if slot<0 {return {},.Pool_Full}
    voice:=&engine.voices[slot]
    if callback {
        if voice.stream!=nil || voice.clip!=nil&&engine.retired_count==len(engine.retired) {return {},.Queue_Full}
        if voice.clip!=nil {engine.retired[engine.retired_count]=voice.clip;engine.retired_count+=1}
    } else {voice_release(voice)}
    generation:=voice.generation+1;if generation==0 {generation=1}
    rate:=u32(0);if clip!=nil {rate=clip.metadata.sample_rate}else {rate=stream.metadata.sample_rate}
    voice^=Voice{source_rate=rate,generation=generation,clip=clip,stream=stream,active=true,looping=desc.looping,category=desc.category,priority=desc.priority,volume=desc.volume,volume_target=desc.volume,pan=desc.pan,pan_target=desc.pan,pitch=desc.pitch,pitch_target=desc.pitch,smoothing=.3}
    clip_retain(clip)
    active:=0;for v in engine.voices {if v.active {active+=1}};engine.peak_voices=max(active,engine.peak_voices)
    return {u32(slot),generation},.None
}
/// Retain a clip independently of its caller. Higher priorities may steal strictly lower priorities.
engine_play :: proc(engine:^Engine,clip:^Clip,desc:Play_Desc=DEFAULT_PLAY)->(Voice_Handle,Error) {
    if engine==nil || clip==nil || !valid_play(desc) {return {},.Invalid_Parameter}
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    return voice_allocate(engine,clip,nil,desc)
}
@(private)
voice_find :: proc(engine:^Engine,handle:Voice_Handle)->^Voice {
    if handle.generation==0 || handle.slot>=len(engine.voices) {return nil}
    voice:=&engine.voices[handle.slot];if voice.generation!=handle.generation {return nil};return voice
}
/// Stale handles report stopped and never control a replacement voice.
voice_state :: proc(engine:^Engine,handle:Voice_Handle)->Voice_State {
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    voice:=voice_find(engine,handle);if voice!=nil&&voice.active {return .Playing};return .Stopped
}
voice_position :: proc(engine:^Engine,handle:Voice_Handle)->(f64,Error) {
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    voice:=voice_find(engine,handle);if voice==nil {return 0,.Stale_Handle}
    rate:=voice.source_rate
    return f64(voice.position)/f64(FIXED_ONE)/f64(rate),.None
}
/// Begin a three millisecond fade rather than discontinuously cutting a sample.
voice_stop :: proc(engine:^Engine,handle:Voice_Handle)->Error {
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    voice:=voice_find(engine,handle);if voice==nil {return .Stale_Handle};voice.fading_out=true;voice.fade_position=0;return .None
}
engine_stop_all :: proc(engine:^Engine) {
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    for &voice in engine.voices {voice.active=false}
}
/// Set all spatial controls together; tweening smooths volume, pan and pitch once per block.
voice_controls :: proc(engine:^Engine,handle:Voice_Handle,volume,pan,pitch,occlusion:f32,tweened:bool=false,smoothing:f32=.3)->Error {
    if !finite(volume)||volume<0||!finite(pan)||!finite(pitch)||pitch<.01||pitch>16||!finite(occlusion)||!finite(smoothing)||smoothing<=0||smoothing>1 {return .Invalid_Parameter}
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    voice:=voice_find(engine,handle);if voice==nil {return .Stale_Handle}
    voice.volume_target=volume;voice.pan_target=clamp(pan,-1,1);voice.pitch_target=pitch;voice.occlusion=clamp(occlusion,0,1);voice.smoothing=smoothing
    if !tweened {voice.volume=volume;voice.pan=voice.pan_target;voice.pitch=pitch};return .None
}
voice_read_controls :: proc(engine:^Engine,handle:Voice_Handle)->(Play_Desc,Error) {
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    v:=voice_find(engine,handle);if v==nil {return {},.Stale_Handle};return {v.category,v.priority,v.looping,v.volume,v.pan,v.pitch},.None
}
/// Each auxiliary bus uses its default send unless this voice explicitly overrides it.
voice_aux_send :: proc(engine:^Engine,handle:Voice_Handle,bus:u32,level:f32)->Error {
    if !finite(level)||level<0 {return .Invalid_Parameter}
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    v:=voice_find(engine,handle);if v==nil {return .Stale_Handle}
    if bus==0||bus>u32(engine.aux_count) {return .Invalid_Parameter};v.aux_set[bus-1]=true;v.aux_levels[bus-1]=level;return .None
}
compute_pan_gains :: proc(pan:f32)->(f32,f32) {angle:=(clamp(pan,-1,1)+1)*.25*math.PI;return math.cos(angle),math.sin(angle)}
db_to_linear :: proc(db:f32)->f32 {return math.pow(f32(10),db/20)}
linear_to_db :: proc(linear:f32)->f32 {if linear<=0 {return transmute(f32)u32(0xff800000)};return 20*math.log10(linear)}
@(private)
catmull_rom :: proc(a,b,c,d,t:f32)->f32 {return .5*((2*b)+(-a+c)*t+(2*a-5*b+4*c-d)*t*t+(-a+3*b-3*c+d)*t*t*t)}
@(private)
clip_sample :: proc(clip:^Clip,frame:i64,channel:int)->f32 {bounded:=clamp(frame,0,i64(clip.metadata.frames)-1);return clip.samples[int(bounded)*int(clip.metadata.channels)+channel]}
@(private)
clip_interpolate :: proc(clip:^Clip,position:u64,channel:int)->f32 {
    frame:=i64(position>>24);t:=f32(position&(FIXED_ONE-1))/f32(FIXED_ONE)
    return catmull_rom(clip_sample(clip,frame-1,channel),clip_sample(clip,frame,channel),clip_sample(clip,frame+1,channel),clip_sample(clip,frame+2,channel),t)
}
@(private)
voice_mix :: proc(engine:^Engine,voice:^Voice,output:[]f32) {
    if !voice.active {return}
    voice.volume+=(voice.volume_target-voice.volume)*voice.smoothing;voice.pan+=(voice.pan_target-voice.pan)*voice.smoothing;voice.pitch+=(voice.pitch_target-voice.pitch)*voice.smoothing
    left,right:=compute_pan_gains(voice.pan)
    category:=engine.settings.sfx_volume;if voice.category==.Music {category=engine.settings.music_volume}else if voice.category==.Ambient {category=engine.settings.ambient_volume}
    meta:Metadata;if voice.clip!=nil {meta=voice.clip.metadata}else {meta=voice.stream.metadata}
    step:=u64(math.round(f64(FIXED_ONE)*f64(voice.pitch)*f64(meta.sample_rate)/f64(engine.sample_rate)))
    total:=meta.frames*FIXED_ONE;crossfade:=min(u64(256),meta.frames/2)*FIXED_ONE
    fade_length:=max(u32(1),engine.sample_rate*3/1000)
    cutoff:=f32(engine.sample_rate)*.5*(1-voice.occlusion)+200*voice.occlusion
    coefficient:=1/(1+f32(engine.sample_rate)/(2*math.PI*cutoff))
    if voice.stream!=nil {sync.mutex_lock(&voice.stream.mutex)}
    defer {if voice.stream!=nil {sync.mutex_unlock(&voice.stream.mutex)}}
    if voice.stream!=nil&&voice.stream.error!=.None {engine.last_error=voice.stream.error;voice.active=false;return}
    for i in 0..<len(output)/2 {
        if voice.clip!=nil && voice.position>=total {if voice.looping {voice.position=crossfade+(voice.position-total)%(total-crossfade)}else {voice.position=total;voice.active=false;break}}
        samples:[2]f32;available:=true
        if voice.clip!=nil {
            if meta.channels==1 {s:=clip_interpolate(voice.clip,voice.position,0);samples={s,s}}
            else if meta.channels==2 {samples={clip_interpolate(voice.clip,voice.position,0),clip_interpolate(voice.clip,voice.position,1)}}
            else {sum:f32;for ch in 0..<int(meta.channels) {sum+=clip_interpolate(voice.clip,voice.position,ch)};sum/=f32(meta.channels);samples={sum,sum}}
            if voice.looping&&crossfade>0&&voice.position>=total-crossfade {
                t:=f32(voice.position-(total-crossfade))/f32(crossfade);a,b:=math.cos(t*math.PI*.5),math.sin(t*math.PI*.5)
                head:=voice.position-(total-crossfade)
                for &s,ch in samples {h:=clip_interpolate(voice.clip,head,min(ch,int(meta.channels)-1));s=s*a+h*b}
            }
        } else {samples,available=stream_samples_locked(voice.stream,step);if !available {if voice.stream.exhausted&&voice.stream.read_frame>=voice.stream.write_frame {voice.active=false};continue}}
        fade:=min(f32(1),f32(voice.fade_position)/f32(fade_length));if voice.fading_out {fade=1-fade}
        volume:=voice.volume*category*fade
        for &s,ch in samples {if voice.occlusion>0 {voice.filter[ch]+=coefficient*(s-voice.filter[ch]);s=voice.filter[ch]}}
        output[i*2]+=samples[0]*left*volume;output[i*2+1]+=samples[1]*right*volume
        voice.position+=step
        if voice.looping&&voice.stream!=nil&&voice.position>=total {voice.position%=total}
        voice.fade_position+=1
        if voice.fading_out&&voice.fade_position>=fade_length {voice.active=false;break}
    }
    if !voice.looping {
        if voice.clip!=nil&&voice.position>=total||voice.stream!=nil&&voice.stream.exhausted&&voice.stream.read_frame>=voice.stream.write_frame {voice.position=min(voice.position,total);voice.active=false}
    }
}
@(private)
compute_levels :: proc(samples:[]f32)->Channel_Levels {
    peak,sum:f32;for sample in samples {peak=max(peak,abs(sample));sum+=sample*sample};if len(samples)==0 {return {}};return {peak,math.sqrt(sum/f32(len(samples)))}
}
/// Render arbitrary stereo blocks with bounded preallocated scratch. No allocation, freeing or decoding occurs here.
engine_render :: proc(engine:^Engine,output:[]f32)->Error {
    if engine==nil||len(output)%2!=0 {return .Invalid_Parameter}
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    for &sample in output {sample=0}
    result:=Error.None
    for offset:=0;offset<len(output);offset+=BLOCK_FRAMES*2 {
        block:=output[offset:min(offset+BLOCK_FRAMES*2,len(output))]
        scheduled_process(engine)
        for &category in engine.categories {for &sample in category[:len(block)] {sample=0}}
        for &bus in engine.aux_buses[:engine.aux_count] {dsp.aux_bus_prepare(&bus,len(block))}
        scratch:=engine.scratch[:len(block)]
        for &voice in engine.voices {
            if !voice.active {continue}
            for &sample in scratch {sample=0};voice_mix(engine,&voice,scratch)
            cat:=int(voice.category)-1
            for sample,i in scratch {block[i]+=sample;engine.categories[cat][i]+=sample}
            for &bus,index in engine.aux_buses[:engine.aux_count] {send:=bus.send_level;if voice.aux_set[index] {send=voice.aux_levels[index]};dsp.aux_bus_accumulate(&bus,scratch,send)}
        }
        for &bus in engine.aux_buses[:engine.aux_count] {if dsp.aux_bus_process(&bus,2)!=.None {result=.Invalid_Parameter};dsp.aux_bus_mix_into(&bus,block)}
        if dsp.chain_process(&engine.master_effects,block,2)!=.None {result=.Invalid_Parameter}
        for &sample in block {if !finite(sample) {sample=0;result=.Invalid_Parameter};sample=clamp(sample*engine.settings.master_volume,-1,1)}
        engine.levels={compute_levels(block),compute_levels(engine.categories[0][:len(block)]),compute_levels(engine.categories[1][:len(block)]),compute_levels(engine.categories[2][:len(block)])}
        engine.clock_frames+=u64(len(block)/2)
    }
    if result!=.None {engine.last_error=result}
    return result
}
