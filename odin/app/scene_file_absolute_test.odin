#+test
#+build darwin, linux
package app

import "core:testing"
import "core:os"
import "core:strings"
import "core:encoding/json"
import "core:time"
import ron "../encoding/ron"
import ecs "../ecs"
import editor "../editor"
import resources "../resources"

@(test)
test_external_scene_file_origins_rebase_actual_assets_and_save_metadata :: proc(t:^testing.T) {
    directory,directory_error:=os.make_directory_temp("","katla-absolute-scene-*",context.allocator); testing.expect(t,directory_error==nil); if directory_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    for name in ([5]string{"project","project/resources","external","external/objects","destination"}) { path:=strings.concatenate({directory,"/",name}); testing.expect_value(t,os.make_directory(path),os.Error(nil)); delete(path) }
    project:=strings.concatenate({directory,"/project"}); defer delete(project); resource:=strings.concatenate({project,"/resources"}); defer delete(resource)
    mesh_file:=strings.concatenate({directory,"/external/objects/cube.katmesh"}); defer delete(mesh_file)
    testing.expect_value(t,os.write_entire_file(mesh_file,`(version:1,name:"Cube",parts:[(id:"cube",geometry:(kind:"cube",size:(1,2,3)))])`),os.Error(nil))
    source_file:=strings.concatenate({directory,"/external/scene.katla"}); defer delete(source_file)
    testing.expect_value(t,os.write_entire_file(source_file,`(version:3,name:"External",author:"Ada",created_at:"123",engine_version:"old",next_entity_id:2,entities:[(id:1,transform:(),source:MeshAsset(path:Scene("objects/cube.katmesh")),perspective:(fov:75,near:0.1,aspect_ratio:1.5))])`),os.Error(nil))
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None); testing.expect_value(t,asset_resources_init(&owner,project,resource),resources.Error.None)
    loaded,load_group:=scene_file_execute(&owner,{action=.Load,path=source_file,has_path=true}); defer editor.tool_result_destroy(&loaded); defer editor.undo_group_destroy(&load_group)
    testing.expect_value(t,loaded.error,editor.Scene_Error.None); testing.expect(t,len(loaded.entities)==1); if len(loaded.entities)!=2 { return }
    id:=loaded.entities[0]; mesh,present:=ecs.get_component(&owner.world,id,Scene_Mesh); testing.expect(t,present && mesh.source.root==.File && mesh.source.path==mesh_file && len(mesh.geometry.vertices)>0)
    destination:=strings.concatenate({directory,"/destination/renamed.scene"}); defer delete(destination)
    before:=time.to_unix_seconds(time.now()); saved,save_group:=scene_file_execute(&owner,{action=.Save,path=destination,has_path=true}); defer editor.tool_result_destroy(&saved); defer editor.undo_group_destroy(&save_group); testing.expect_value(t,saved.error,editor.Scene_Error.None)
    state,has_state:=ecs.get_resource(&owner.world,Scene_File_State); testing.expect(t,has_state && state.path==destination && state.name=="External")
    bytes,read_error:=os.read_entire_file(destination,context.allocator); testing.expect_value(t,read_error,os.Error(nil)); defer delete(bytes)
    tree,parse_error:=ron.parse(string(bytes)); testing.expect(t,parse_error.kind==.None); defer json.destroy_value(tree)
    fields:=tree.(json.Object); created,_:=fields["created_at"].(string); modified,_:=fields["modified_at"].(string); version,_:=fields["engine_version"].(string)
    stamp,stamp_valid:=ron.decimal_u64(modified); testing.expect(t,created=="123" && stamp_valid && stamp>=u64(before) && stamp<=u64(time.to_unix_seconds(time.now())) && version==SCENE_ENGINE_VERSION)
    source:=fields["entities"].(json.Array)[0].(json.Object)["source"]; _,payload,_:=scene_variant(source); path_variant:=payload.(json.Object)["path"]; variant,path_payload,_:=scene_variant(path_variant); if array,is_array:=path_payload.(json.Array); is_array { path_payload=array[0] }; testing.expect(t,variant=="File" && path_payload.(string)==mesh_file)
    again,again_group:=scene_file_execute(&owner,{action=.Load,path=destination,has_path=true}); defer editor.tool_result_destroy(&again); defer editor.undo_group_destroy(&again_group); testing.expect(t,again.error==.None && !ecs.entity_exists(&owner.world,id) && len(again.entities)==1)
    portable:=strings.concatenate({directory,"/external/portable.katla"}); defer delete(portable)
    portable_result,portable_group:=scene_file_execute(&owner,{action=.Save,path=portable,has_path=true}); defer editor.tool_result_destroy(&portable_result); defer editor.undo_group_destroy(&portable_group); testing.expect_value(t,portable_result.error,editor.Scene_Error.None)
    wire,wire_error:=os.read_entire_file(portable,context.allocator); defer delete(wire); testing.expect(t,wire_error==nil && strings.contains(string(wire),`Scene("objects/cube.katmesh",`))
    // The exact File child remains confined: a replacement symlink is rejected without retiring the scene.
    outside:=strings.concatenate({directory,"/outside.katmesh"}); defer delete(outside); testing.expect_value(t,os.write_entire_file(outside,"outside"),os.Error(nil)); testing.expect_value(t,os.remove(mesh_file),os.Error(nil)); testing.expect_value(t,os.symlink(outside,mesh_file),os.Error(nil))
    rejected,rejected_group:=scene_file_execute(&owner,{action=.Load,path=source_file,has_path=true}); defer editor.tool_result_destroy(&rejected); defer editor.undo_group_destroy(&rejected_group)
    testing.expect(t,rejected.error!=.None && ecs.entity_exists(&owner.world,again.entities[0]) && owner.world.live_count==1)
}

@(private="file")
absolute_test_reject_prepare :: proc(state:rawptr,owner:^Authoring,ids:[]ecs.Entity_Id,mode:Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) { return nil,.Invalid_Operation }
@(private="file")
absolute_test_reject_finish :: proc(state,token:rawptr,committed:bool) {}

@(private="file")
external_document_sources :: proc(t:^testing.T,native:bool) {
    directory,error:=os.make_directory_temp("","katla-external-document-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    for folder in ([5]string{"project","project/resources","document","models","code"}) { path:=strings.concatenate({directory,"/",folder}); testing.expect(t,os.make_directory(path)==nil); delete(path) }
    resource:=strings.concatenate({directory,"/project/resources"}); defer delete(resource); project:=strings.concatenate({directory,"/project"}); defer delete(project)
    for name in ([2]string{"Box.gltf","Box_data.bin"}) { destination:=strings.concatenate({directory,"/models/",name}); source:=strings.concatenate({"resources/models/",name}); testing.expect(t,os.copy_file(destination,source)==nil); delete(destination); delete(source) }
    model:=strings.concatenate({directory,"/models/Box.gltf"}); defer delete(model)
    script_base:=strings.concatenate({directory,"/code/external"}); defer delete(script_base); script_file:=strings.concatenate({script_base,".luau"}); defer delete(script_file); testing.expect(t,os.write_entire_file(script_file,"speed=3")==nil)
    audio_file:=strings.concatenate({directory,"/models/tone.wav"}); defer delete(audio_file); testing.expect(t,os.copy_file(audio_file,"odin/audio/testdata/tone.wav")==nil)
    path:=strings.concatenate({directory,"/document/open.katla"}); defer delete(path)
    Document_Entity :: struct {id:u64,source:struct {GltfModel:struct {path:struct {File:string}}},script:struct {path:struct {File:string}},audio_source:struct {path:struct {File:string}}}
    wire,wire_error:=json.marshal(struct {version:u32,name:string,next_entity_id:u64,entities:[]Document_Entity}{3,"External",2,{{1,{{{model}}},{{script_base}},{{audio_file}}}}}); defer delete(wire); testing.expect(t,wire_error==nil && os.write_entire_file(path,wire)==nil)
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect(t,authoring_services_init(&owner)==.None && asset_resources_init(&owner,project,resource)==.None)
    if native { testing.expect(t,script_native_init(&owner,LUAU_APP_LIBRARY)==.None) }
    before:=ecs.create_entity(&owner.world)
    ecs.insert_resource(&owner.world,Scene_Participant{nil,absolute_test_reject_prepare,absolute_test_reject_finish})
    rejected,rejected_group:=scene_file_execute(&owner,{action=.Load,path=path,has_path=true}); defer editor.tool_result_destroy(&rejected); defer editor.undo_group_destroy(&rejected_group)
    testing.expect_value(t,rejected.error,editor.Scene_Error.Invalid_Operation); testing.expect(t,ecs.entity_exists(&owner.world,before) && owner.world.live_count==1)
    unauthorized,unauthorized_error:=script_source_read(&owner,{path=script_file,root=.File}); delete(unauthorized); testing.expect(t,unauthorized_error==.Invalid_Field_Value)
    ecs.remove_resource(&owner.world,Scene_Participant)
    loaded,load_group:=scene_file_execute(&owner,{action=.Load,path=path,has_path=true}); defer editor.tool_result_destroy(&loaded); defer editor.undo_group_destroy(&load_group); testing.expect_value(t,loaded.error,editor.Scene_Error.None); testing.expect(t,len(loaded.entities)==2 && !ecs.entity_exists(&owner.world,before)); if len(loaded.entities)!=2 { return }
    id:ecs.Entity_Id
    for entity in loaded.entities { if candidate,present:=ecs.get_component(&owner.world,entity,Scene_Model); present && candidate.source.kind==.Group { id=entity; break } }
    imported,has_model:=ecs.get_component(&owner.world,id,Scene_Model); testing.expect(t,has_model && imported.source.root==.File && imported.source.path==model && len(imported.model.primitives)>0)
    source,has_script:=ecs.get_component(&owner.world,id,Script_Component); testing.expect(t,has_script && source.root==.File && source.path==script_file)
    bytes,read_error:=script_source_read(&owner,source); defer delete(bytes); testing.expect(t,read_error==.None && string(bytes)=="speed=3")
    source_audio,has_audio:=ecs.get_component(&owner.world,id,Audio_Source); metadata,metadata_error:=audio_source_metadata(&owner,source_audio); testing.expect(t,has_audio && source_audio.root==.File && source_audio.path==audio_file && metadata_error==.None && metadata.frames==4800)
    if native {
        played,play_group:=simulation_execute(&owner,.Play); defer editor.tool_result_destroy(&played); defer editor.undo_group_destroy(&play_group); testing.expect(t,played.error==.None && owner.mode==.Playing)
        stopped,stop_group:=simulation_execute(&owner,.Stop); defer editor.tool_result_destroy(&stopped); defer editor.undo_group_destroy(&stop_group); testing.expect(t,stopped.error==.None && owner.mode==.Editing && !ecs.entity_exists(&owner.world,id) && owner.world.live_count==2)
    }
}

@(test)
test_external_document_model_audio_and_script_capabilities_publish_atomically :: proc(t:^testing.T) { external_document_sources(t,false) }
when LUAU_APP_LIBRARY!="" {
    @(test)
    test_external_document_script_file_load_play_stop :: proc(t:^testing.T) { external_document_sources(t,true) }
}
