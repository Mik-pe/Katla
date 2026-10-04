//! The world owns one stationary audio service; script and editor consumers share its real device and mixer.
package app
import audio "../audio"
import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:strings"
Audio_Runtime :: struct {service:^Audio_Service}
@(private="package")
audio_runtime_destroy :: proc(value:rawptr) {runtime:=cast(^Audio_Runtime)value;if runtime.service!=nil {allocator:=runtime.service.allocator;audio_service_destroy(runtime.service);free(runtime.service,allocator)};runtime^={}}
/// Publish even a failed native-device attempt as observable unavailable state, retaining retry bookkeeping.
audio_runtime_init :: proc(owner:^Authoring,native_output:bool=true)->audio.Error {
    if ecs.contains_resource(&owner.world,Audio_Runtime) {return .Already_Initialized}
    service:=new(Audio_Service,owner.world.allocator);error:=audio_service_init(service,owner,native_output)
    ecs.insert_resource(&owner.world,Audio_Runtime{service},ecs.Value_Ops{destroy=audio_runtime_destroy});return error
}
/// Retry actual hardware after a recoverable initialization failure without replacing scene ownership.
audio_runtime_retry :: proc(owner:^Authoring)->audio.Error {
    runtime:=ecs.get_resource_mut(&owner.world,Audio_Runtime);if runtime==nil||runtime.service==nil {return .Not_Initialized}
    service:=runtime.service;if service.engine!=nil {error:=audio.engine_reopen_device(service.engine);service.last_error=error;return error}
    engine,error:=audio.engine_create(allocator=service.allocator);service.last_error=error
    if error!=.None {return error};service.engine=engine;_,zone_error:=audio.engine_create_zone_reverb(engine);service.last_error=zone_error;return zone_error
}
/// Process device recovery, preview and authored emitter playing flags independently of editor simulation.
audio_runtime_update :: proc(owner:^Authoring,delta_seconds:f32)->audio.Error {
    runtime:=ecs.get_resource_mut(&owner.world,Audio_Runtime);if runtime==nil||runtime.service==nil {return .Not_Initialized}
    return audio_service_update(runtime.service,delta_seconds,true)
}
/// Stop old entity participants before restoring a preview snapshot; independent editor preview remains explicit.
audio_runtime_reset :: proc(owner:^Authoring) {runtime:=ecs.get_resource_mut(&owner.world,Audio_Runtime);if runtime!=nil&&runtime.service!=nil {audio_service_reset(runtime.service)}}
/// Script file sources resolve below the resource root; callers cannot inject unrestricted file paths.
audio_script_play :: proc(owner:^Authoring,path:string,volume:f32,looping:bool,position:km.Vec3={},spatial:bool=false)->editor.Scene_Error {
    runtime:=ecs.get_resource_mut(&owner.world,Audio_Runtime);if runtime==nil||runtime.service==nil||runtime.service.engine==nil {return .Application_Owned}
    service:=runtime.service;relative:=path
    if roots:=ecs.get_resource_mut(&owner.world,Asset_Roots);roots!=nil&&len(roots.resource.path)>len(roots.project.path)+1 {
        prefix:=roots.resource.path[len(roots.project.path)+1:]
        if len(path)>len(prefix)&&strings.has_prefix(path,prefix)&&path[len(prefix)]=='/' {relative=path[len(prefix)+1:]}
    }
    clip,error:=audio_service_clip(service,{relative,.Resource});if error!=.None {service.last_error=error;return .Invalid_Field_Value}
    if !finite_nonnegative(volume)||volume>1 {return .Invalid_Field_Value}
    desc:=audio.DEFAULT_PLAY;desc.volume=volume;desc.looping=looping
    if spatial&&service.has_listener {listener:=service.listener_position;emitter:=AUDIO_EMITTER_DEFAULT;attenuation,pan:=audio_distance_pan(position,listener,service.listener_forward,service.listener_up,emitter);desc.volume*=attenuation;desc.pan=pan}
    if len(service.script_voices)>=256 {return .Invalid_Operation}
    handle,play_error:=audio.engine_play(service.engine,clip,desc);service.last_error=play_error
    if play_error!=.None {return .Invalid_Operation}
    append(&service.script_voices,handle)
    if resume_error:=audio.engine_resume(service.engine);resume_error!=.None {service.last_error=resume_error;return .Application_Owned};return .None
}
audio_script_cue :: proc(owner:^Authoring,name:string)->editor.Scene_Error {
    runtime:=ecs.get_resource_mut(&owner.world,Audio_Runtime);if runtime==nil||runtime.service==nil||runtime.service.engine==nil {return .Application_Owned}
    if len(runtime.service.script_voices)>=256 {return .Invalid_Operation}
    handle,error:=audio_service_play_cue(runtime.service,name);runtime.service.last_error=error;if error!=.None {return .Invalid_Operation}
    append(&runtime.service.script_voices,handle)
    if audio.engine_resume(runtime.service.engine)!=.None {return .Application_Owned};return .None
}
