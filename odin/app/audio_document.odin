//! Audio document DTOs rebase confined paths before shared scene staging.
package app
import ecs "../ecs"
import editor "../editor"
import "core:encoding/json"

/// Decode audio emitter, source, listener and zone descriptions before publishing any entity.
audio_scene_decode :: proc(owner:^Authoring,row:^Scene_Entity,fields:json.Object,origin:string)->editor.Scene_Error {
    if value,present:=scene_gameplay_present(fields,"audio_source");present {
        object,is_object:=value.(json.Object);if !is_object||!recipe_keys(object,{"path"}) {return .Decode_Failed}
        path,root:="",Mesh_Path_Root.Resource;valid:=true
        if _,is_null:=object["path"].(json.Null);!is_null&&object["path"]!=nil {path,root,valid=scene_asset_path(owner,object["path"],origin)}
        if !valid {return .Invalid_Field_Value};defer delete(path,owner.world.allocator)
        if error:=scene_row_component(owner,row,"AudioSource",Audio_Source{path,root});error!=.None {return error}
    }
    if value,present:=scene_gameplay_present(fields,"audio_emitter");present {
        object,is_object:=value.(json.Object)
        if !is_object||!recipe_keys(object,{"path","volume","looping","playing","spatial","min_distance","max_distance","rolloff_factor","distance_model"})||!scene_gameplay_required(object,{"path"}) {return .Decode_Failed}
        emitter:=AUDIO_EMITTER_DEFAULT
        path,root:="",Mesh_Path_Root.Resource;valid:=true
        if _,is_null:=object["path"].(json.Null);!is_null&&object["path"]!=nil {path,root,valid=scene_asset_path(owner,object["path"],origin)}
        if !valid {return .Invalid_Field_Value};defer delete(path,owner.world.allocator);emitter.source_path=path;emitter.root=root
        if !scene_gameplay_number(object,"volume",&emitter.volume)||!scene_gameplay_number(object,"min_distance",&emitter.min_distance)||!scene_gameplay_number(object,"max_distance",&emitter.max_distance)||!scene_gameplay_number(object,"rolloff_factor",&emitter.rolloff_factor)||!scene_gameplay_bool(object,"looping",&emitter.looping)||!scene_gameplay_bool(object,"playing",&emitter.playing)||!scene_gameplay_bool(object,"spatial",&emitter.spatial) {return .Decode_Failed}
        if model,has_model:=object["distance_model"];has_model {
            kind,payload,model_valid:=scene_variant(model);if !model_valid||payload!=nil {return .Decode_Failed}
            switch kind {
            case "InverseClamped":emitter.distance_model=.Inverse_Clamped
            case "Linear":emitter.distance_model=.Linear
            case "Exponential":emitter.distance_model=.Exponential
            case:return .Invalid_Field_Value
            }
        }
        if !audio_emitter_valid(emitter) {return .Invalid_Field_Value}
        if error:=scene_row_component(owner,row,"AudioEmitter",emitter);error!=.None {return error}
    }
    if value,present:=scene_gameplay_present(fields,"audio_listener");present {
        listener,is_listener:=value.(bool);if !is_listener {return .Decode_Failed};if listener {if error:=scene_row_component(owner,row,"AudioListener",Audio_Listener{});error!=.None {return error}}
    }
    if value,present:=scene_gameplay_present(fields,"reverb_zone");present {
        object,is_object:=value.(json.Object);zone:=REVERB_ZONE_DEFAULT
        if !is_object||!recipe_keys(object,{"decay","wet","dampening","half_extents"})||!scene_gameplay_number(object,"decay",&zone.decay)||!scene_gameplay_number(object,"wet",&zone.wet)||!scene_gameplay_number(object,"dampening",&zone.dampening)||!scene_gameplay_vector(object,"half_extents",&zone.half_extents)||!audio_zone_valid(zone) {return .Decode_Failed}
        if error:=scene_row_component(owner,row,"ReverbZone",zone);error!=.None {return error}
    }
    return .None
}
/// Encode authored audio fields using exact destination-root rebasing and no runtime handles.
audio_scene_encode :: proc(owner:^Authoring,row:Scene_Entity,fields:^json.Object,origin:string)->editor.Scene_Error {
    for name in ([4]string{"AudioSource","AudioEmitter","AudioListener","ReverbZone"}) {
        if !scene_row_has(row,name) {continue}
        value,ok:=scene_row_owned_decode(owner,row,name);defer scene_row_owned_destroy(owner,name,value);if !ok {return .Decode_Failed}
        switch name {
        case "AudioListener":scene_gameplay_store(fields,"audio_listener",true)
        case "ReverbZone":zone:=(cast(^Reverb_Zone)value)^;if !audio_zone_valid(zone) {return .Invalid_Field_Value};scene_gameplay_store(fields,"reverb_zone",trigger_json_value(zone))
        case "AudioSource":
            source:=(cast(^Audio_Source)value)^;path:json.Value;valid:=true;if source.path!="" {path,valid=scene_asset_reference(owner,source.path,source.root,origin)};if !valid {return .Invalid_Operation};defer json.destroy_value(path)
            scene_gameplay_store(fields,"audio_source",trigger_json_value(struct{path:json.Value}{path}))
        case "AudioEmitter":
            emitter:=(cast(^Audio_Emitter)value)^;if !audio_emitter_valid(emitter) {return .Invalid_Field_Value}
            path:json.Value;valid:=true;if emitter.source_path!="" {path,valid=scene_asset_reference(owner,emitter.source_path,emitter.root,origin)};if !valid {return .Invalid_Operation};defer json.destroy_value(path)
            model:="InverseClamped";if emitter.distance_model==.Linear {model="Linear"}else if emitter.distance_model==.Exponential {model="Exponential"}
            scene_gameplay_store(fields,"audio_emitter",trigger_json_value(struct{path:json.Value,volume:f32,looping,playing,spatial:bool,min_distance,max_distance,rolloff_factor:f32,distance_model:string}{path,emitter.volume,emitter.looping,emitter.playing,emitter.spatial,emitter.min_distance,emitter.max_distance,emitter.rolloff_factor,model}))
        }
    }
    return .None
}
/// Admit only actual confined source bytes and valid spatial/zone parameters before scene commit.
audio_scene_validate :: proc(owner:^Authoring,ids:[]ecs.Entity_Id)->editor.Scene_Error {
    for id in ids {
        if source,present:=ecs.get_component(&owner.world,id,Audio_Source);present {if _,error:=audio_source_metadata(owner,source);error!=.None&&error!=.Unconfigured {return .Invalid_Field_Value}}
        if emitter,present:=ecs.get_component(&owner.world,id,Audio_Emitter);present {if !audio_emitter_valid(emitter) {return .Invalid_Field_Value};if _,error:=audio_source_metadata(owner,{emitter.source_path,emitter.root});error!=.None&&error!=.Unconfigured {return .Invalid_Field_Value}}
        if zone,present:=ecs.get_component(&owner.world,id,Reverb_Zone);present&&!audio_zone_valid(zone) {return .Invalid_Field_Value}
    }
    return .None
}
