//! Scene file publication prepares complete documents and commits origins after atomic filesystem success.
package app

import asset "../agent/assets"
import editor "../editor"
import ecs "../ecs"
import resources "../resources"
import ron "../encoding/ron"
import "core:encoding/json"
import "core:strings"
import "core:fmt"
import "core:time"
import "core:path/filepath"

/// Identifies the authored engine schema version recorded at actual save publication.
SCENE_ENGINE_VERSION :: #config(KATLA_ENGINE_VERSION,"0.1.0")

/// Retains the opened origin and header metadata independently from runtime entity identity.
Scene_File_State :: struct {path,name:string,header:[]byte}
@(private="package")
scene_file_state_destroy :: proc(value:rawptr) { state:=cast(^Scene_File_State)value; delete(state.path); delete(state.name); delete(state.header); state^={} }
@(private="package")
scene_file_state :: proc(app:^Authoring,path:string,document:json.Value)->(Scene_File_State,bool) {
    object,is_object:=document.(json.Object); if !is_object { return {},false }; name,is_name:=object["name"].(string); if !is_name { return {},false }
    header:=make(json.Object,app.world.allocator); defer delete(header)
    for key,value in object { if key!="entities" && key!="next_entity_id" { header[key]=value } }
    data,err:=json.marshal(header,allocator=app.world.allocator); if err!=nil { return {},false }
    return {strings.clone(path,app.world.allocator),strings.clone(name,app.world.allocator),data},true
}
/// Executes canonical load_scene/save_scene calls with staged replacement and explicit current-origin handling.
scene_file_execute :: proc(app:^Authoring,request:asset.Scene_File_Request)->(editor.Tool_Result,editor.Undo_Group) {
    allocator:=app.world.allocator; context.allocator=allocator; result:=error_result(&app.world,.None)
    if app.mode!=.Editing { result.error=.Editing_Required; return result,{} }
    path:=request.path
    current:=ecs.get_resource_mut(&app.world,Scene_File_State)
    if !request.has_path { if request.action!=.Save || current==nil { result.error=.Invalid_Operation; return result,{} }; path=current.path }
    scope_kind:Mesh_Path_Root=.Project; if filepath.is_abs(path) { scope_kind=.File }
    scope,scope_error:=asset_path_scope(app,scope_kind,path); if scope_error!=.None { result.error=.Invalid_Operation; return result,{} }; defer asset_path_scope_destroy(&scope)
    document:json.Value; defer json.destroy_value(document)
    snapshot:Scene_Snapshot; defer scene_snapshot_destroy(&snapshot)
    if request.action==.Load {
        bytes,read_error:=resources.read_text(&scope.root,scope.path); if read_error!=.None { result.error=.Invalid_Operation; return result,{} }; defer delete(bytes,allocator)
        parsed,parse_error:=ron.parse(string(bytes),allocator); if parse_error.kind!=.None { result.error=.Decode_Failed; return result,{} }; document=parsed
        decoded,decode_error:=scene_document_prepare(app,document,path); if decode_error!=.None { result.error=decode_error; return result,{} }; snapshot=decoded
    } else {
        captured,capture_error:=scene_snapshot_capture(app,commit_identity=false); if capture_error!=.None { result.error=capture_error; return result,{} }; snapshot=captured
        name:="Untitled"; if current!=nil { name=current.name }
        encoded,encode_error:=scene_document_encode(app,&snapshot,name,path); if encode_error!=.None { result.error=encode_error; return result,{} }; document=encoded
        if current!=nil && len(current.header)>0 {
            metadata,parse_error:=json.parse(current.header,spec=.JSON,parse_integers=true,allocator=allocator); if parse_error!=nil { result.error=.Decode_Failed; return result,{} }; defer json.destroy_value(metadata)
            if fields,is_fields:=metadata.(json.Object); is_fields { target:=document.(json.Object); for field in ([4]string{"author","created_at","modified_at","engine_version"}) { if value,present:=fields[field]; present { cloned,ok:=scene_value_clone(value); if !ok { result.error=.Decode_Failed; return result,{} }; scene_json_put(&target,field,cloned) } }; document=target }
        }
        target:=document.(json.Object)
        timestamp:=time.to_unix_seconds(time.now())
        if timestamp>=0 {
            if created,present:=scene_gameplay_present(target,"created_at"); !present || created==nil { scene_migration_set(&target,"created_at",fmt.aprintf("%d",timestamp)) }
            scene_migration_set(&target,"modified_at",fmt.aprintf("%d",timestamp))
        }
        scene_migration_set(&target,"engine_version",strings.clone(SCENE_ENGINE_VERSION))
        document=target
    }
    state,state_ok:=scene_file_state(app,path,document); if !state_ok { result.error=.Decode_Failed; return result,{} }; state_transferred:=false; defer { if !state_transferred { scene_file_state_destroy(&state) } }
    data,marshal_error:=json.marshal(struct {path,name:string,entity_count:int,published,runtime_ids_replaced:bool}{path,state.name,len(snapshot.entities),false,request.action==.Load},allocator=allocator)
    if marshal_error!=nil { result.error=.Decode_Failed; return result,{} }; result.data=data
    if request.action==.Load {
        restore_error:=scene_snapshot_restore(app,&snapshot,file_publication=true); if restore_error!=.None { result.error=restore_error; return result,{} }
        session:=&app.agent.session; next_id,paused,finished:=session.next_id,session.paused,session.finished
        editor.agent_session_destroy(session); editor.agent_session_init(session,allocator); session.next_id=next_id; session.paused=paused; session.finished=finished
        ids:=ecs.entity_ids(&app.world); defer delete(ids); for id in ids { if _,hidden:=ecs.get_component(&app.world,id,Editor_Hidden); !hidden { append(&result.entities,id) } }
        ecs.insert_resource(&app.world,state,ecs.Value_Ops{destroy=scene_file_state_destroy}); state_transferred=true
        return result,{}
    }
    observation,observe_error:=scene_file_observe_begin(app,&snapshot); if observe_error!=.None { result.error=observe_error; return result,{} }
    did_publish:=false; defer scene_file_observe_finish(&observation,did_publish)
    ron_document,ron_ok:=scene_document_ron_clone(document); if !ron_ok { result.error=.Decode_Failed; return result,{} }; defer json.destroy_value(ron_document)
    bytes,write_error:=ron.write(ron_document,allocator); if write_error.kind!=.None { result.error=.Decode_Failed; return result,{} }; defer delete(bytes,allocator)
    published_data,published_error:=json.marshal(struct {path,name:string,entity_count:int,published,runtime_ids_replaced:bool}{path,state.name,len(snapshot.entities),true,false},allocator=allocator)
    if published_error!=nil { result.error=.Decode_Failed; return result,{} }; selected:=false; defer { if !selected { delete(published_data,allocator) } }
    published,error:=resources.write_atomic(&scope.root,scope.path,bytes)
    if published {
        did_publish=true
        delete(result.data,allocator); result.data=published_data; selected=true
        scene_snapshot_commit_keys(app,&snapshot)
        ecs.insert_resource(&app.world,state,ecs.Value_Ops{destroy=scene_file_state_destroy}); state_transferred=true
    }
    if error!=.None { result.error=.Invalid_Operation }
    return result,{}
}
