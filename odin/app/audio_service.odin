//! Main-thread audio ownership connects confined clips, scene emitters and editor preview to native output.
package app
import audio "../audio"
import ecs "../ecs"
import box3d "../physics/box3d"
import km "../math"
import "core:mem"
import "core:strings"
import "core:math"

Audio_Active_Voice :: struct {handle:audio.Voice_Handle,key:string,position:km.Vec3,has_position:bool,looping:bool}
Audio_Service :: struct {
    owner:^Authoring,engine:^audio.Engine,allocator:mem.Allocator,
    clips:map[string]^audio.Clip,voices:map[ecs.Entity_Id]Audio_Active_Voice,cues:map[string]^audio.Sound_Cue,
    preview:audio.Voice_Handle,preview_key:string,unconfigured_emitters:int,
    listener_position,listener_forward,listener_up:km.Vec3,script_voices:[dynamic]audio.Voice_Handle,has_listener:bool,last_error:audio.Error,cache_samples:u64,
}
Audio_Service_State :: struct {available,playing,preview_playing:bool,error:audio.Error,device:audio.Device_Info,levels:audio.Levels,active_voices,peak_voices,unconfigured_emitters:int}
/// Open a stationary paused engine only after all main-thread bookkeeping has initialized.
audio_service_init :: proc(service:^Audio_Service,owner:^Authoring,native_output:bool=true)->audio.Error {
    if service.owner!=nil {return .Already_Initialized}
    allocator:=owner.world.allocator;service^={owner=owner,allocator=allocator,clips=make(map[string]^audio.Clip,allocator),voices=make(map[ecs.Entity_Id]Audio_Active_Voice,allocator),cues=make(map[string]^audio.Sound_Cue,allocator),script_voices=make([dynamic]audio.Voice_Handle,allocator)}
    engine,error:=audio.engine_create(native_output=native_output,allocator=allocator)
    service.last_error=error;if error!=.None {return error};service.engine=engine
    _,zone_error:=audio.engine_create_zone_reverb(engine);service.last_error=zone_error
    return zone_error
}
/// Device callbacks and decoder workers are joined before clips, cue owners or source strings are released.
audio_service_destroy :: proc(service:^Audio_Service) {
    audio.engine_destroy(service.engine)
    for key,clip in service.clips {audio.clip_release(clip);delete(key,service.allocator)}
    for _,voice in service.voices {delete(voice.key,service.allocator)}
    for key,cue in service.cues {audio.cue_destroy(cue);free(cue,service.allocator);delete(key,service.allocator)}
    delete(service.preview_key,service.allocator);delete(service.script_voices);delete(service.clips);delete(service.voices);delete(service.cues);service^={}
}
@(private="package")
audio_source_key :: proc(source:Audio_Source,allocator:mem.Allocator)->string {prefix:="resource:";if source.root==.Project {prefix="project:"};if source.root==.File {prefix="file:"};return strings.concatenate({prefix,source.path},allocator)}
@(private="package")
audio_service_clip :: proc(service:^Audio_Service,source:Audio_Source)->(^audio.Clip,audio.Error) {
    key:=audio_source_key(source,service.allocator)
    if clip,present:=service.clips[key];present {delete(key,service.allocator);return clip,.None}
    if len(service.clips)>=256 {delete(key,service.allocator);return nil,.Capacity_Exceeded}
    bytes,error:=audio_source_bytes(service.owner,source);if error!=.None {delete(key,service.allocator);return nil,error};defer delete(bytes,service.allocator)
    clip,decode:=audio.clip_decode(bytes,service.allocator);if decode!=.None {delete(key,service.allocator);return nil,decode}
    if clip.metadata.sample_count>audio.MAX_PCM_SAMPLES-service.cache_samples {audio.clip_release(clip);delete(key,service.allocator);return nil,.Capacity_Exceeded}
    service.clips[key]=clip;service.cache_samples+=clip.metadata.sample_count;return clip,.None
}
/// Load and start the candidate before stopping the old preview; repeated selection toggles the same clip.
audio_service_preview :: proc(service:^Audio_Service,source:Audio_Source)->audio.Error {
    if service.engine==nil {return service.last_error}
    key:=audio_source_key(source,service.allocator)
    if key==service.preview_key&&audio.voice_state(service.engine,service.preview)==.Playing {delete(key,service.allocator);audio_service_stop_preview(service);return .None}
    clip,error:=audio_service_clip(service,source)
    if error!=.None {delete(key,service.allocator);service.last_error=error;return error}
    desc:=audio.DEFAULT_PLAY;desc.priority=.High
    voice,play_error:=audio.engine_play(service.engine,clip,desc)
    if play_error!=.None {delete(key,service.allocator);service.last_error=play_error;return play_error}
    if error=audio.engine_resume(service.engine);error!=.None {audio.voice_stop(service.engine,voice);delete(key,service.allocator);service.last_error=error;return error}
    if service.preview.generation!=0 {audio.voice_stop(service.engine,service.preview)}
    delete(service.preview_key,service.allocator);service.preview_key=key;service.preview=voice;service.last_error=.None;return .None
}
audio_service_stop_preview :: proc(service:^Audio_Service) {if service.engine!=nil&&service.preview.generation!=0 {audio.voice_stop(service.engine,service.preview)};service.preview={};delete(service.preview_key,service.allocator);service.preview_key=""}
/// Expose actual device errors and output/meter evidence; unavailable output remains explicitly unavailable.
audio_service_snapshot :: proc(service:^Audio_Service)->Audio_Service_State {
    state:=Audio_Service_State{error=service.last_error,unconfigured_emitters=service.unconfigured_emitters}
    if service.engine!=nil {callback_error:=audio.engine_last_error(service.engine);if callback_error!=.None {state.error=callback_error}
        state.playing=service.engine.playing;state.preview_playing=audio.voice_state(service.engine,service.preview)==.Playing;state.device=audio.engine_device_info(service.engine);state.available=state.device.sample_rate!=0;state.levels=audio.engine_read_levels(service.engine);state.active_voices,state.peak_voices=audio.engine_voice_counts(service.engine)};return state
}
/// Mirror persisted preferences without making the audio package depend on application settings.
audio_service_settings :: proc(service:^Audio_Service,master,sfx,music,ambient:f32)->audio.Error {
    if service.engine==nil {return service.last_error};error:=audio.engine_settings(service.engine,{master,sfx,music,ambient});service.last_error=error;return error
}
/// Stop deleted/disabled emitters and release finished owners while retaining immutable cached clip data.
audio_service_reset :: proc(service:^Audio_Service) {
    if service.engine==nil {return}
    for id,voice in service.voices {audio.voice_stop(service.engine,voice.handle);delete(voice.key,service.allocator);delete_key(&service.voices,id)}
    for voice in service.script_voices {audio.voice_stop(service.engine,voice)};clear(&service.script_voices)
    service.has_listener=false;audio.engine_zone_reverb(service.engine,0,0,.2);audio.engine_collect(service.engine)
}
@(private="package")
audio_distance_pan :: proc(emitter,listener,forward,up:km.Vec3,descriptor:Audio_Emitter)->(f32,f32) {
    delta:=emitter-listener;distance:=km.length(delta);gain:f32
    switch descriptor.distance_model {
    case .Inverse_Clamped:d:=clamp(distance,descriptor.min_distance,descriptor.max_distance);gain=descriptor.min_distance/(descriptor.min_distance+descriptor.rolloff_factor*(d-descriptor.min_distance))
    case .Linear:gain=1-descriptor.rolloff_factor*(distance-descriptor.min_distance)/(descriptor.max_distance-descriptor.min_distance);if distance<=descriptor.min_distance {gain=1}else if distance>=descriptor.max_distance {gain=0}
    case .Exponential:gain=math.pow(max(distance,descriptor.min_distance)/descriptor.min_distance,-descriptor.rolloff_factor)
    }
    pan:f32;if distance>.001 {right:=km.cross(forward,up);if km.length(right)>.001 {pan=clamp(km.dot(km.normalize(right),delta/distance),-1,1)}}
    return clamp(gain,0,1),pan
}
@(private="package")
audio_doppler :: proc(emitter,listener,emitter_velocity,listener_velocity:km.Vec3)->f32 {
    delta:=listener-emitter;distance:=km.length(delta);if distance<.001 {return 1};direction:=delta/distance
    denominator:=343-km.dot(listener_velocity,direction)+km.dot(emitter_velocity,direction);if abs(denominator)<.001 {return 1};return clamp(343/denominator,.5,2)
}
/// Advance scene audio on the application thread. Output is independent of renderer and frame ownership.
audio_service_update :: proc(service:^Audio_Service,delta_seconds:f32,scene_playing:bool=true)->audio.Error {
    if service.engine==nil {return service.last_error}
    if !finite_nonnegative(delta_seconds)||delta_seconds>.25 {return .Invalid_Parameter}
    owner:=service.owner;context.allocator=service.allocator
    _,device_error:=audio.engine_poll_device_change(service.engine);if device_error!=.None {service.last_error=device_error;return device_error}
    if !scene_playing {audio_service_reset(service);audio.engine_collect(service.engine);return .None}
    if error:=audio.engine_resume(service.engine);error!=.None {service.last_error=error;return error}
    ids:=ecs.entity_ids(&owner.world);defer delete(ids)
    listener,forward,up:=km.VEC3_ZERO,-km.VEC3_Z,km.VEC3_Y
    for id in ids {if _,present:=ecs.get_component(&owner.world,id,Audio_Listener);present {world_matrix,error:=scene_world_matrix(owner,id);if error!=.None {continue};listener=km.mat4_extract_translation(world_matrix);forward=-km.Vec3{world_matrix[2][0],world_matrix[2][1],world_matrix[2][2]};up={world_matrix[1][0],world_matrix[1][1],world_matrix[1][2]};break}}
    velocity:=km.VEC3_ZERO;if service.has_listener {velocity=(listener-service.listener_position)/max(delta_seconds,.001)};service.listener_position=listener;service.listener_forward=forward;service.listener_up=up;service.has_listener=true
    decay,wet,dampening:f32;zones:=0
    for id in ids {zone,present:=ecs.get_component(&owner.world,id,Reverb_Zone);if !present {continue};if !audio_zone_valid(zone) {return .Invalid_Parameter};world_matrix,error:=scene_world_matrix(owner,id);if error!=.None {continue};center:=km.mat4_extract_translation(world_matrix);delta:=listener-center;inside:=true;for value,i in delta {if abs(value)>zone.half_extents[i] {inside=false}};if inside {decay+=zone.decay;wet+=zone.wet;dampening+=zone.dampening;zones+=1}}
    if zones>0 {decay/=f32(zones);wet/=f32(zones);dampening/=f32(zones)}else {dampening=.2};audio.engine_zone_reverb(service.engine,decay,wet,dampening)
    for id,voice in service.voices {emitter,present:=ecs.get_component(&owner.world,id,Audio_Emitter);if !present||!emitter.playing||emitter.source_path=="" {audio.voice_stop(service.engine,voice.handle);delete(voice.key,service.allocator);delete_key(&service.voices,id)}}
    service.unconfigured_emitters=0
    for id in ids {
        emitter,present:=ecs.get_component(&owner.world,id,Audio_Emitter);if !present {continue};if emitter.source_path=="" {service.unconfigured_emitters+=1;continue};if !emitter.playing {continue}
        if !audio_emitter_valid(emitter) {return .Invalid_Parameter}
        source:=Audio_Source{emitter.source_path,emitter.root};key:=audio_source_key(source,service.allocator)
        active,has_voice:=service.voices[id]
        if has_voice&&active.key==key&&active.looping==emitter.looping&&audio.voice_state(service.engine,active.handle)==.Stopped {
            delete(key,service.allocator);delete(active.key,service.allocator);delete_key(&service.voices,id)
            if mutable:=ecs.get_component_mut(&owner.world,id,Audio_Emitter);mutable!=nil {mutable.playing=false};continue
        }
        if !has_voice||active.key!=key||active.looping!=emitter.looping {
            clip,error:=audio_service_clip(service,source);if error!=.None {delete(key,service.allocator);service.last_error=error;return error}
            desc:=audio.DEFAULT_PLAY;desc.looping=emitter.looping;desc.volume=emitter.volume
            handle,play_error:=audio.engine_play(service.engine,clip,desc);if play_error!=.None {delete(key,service.allocator);service.last_error=play_error;return play_error}
            if has_voice {audio.voice_stop(service.engine,active.handle);delete(active.key,service.allocator)}
            active={handle=handle,key=key,looping=emitter.looping};service.voices[id]=active
        }else {delete(key,service.allocator)}
        gain,pan,pitch,occlusion:=emitter.volume,f32(0),f32(1),f32(0)
        if emitter.spatial {
            world_matrix,error:=scene_world_matrix(owner,id);if error!=.None {return .Invalid_Parameter};position:=km.mat4_extract_translation(world_matrix)
            attenuation,panning:=audio_distance_pan(position,listener,forward,up,emitter);gain*=attenuation;pan=panning
            emitter_velocity:=km.VEC3_ZERO;if active.has_position {emitter_velocity=(position-active.position)/max(delta_seconds,.001)}
            pitch=audio_doppler(position,listener,emitter_velocity,velocity);active.position=position;active.has_position=true;service.voices[id]=active
            if ecs.contains_resource(&owner.world,box3d.Backend) {
                delta:=listener-position;distance:=km.length(delta)
                if distance>.001 {hit:=physics_raycast(owner,position,delta/distance,distance*.99);if hit.error!=.None {service.last_error=.Not_Initialized;return .Not_Initialized};if hit.hit {occlusion=clamp(1-hit.distance/distance,.1,1)*.85}}
            }
        }
        control_error:=audio.voice_controls(service.engine,active.handle,gain,pan,pitch,occlusion,emitter.spatial)
        if control_error!=.None {service.last_error=control_error;return control_error}
    }
    for index:=len(service.script_voices)-1;index>=0;index-=1 {if audio.voice_state(service.engine,service.script_voices[index])==.Stopped {unordered_remove(&service.script_voices,index)}}
    audio.engine_collect(service.engine);service.last_error=.None;return .None
}
/// Register an owned cue whose variants all resolve through the same confined asset roots.
audio_service_register_cue :: proc(service:^Audio_Service,name:string,sources:[]Audio_Source,mode:audio.Cue_Mode=.Random,category:audio.Category=.Sfx,pitch_semitones:f32=0,volume_db:f32=0)->audio.Error {
    if name==""||len(name)>4096||len(service.cues)>=256 {return .Invalid_Parameter}
    clips:=make([]^audio.Clip,len(sources),service.allocator);defer delete(clips,service.allocator)
    for source,i in sources {clip,error:=audio_service_clip(service,source);if error!=.None {return error};clips[i]=clip}
    cue:=new(audio.Sound_Cue,service.allocator);error:=audio.cue_init(cue,clips,category,mode,42,service.allocator);if error!=.None {free(cue,service.allocator);return error};cue.pitch_semitones=pitch_semitones;cue.volume_db=volume_db
    if old,present:=service.cues[name];present {audio.cue_destroy(old);free(old,service.allocator);service.cues[name]=cue}else {service.cues[strings.clone(name,service.allocator)]=cue};return .None
}
audio_service_play_cue :: proc(service:^Audio_Service,name:string)->(audio.Voice_Handle,audio.Error) {cue,present:=service.cues[name];if !present {return {},.Invalid_Parameter};return audio.cue_play(cue,service.engine)}

/// Reload actual source bytes transactionally; current voices retain the previous immutable clip revision.
audio_service_reload :: proc(service:^Audio_Service,source:Audio_Source)->audio.Error {
    key:=audio_source_key(source,service.allocator);defer delete(key,service.allocator)
    previous,present:=service.clips[key]
    bytes,error:=audio_source_bytes(service.owner,source);if error!=.None {return error};defer delete(bytes,service.allocator)
    candidate,decode:=audio.clip_decode(bytes,service.allocator);if decode!=.None {return decode}
    samples:=service.cache_samples;if present {samples-=previous.metadata.sample_count}
    if candidate.metadata.sample_count>audio.MAX_PCM_SAMPLES-samples||!present&&len(service.clips)>=256 {audio.clip_release(candidate);return .Capacity_Exceeded}
    if present {service.clips[key]=candidate;audio.clip_release(previous)}else {service.clips[strings.clone(key,service.allocator)]=candidate}
    service.cache_samples=samples+candidate.metadata.sample_count;return .None
}
/// Stop one entity without invalidating other scene voices or the editor's preview.
audio_service_stop_entity :: proc(service:^Audio_Service,id:ecs.Entity_Id) {
    if voice,present:=service.voices[id];present {audio.voice_stop(service.engine,voice.handle);delete(voice.key,service.allocator);delete_key(&service.voices,id)}
}
