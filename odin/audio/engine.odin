//! Stable engine ownership and native stereo callbacks. Device operations belong to the main thread.
package katla_audio
import native "../deps/audio"
import dsp "dsp"
import "base:runtime"
import "core:mem"
import "core:sync"

Category :: enum {Master,Sfx,Music,Ambient}
Settings :: struct {master_volume,sfx_volume,music_volume,ambient_volume:f32}
DEFAULT_SETTINGS :: Settings{1,1,1,1}
Priority :: enum {Low,Medium,High}
Voice_State :: enum {Stopped,Playing}
Voice_Handle :: struct {slot:u32,generation:u64}
Play_Desc :: struct {category:Category,priority:Priority,looping:bool,volume,pan,pitch:f32}
DEFAULT_PLAY :: Play_Desc{category=.Sfx,priority=.Medium,volume=1,pitch=1}
Channel_Levels :: struct {peak,rms:f32}
Levels :: struct {master,sfx,music,ambient:Channel_Levels}
Device_Info :: native.Device_Info
MAX_VOICES :: 64
MAX_STREAMS :: 8
MAX_AUX_BUSES :: 16
BLOCK_FRAMES :: 512
FIXED_ONE :: u64(1)<<24
Voice :: struct {
    generation:u64,clip:^Clip,stream:^Stream,active,looping:bool,category:Category,priority:Priority,
    position:u64,source_rate:u32,volume,pan,pitch,volume_target,pan_target,pitch_target,smoothing,occlusion:f32,
    filter:[2]f32,fade_position:u32,fading_out:bool,aux_levels:[MAX_AUX_BUSES]f32,aux_set:[MAX_AUX_BUSES]bool,
}
/// Stationary owner; destroy joins native callbacks before releasing any audio data.
Engine :: struct {
    allocator:mem.Allocator,mutex:sync.Mutex,device:native.Device,sample_rate:u32,playing,closed:bool,last_error:Error,scheduled_failures:u64,recovery_attempts:u32,recovery_pending:bool,
    voices:[MAX_VOICES+MAX_STREAMS]Voice,settings:Settings,peak_voices:int,clock_frames:u64,levels:Levels,
    scratch:[BLOCK_FRAMES*2]f32,categories:[3][BLOCK_FRAMES*2]f32,
    master_effects:dsp.Effect_Chain,aux_buses:[MAX_AUX_BUSES]dsp.Aux_Bus,aux_count:int,
    zone:dsp.Zone_Reverb,zone_targets:dsp.Zone_Targets,zone_bus:int,
    events:[256]Scheduled_Event,event_count:int,retired:[256]^Clip,retired_count:int,
}
/// Create paused. Offline engines use the identical render procedure without opening a device.
engine_create :: proc(sample_rate:u32=48000,native_output:bool=true,allocator:=context.allocator)->(^Engine,Error) {
    if native.abi_version()!=1 {return nil,.Unsupported}
    if sample_rate<8000 || sample_rate>384000 {return nil,.Invalid_Parameter}
    engine:=new(Engine,allocator);engine.allocator=allocator;engine.sample_rate=sample_rate;engine.settings=DEFAULT_SETTINGS;engine.zone_bus=-1
    if native_output && native.device_open(device_callback,engine,sample_rate,-1,&engine.device)!=0 {free(engine,allocator);return nil,.Device_Not_Found}
    return engine,.None
}
@(private)
device_callback :: proc "c" (user:rawptr,output:[^]f32,frames:u32) {
    context=runtime.default_context()
    engine:=cast(^Engine)user
    engine_render(engine,output[:int(frames)*2])
}
/// Resume actual native output; offline engines simply enter the same playing state.
engine_resume :: proc(engine:^Engine)->Error {
    if engine==nil || engine.closed {return .Not_Initialized}
    if engine.device!=nil && native.device_start(engine.device)!=0 {return .Device_Failed}
    engine.playing=true;return .None
}
/// Pause waits for the device callback to stop before returning.
engine_pause :: proc(engine:^Engine)->Error {
    if engine==nil || engine.closed {return .Not_Initialized}
    if engine.device!=nil && native.device_stop(engine.device)!=0 {return .Device_Failed}
    engine.playing=false;return .None
}
/// Transactionally reopen an enumerated device (-1 selects default) while preserving voices and clock.
engine_reopen_device :: proc(engine:^Engine,index:int=-1)->Error {
    if engine==nil || engine.closed || index < -1 || index>2147483647 {return .Invalid_Parameter}
    candidate:native.Device
    if native.device_open(device_callback,engine,engine.sample_rate,i32(index),&candidate)!=0 {return .Device_Not_Found}
    old:=engine.device;playing:=engine.playing
    if old!=nil && native.device_stop(old)!=0 {native.device_close(candidate);return .Device_Failed}
    if playing && native.device_start(candidate)!=0 {
        native.device_close(candidate)
        if old!=nil {native.device_start(old)}
        return .Device_Failed
    }
    engine.device=candidate;engine.recovery_pending=false;engine.recovery_attempts=0;native.device_close(old);return .None
}
/// Native reroute/interruption notifications trigger an actual replacement stream on the default output.
engine_poll_device_change :: proc(engine:^Engine)->(bool,Error) {
    if engine==nil || engine.closed {return false,.Not_Initialized}
    if engine.device==nil {return false,.None}
    if native.device_changed(engine.device)!=0 {engine.recovery_pending=true;engine.recovery_attempts=0}
    if !engine.recovery_pending {return false,.None}
    if engine.recovery_attempts>=3 {return false,.Device_Failed}
    engine.recovery_attempts+=1
    err:=engine_reopen_device(engine);return err==.None,err
}
/// Enumerate real playback devices; return total count even if the caller's slice is smaller.
devices :: proc(output:[]Device_Info)->(u32,Error) {
    count:u32;if native.devices(raw_data(output),u32(len(output)),&count)!=0 {return 0,.Device_Not_Found};return count,.None
}
/// Snapshot real backend callback, frame and nonzero-output counters.
engine_device_info :: proc(engine:^Engine)->Device_Info {info:Device_Info;if engine!=nil {native.device_snapshot(engine.device,&info)};return info}
/// Stop device callbacks first, then release retained clips, streaming workers and delay buffers.
engine_destroy :: proc(engine:^Engine) {
    if engine==nil {return}
    native.device_close(engine.device);engine.device=nil;engine.closed=true
    for &voice in engine.voices {voice_release(&voice)}
    for event in engine.events[:engine.event_count] {clip_release(event.clip)}
    for clip in engine.retired[:engine.retired_count] {clip_release(clip)}
    for &bus in engine.aux_buses[:engine.aux_count] {dsp.aux_bus_destroy(&bus)}
    if engine.zone_bus>=0 {dsp.zone_reverb_destroy(&engine.zone)}
    allocator:=engine.allocator;free(engine,allocator)
}
/// Reclaim completed voice owners on the main thread, never on a native callback.
engine_collect :: proc(engine:^Engine) {
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    for &voice in engine.voices {if !voice.active {voice_release(&voice)}}
    for clip in engine.retired[:engine.retired_count] {clip_release(clip)};engine.retired_count=0
}
@(private)
voice_release :: proc(voice:^Voice) {
    clip_release(voice.clip);voice.clip=nil
    if voice.stream!=nil {stream_destroy(voice.stream);voice.stream=nil}
}
/// Apply validated category controls atomically with respect to rendering.
engine_settings :: proc(engine:^Engine,settings:Settings)->Error {
    if !finite(settings.master_volume)||!finite(settings.sfx_volume)||!finite(settings.music_volume)||!finite(settings.ambient_volume) {return .Invalid_Parameter}
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    engine.settings={clamp(settings.master_volume,0,1),clamp(settings.sfx_volume,0,1),clamp(settings.music_volume,0,1),clamp(settings.ambient_volume,0,1)};return .None
}
engine_last_error :: proc(engine:^Engine)->Error {sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex);return engine.last_error}
engine_clear_error :: proc(engine:^Engine) {sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex);engine.last_error=.None}
engine_read_settings :: proc(engine:^Engine)->Settings {sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex);return engine.settings}
engine_read_levels :: proc(engine:^Engine)->Levels {sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex);return engine.levels}
engine_voice_counts :: proc(engine:^Engine,reset_peak:bool=false)->(int,int) {
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex);active:=0
    for voice in engine.voices {if voice.active {active+=1}}
    peak:=engine.peak_voices;if reset_peak {engine.peak_voices=active};return active,peak
}
engine_clock :: proc(engine:^Engine,after_frames:u64=0)->f64 {sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex);return f64(engine.clock_frames+after_frames)/f64(engine.sample_rate)}
/// Borrow a stable externally owned effect until engine destruction; setup is outside callbacks.
engine_add_master_effect :: proc(engine:^Engine,effect:dsp.Effect)->Error {
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    if dsp.chain_add(&engine.master_effects,effect)!=.None {return .Capacity_Exceeded};return .None
}
/// Allocate bounded auxiliary scratch on the main thread, transferring no external effect ownership.
engine_add_aux_bus :: proc(engine:^Engine,send,return_level:f32)->(u32,Error) {
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    if engine.aux_count==MAX_AUX_BUSES {return 0,.Capacity_Exceeded}
    index:=engine.aux_count
    if dsp.aux_bus_init(&engine.aux_buses[index],BLOCK_FRAMES*2,send,return_level,engine.allocator)!=.None {return 0,.Invalid_Parameter}
    engine.aux_buses[index].id=u64(index+1);engine.aux_count+=1;return u32(index+1),.None
}
engine_aux_effect :: proc(engine:^Engine,id:u32,effect:dsp.Effect)->Error {
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    if id==0 || id>u32(engine.aux_count) {return .Invalid_Parameter}
    if dsp.chain_add(&engine.aux_buses[id-1].chain,effect)!=.None {return .Capacity_Exceeded};return .None
}
engine_create_zone_reverb :: proc(engine:^Engine)->(u32,Error) {
    if engine.zone_bus>=0 {return u32(engine.zone_bus+1),.Already_Initialized}
    context.allocator=engine.allocator
    if dsp.zone_reverb_init(&engine.zone,engine.sample_rate,&engine.zone_targets)!=.None {return 0,.Invalid_Parameter}
    id,err:=engine_add_aux_bus(engine,1,1)
    if err!=.None {dsp.zone_reverb_destroy(&engine.zone);return 0,err}
    engine.zone_bus=int(id)-1
    err=engine_aux_effect(engine,id,dsp.effect_zone_reverb(&engine.zone));return id,err
}
engine_zone_reverb :: proc(engine:^Engine,decay,wet,dampening:f32)->Error {
    if engine.zone_bus<0 {return .Not_Initialized}
    if dsp.zone_targets_set(&engine.zone_targets,decay,wet,dampening)!=.None {return .Invalid_Parameter};return .None
}

Voice_Info :: struct {handle:Voice_Handle,category:Category,priority:Priority,streaming,looping:bool,volume,pan,pitch:f32,position_seconds:f64,stream_underruns:u64}
/// Snapshot active voices into caller storage without allocation; total count includes entries beyond capacity.
engine_voices :: proc(engine:^Engine,output:[]Voice_Info)->int {
    sync.mutex_lock(&engine.mutex);defer sync.mutex_unlock(&engine.mutex)
    count:=0
    for voice,slot in engine.voices {if !voice.active {continue};if count<len(output) {output[count]={handle={u32(slot),voice.generation},category=voice.category,priority=voice.priority,streaming=voice.stream!=nil,looping=voice.looping,volume=voice.volume,pan=voice.pan,pitch=voice.pitch,position_seconds=f64(voice.position)/f64(FIXED_ONE)/f64(voice.source_rate)};if voice.stream!=nil {sync.mutex_lock(&voice.stream.mutex);output[count].stream_underruns=voice.stream.underrun_frames;sync.mutex_unlock(&voice.stream.mutex)}};count+=1};return count
}
