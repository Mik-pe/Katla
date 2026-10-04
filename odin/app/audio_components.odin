//! Durable audio descriptors carry confined source identity; device and voice owners stay in Audio_Service.
package app
import audio "../audio"
import ecs "../ecs"
import editor "../editor"
import resources "../resources"
import "core:strings"
import "core:mem"
import "core:encoding/json"
Distance_Model :: enum {Inverse_Clamped,Linear,Exponential}
Audio_Source :: struct {path:string,root:Mesh_Path_Root}
Audio_Listener :: struct {}
Audio_Emitter :: struct {
    source_path:string,root:Mesh_Path_Root,
    volume:f32,looping,playing,spatial:bool,
    min_distance,max_distance,rolloff_factor:f32,distance_model:Distance_Model,
}
Reverb_Zone :: struct {decay,wet,dampening:f32,half_extents:[3]f32}
AUDIO_EMITTER_DEFAULT :: Audio_Emitter{volume=1,playing=true,min_distance=1,max_distance=100,rolloff_factor=1}
REVERB_ZONE_DEFAULT :: Reverb_Zone{.7,.4,.3,{5,3,5}}
@(private="package")
audio_source_destroy :: proc(value:rawptr) {source:=cast(^Audio_Source)value;delete(source.path);source^={}}
@(private="package")
audio_source_clone :: proc(dst,src:rawptr) {source:=cast(^Audio_Source)src;target:=cast(^Audio_Source)dst;target^=source^;target.path=strings.clone(source.path)}
@(private="package")
audio_emitter_destroy :: proc(value:rawptr) {emitter:=cast(^Audio_Emitter)value;delete(emitter.source_path);emitter^={}}
@(private="package")
audio_emitter_clone :: proc(dst,src:rawptr) {source:=cast(^Audio_Emitter)src;target:=cast(^Audio_Emitter)dst;target^=source^;target.source_path=strings.clone(source.source_path)}
/// Reads bytes through the selected Resource/Project or explicit File directory capability.
audio_source_bytes :: proc(owner:^Authoring,source:Audio_Source)->([]u8,audio.Error) {
    if owner==nil||source.root>.File {return nil,.Invalid_Parameter}
    scope,scope_error:=asset_path_scope(owner,source.root,source.path)
    if scope_error!=.None {return nil,.Invalid_Parameter};defer asset_path_scope_destroy(&scope)
    bytes,error:=resources.read_bytes(&scope.root,scope.path,audio.MAX_ENCODED_BYTES)
    if error!=.None {return nil,.IO};return bytes,.None
}
/// Metadata reflects the actual confined bytes, independently of output-device availability.
audio_source_metadata :: proc(owner:^Authoring,source:Audio_Source)->(audio.Metadata,audio.Error) {
    if source.path=="" {return {},.Unconfigured}
    bytes,error:=audio_source_bytes(owner,source);if error!=.None {return {},error};defer delete(bytes,owner.world.allocator)
    return audio.metadata(bytes)
}
@(private="package")
audio_emitter_valid :: proc(value:Audio_Emitter)->bool {return value.root<=.File&&finite_nonnegative(value.volume)&&finite_nonnegative(value.min_distance)&&value.min_distance>0&&finite_nonnegative(value.max_distance)&&value.max_distance>value.min_distance&&finite_nonnegative(value.rolloff_factor)&&value.distance_model<=.Exponential}
@(private="package")
audio_zone_valid :: proc(value:Reverb_Zone)->bool {if !finite_nonnegative(value.decay)||value.decay>.99||!finite_nonnegative(value.wet)||value.wet>1||!finite_nonnegative(value.dampening)||value.dampening>1 {return false};for extent in value.half_extents {if !finite_nonnegative(extent)||extent<=0 {return false}};return true}
@(private="package")
audio_component_encode :: proc(state,value:rawptr,allocator:mem.Allocator)->([]byte,bool) {
    // The entry's stationary state selects its concrete source-bearing descriptor.
    kind:=cast(^Audio_Codec_State)state
    if kind.emitter {bytes,error:=json.marshal((cast(^Audio_Emitter)value)^,allocator=allocator);return bytes,error==nil}
    bytes,error:=json.marshal((cast(^Audio_Source)value)^,allocator=allocator);return bytes,error==nil
}
Audio_Codec_State :: struct {owner:^Authoring,emitter:bool}
@(private="package")
audio_component_decode :: proc(state:rawptr,bytes:[]byte,allocator:mem.Allocator)->(rawptr,bool) {
    context.allocator=allocator;codec:=cast(^Audio_Codec_State)state
    tree,parse_error:=json.parse(bytes,spec=.JSON,parse_integers=true,allocator=allocator)
    valid:=parse_error==nil;defer json.destroy_value(tree)
    if object,is_object:=tree.(json.Object);is_object {if codec.emitter {valid=valid&&recipe_keys(object,{"source_path","root","volume","looping","playing","spatial","min_distance","max_distance","rolloff_factor","distance_model"})}else {valid=valid&&recipe_keys(object,{"path","root"})}}else {valid=false}
    if codec.emitter {
        value:=new(Audio_Emitter,allocator);error:=json.unmarshal(bytes,value,allocator=allocator)
        if !valid||error!=nil||!audio_emitter_valid(value^) {return value,false}
        if value.source_path=="" {return value,true}
        _,source_error:=audio_source_metadata(codec.owner,{value.source_path,value.root});return value,source_error==.None
    }
    value:=new(Audio_Source,allocator);error:=json.unmarshal(bytes,value,allocator=allocator)
    if !valid||error!=nil||value.root>.File {return value,false};if value.path=="" {return value,true};_,source_error:=audio_source_metadata(codec.owner,value^);return value,source_error==.None
}
Audio_Codecs :: struct {source,emitter:Audio_Codec_State}
/// Install canonical editable components and source-only owned history codecs. No device is opened here.
audio_register :: proc(owner:^Authoring) {
    ecs.insert_resource(&owner.world,Audio_Codecs{source={owner,false},emitter={owner,true}})
    codecs:=ecs.get_resource_mut(&owner.world,Audio_Codecs)
    editor.editor_register(&owner.world,&owner.registry,"AudioSource",Audio_Source{},ecs.Value_Ops{audio_source_destroy,audio_source_clone},spawn_default=false)
    editor.editor_register(&owner.world,&owner.registry,"AudioEmitter",AUDIO_EMITTER_DEFAULT,ecs.Value_Ops{audio_emitter_destroy,audio_emitter_clone},spawn_default=false)
    editor.editor_register(&owner.world,&owner.registry,"AudioListener",Audio_Listener{},spawn_default=false)
    editor.editor_register(&owner.world,&owner.registry,"ReverbZone",REVERB_ZONE_DEFAULT,spawn_default=false)
    source:=owner.registry.entries["AudioSource"];source.value_state=&codecs.source;source.encode_owned=audio_component_encode;source.decode_owned=audio_component_decode
    emitter:=owner.registry.entries["AudioEmitter"];emitter.value_state=&codecs.emitter;emitter.encode_owned=audio_component_encode;emitter.decode_owned=audio_component_decode
}
