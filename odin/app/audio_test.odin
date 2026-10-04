#+test
package app
import audio "../audio"
import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:testing"
import "core:encoding/json"
import "core:strings"

@(test)
test_audio_actual_rooted_preview_toggle_failure_and_deleted_emitter :: proc(t:^testing.T) {
    owner:Authoring;authoring_init(&owner);defer authoring_destroy(&owner)
    if owner.registry.entries["AudioEmitter"]==nil {audio_register(&owner)}
    testing.expect(t,asset_resources_init(&owner,".","resources")==.None)
    service:Audio_Service;testing.expect(t,audio_service_init(&service,&owner,false)==.None);defer audio_service_destroy(&service)
    source:=Audio_Source{"odin/audio/testdata/tone.wav",.Project}
    metadata,error:=audio_source_metadata(&owner,source);testing.expect(t,error==.None&&metadata.frames==4800)
    testing.expect(t,audio_service_preview(&service,source)==.None&&audio_service_snapshot(&service).preview_playing)
    previous:=service.preview
    testing.expect(t,audio_service_preview(&service,{"../escape.wav",.Project})==.Invalid_Parameter&&service.preview==previous&&audio.voice_state(service.engine,previous)==.Playing)
    testing.expect(t,audio_service_preview(&service,source)==.None&&!audio_service_snapshot(&service).preview_playing)
    id:=ecs.create_entity(&owner.world);ecs.add_component(&owner.world,id,Scene_Transform{km.transform(position={10,0,0})})
    emitter:=AUDIO_EMITTER_DEFAULT;emitter.source_path=strings.clone(source.path);emitter.root=.Project;emitter.looping=true;emitter.spatial=true;ecs.add_component(&owner.world,id,emitter)
    testing.expect(t,audio_service_update(&service,1.0/60)==.None&&len(service.voices)==1)
    block:[1024]f32;audio.engine_render(service.engine,block[:]);snapshot:=audio_service_snapshot(&service);testing.expect(t,snapshot.levels.sfx.peak>0&&snapshot.active_voices==1)
    voice:=service.voices[id].handle;ecs.destroy_entity(&owner.world,id)
    testing.expect(t,audio_service_update(&service,1.0/60)==.None&&len(service.voices)==0);audio.engine_render(service.engine,block[:]);testing.expect(t,audio.voice_state(service.engine,voice)==.Stopped)
}
@(test)
test_audio_document_actual_source_codec_roundtrip_and_confinement :: proc(t:^testing.T) {
    owner:Authoring;authoring_init(&owner);defer authoring_destroy(&owner);testing.expect(t,asset_resources_init(&owner,".","resources")==.None)
    if owner.registry.entries["AudioEmitter"]==nil {audio_register(&owner)}
    tree,parse_error:=json.parse(`{"audio_emitter":{"path":{"Scene":"tone.wav"},"volume":0.7,"looping":true,"spatial":true,"distance_model":"Exponential"},"reverb_zone":{"decay":0.8,"wet":0.3,"dampening":0.2,"half_extents":[4,3,2]},"audio_listener":true}`,spec=.JSON,parse_integers=true)
    testing.expect(t,parse_error==nil);defer json.destroy_value(tree)
    row:=Scene_Entity{components=make([dynamic]Scene_Component)};defer {for &component in row.components {delete(component.name);delete(component.data)};delete(row.components)}
    testing.expect(t,audio_scene_decode(&owner,&row,tree.(json.Object),"odin/audio/testdata/test.katla")==.None&&len(row.components)==3)
    fields:=make(json.Object);defer json.destroy_value(json.Value(fields))
    testing.expect(t,audio_scene_encode(&owner,row,&fields,"odin/audio/testdata/copy.katla")==.None)
    emitted:=fields["audio_emitter"].(json.Object);testing.expect(t,emitted["distance_model"].(string)=="Exponential"&&emitted["looping"].(bool))
    entry:=owner.registry.entries["AudioEmitter"];component:=row.components[0];value,valid:=editor.editor_decode_value(entry,component.data,owner.world.allocator);defer {entry.ops.destroy(value);free(value,owner.world.allocator)}
    testing.expect(t,valid&&(cast(^Audio_Emitter)value).source_path=="odin/audio/testdata/tone.wav")
    bad:="{\"source_path\":\"../escape.wav\",\"root\":\"Project\",\"volume\":1,\"min_distance\":1,\"max_distance\":100,\"rolloff_factor\":1}"
    rejected,ok:=editor.editor_decode_value(entry,transmute([]byte)bad,owner.world.allocator);entry.ops.destroy(rejected);free(rejected,owner.world.allocator);testing.expect(t,!ok)
}
@(test)
test_audio_spatial_distance_doppler_zone_and_mixer_controls :: proc(t:^testing.T) {
    emitter:=AUDIO_EMITTER_DEFAULT
    inverse,pan:=audio_distance_pan({10,0,0},{},{0,0,-1},{0,1,0},emitter);testing.expect(t,abs(inverse-.1)<1e-6&&pan==1)
    emitter.distance_model=.Linear;emitter.max_distance=11;linear,_:=audio_distance_pan({6,0,0},{},{0,0,-1},{0,1,0},emitter);testing.expect(t,abs(linear-.5)<1e-6)
    emitter.distance_model=.Exponential;exponential,_:=audio_distance_pan({10,0,0},{},{0,0,-1},{0,1,0},emitter);testing.expect(t,abs(exponential-.1)<1e-6)
    testing.expect(t,audio_doppler({10,0,0},{},{-30,0,0},{})<1)
}
when BOX3D_LIBRARY!="" {
@(test)
test_audio_actual_native_physics_occlusion_and_entity_lifecycle :: proc(t:^testing.T) {
    owner:Authoring;authoring_init(&owner);defer authoring_destroy(&owner);register_test_scene_runtime(&owner)
    testing.expect(t,asset_resources_init(&owner,".","resources")==.None&&physics_select_box3d(&owner,BOX3D_LIBRARY)==.None)
    service:Audio_Service;testing.expect(t,audio_service_init(&service,&owner,false)==.None);defer audio_service_destroy(&service)
    wall:=ecs.create_entity(&owner.world);ecs.add_component(&owner.world,wall,Scene_Transform{km.transform(position={5,0,0})});ecs.add_component(&owner.world,wall,physics_body({kind=.Box,half_extents={.5,2,2}},.Fixed))
    id:=ecs.create_entity(&owner.world);ecs.add_component(&owner.world,id,Scene_Transform{km.transform(position={10,0,0})});emitter:=AUDIO_EMITTER_DEFAULT;emitter.source_path=strings.clone("odin/audio/testdata/tone.wav");emitter.root=.Project;emitter.spatial=true;emitter.looping=true;ecs.add_component(&owner.world,id,emitter)
    testing.expect(t,physics_prepare(&owner)==.None&&audio_service_update(&service,1.0/60)==.None)
    handle:=service.voices[id].handle;voice:=&service.engine.voices[handle.slot];testing.expect(t,abs(voice.occlusion-.4675)<.001)
    ecs.destroy_entity(&owner.world,wall);testing.expect(t,physics_prepare(&owner)==.None&&audio_service_update(&service,1.0/60)==.None&&voice.occlusion==0)
}
}

@(test)
test_audio_unconfigured_add_snapshot_and_explicit_metadata_state :: proc(t:^testing.T) {
    owner:Authoring;authoring_init(&owner);defer authoring_destroy(&owner);testing.expect(t,authoring_services_init(&owner)==.None)
    id:=ecs.create_entity(&owner.world);ecs.add_component(&owner.world,id,Scene_Transform{km.TRANSFORM_IDENTITY});ecs.add_component(&owner.world,id,AUDIO_EMITTER_DEFAULT);ecs.add_component(&owner.world,id,Audio_Source{})
    testing.expect(t,audio_scene_validate(&owner,{id})==.None)
    _,metadata_error:=audio_source_metadata(&owner,{});testing.expect_value(t,metadata_error,audio.Error.Unconfigured)
    snapshot,capture_error:=scene_snapshot_capture(&owner);defer scene_snapshot_destroy(&snapshot);testing.expect(t,capture_error==.None)
    fields:=make(json.Object);defer json.destroy_value(json.Value(fields));testing.expect(t,audio_scene_encode(&owner,snapshot.entities[0],&fields,"scenes/empty.katla")==.None)
    row:=Scene_Entity{components=make([dynamic]Scene_Component)};defer {for component in row.components {delete(component.name);delete(component.data)};delete(row.components)}
    testing.expect(t,audio_scene_decode(&owner,&row,fields,"scenes/empty.katla")==.None)
    service:Audio_Service;testing.expect(t,audio_service_init(&service,&owner,false)==.None);defer audio_service_destroy(&service)
    testing.expect(t,audio_service_update(&service,1.0/60)==.None&&len(service.voices)==0&&audio_service_snapshot(&service).unconfigured_emitters==1)
}

@(test)
test_audio_runtime_owned_script_loop_cue_reset_and_world_teardown :: proc(t:^testing.T) {
    owner:Authoring;authoring_init(&owner);defer authoring_destroy(&owner);testing.expect(t,authoring_services_init(&owner)==.None)
    testing.expect(t,asset_resources_init(&owner,".","odin/audio/testdata")==.None&&audio_runtime_init(&owner,false)==.None)
    runtime:=ecs.get_resource_mut(&owner.world,Audio_Runtime);testing.expect(t,runtime!=nil&&runtime.service.engine!=nil)
    testing.expect(t,audio_script_play(&owner,"tone.wav",.7,true,{1,0,0},true)==.None)
    service:=runtime.service;testing.expect(t,len(service.script_voices)==1)
    testing.expect(t,audio_service_register_cue(service,"actual",{{"tone.ogg",.Resource},{"tone.mp3",.Resource}},.Shuffle)==.None&&audio_script_cue(&owner,"actual")==.None)
    testing.expect(t,len(service.script_voices)==2&&audio_runtime_update(&owner,1.0/60)==.None)
    block:[1024]f32;audio.engine_render(service.engine,block[:]);testing.expect(t,audio_service_snapshot(service).levels.sfx.peak>0)
    handles:=[2]audio.Voice_Handle{service.script_voices[0],service.script_voices[1]};audio_runtime_reset(&owner);audio.engine_render(service.engine,block[:]);audio.engine_collect(service.engine)
    testing.expect(t,len(service.script_voices)==0&&audio.voice_state(service.engine,handles[0])==.Stopped&&audio.voice_state(service.engine,handles[1])==.Stopped)
    testing.expect(t,audio_script_play(&owner,"../escape.wav",1,false)==.Invalid_Field_Value)
}

@(test)
test_audio_explicit_file_source_uses_exact_parent_capability :: proc(t:^testing.T) {
    owner:Authoring;authoring_init(&owner);defer authoring_destroy(&owner)
    testing.expect(t,asset_resources_init(&owner,".","resources")==.None)
    absolute,valid:=asset_absolute_path(&owner,.Project,"odin/audio/testdata/tone.wav");defer delete(absolute,owner.world.allocator);testing.expect(t,valid)
    source:=Audio_Source{absolute,.File}
    metadata,error:=audio_source_metadata(&owner,source);testing.expect(t,error==.None&&metadata.frames==4800)
    _,relative_error:=audio_source_metadata(&owner,{"odin/audio/testdata/tone.wav",.File});testing.expect_value(t,relative_error,audio.Error.Invalid_Parameter)
    service:Audio_Service;testing.expect(t,audio_service_init(&service,&owner,false)==.None);defer audio_service_destroy(&service)
    testing.expect_value(t,audio_service_preview(&service,source),audio.Error.None)
    testing.expect(t,strings.has_prefix(service.preview_key,"file:"))
    block:[1024]f32;audio.engine_render(service.engine,block[:]);testing.expect(t,audio_service_snapshot(&service).levels.sfx.peak>0)
}
