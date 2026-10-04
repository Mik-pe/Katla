#+test
#+build darwin, linux
package app

import ecs "../ecs"
import editor "../editor"
import "core:testing"
import "core:os"
import "core:strings"
import "core:encoding/json"

@(private="file")
File_Batch_Native :: struct { reject:bool,commits:int }
@(private="file")
file_batch_prepare :: proc(state:rawptr,owner:^Authoring,ids:[]ecs.Entity_Id,mode:Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
    native:=cast(^File_Batch_Native)state
    if native.reject { return nil,.Invalid_Operation }; return native,.None
}
@(private="file")
file_batch_finish :: proc(state,token:rawptr,committed:bool) { if committed { (cast(^File_Batch_Native)state).commits+=1 } }

@(test)
test_absolute_model_prefab_batch_rolls_back_capabilities_and_redoes_owned_geometry :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-file-batch-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    for name in ([5]string{"project","project/resources","models","templates","code"}) { path:=strings.concatenate({directory,"/",name}); testing.expect(t,os.make_directory(path)==nil); delete(path) }
    project:=strings.concatenate({directory,"/project"}); defer delete(project); resource:=strings.concatenate({project,"/resources"}); defer delete(resource)
    for name in ([2]string{"Box.gltf","Box_data.bin"}) { destination:=strings.concatenate({directory,"/models/",name}); source:=strings.concatenate({"resources/models/",name}); testing.expect(t,os.copy_file(destination,source)==nil); delete(destination); delete(source) }
    model_file:=strings.concatenate({directory,"/models/Box.gltf"}); defer delete(model_file)
    script_base:=strings.concatenate({directory,"/code/logic"}); defer delete(script_base); script_file:=strings.concatenate({script_base,".luau"}); defer delete(script_file); testing.expect(t,os.write_entire_file(script_file,"speed=7")==nil)
    mesh_file:=strings.concatenate({directory,"/templates/piece.katmesh"}); defer delete(mesh_file); testing.expect(t,os.write_entire_file(mesh_file,`(version:1,name:"Piece",parts:[(id:"cube",geometry:(kind:"cube",size:(1,2,3)))])`)==nil)
    prefab_file:=strings.concatenate({directory,"/templates/root.katprefab"}); defer delete(prefab_file)
    encoded_script,marshal_error:=json.marshal(script_base); defer delete(encoded_script); testing.expect(t,marshal_error==nil)
    document:=strings.concatenate({`(version:1,root:1,scene:(version:3,name:"External",next_entity_id:3,entities:[(id:1,transform:(),script:(path:File(`,string(encoded_script),`))),(id:2,parent:1,transform:(position:(0,2,0)),source:MeshAsset(path:Scene("piece.katmesh")))]))`}); defer delete(document); testing.expect(t,os.write_entire_file(prefab_file,document)==nil)
    prefab_args,args_error:=json.marshal(struct {action,path:string,position:[3]f32}{"instantiate",prefab_file,{2,0,0}}); defer delete(prefab_args); testing.expect(t,args_error==nil)
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect(t,authoring_services_init(&owner)==.None && asset_resources_init(&owner,project,resource)==.None)
    before:=ecs.create_entity(&owner.world); key:=ecs.get_resource_mut(&owner.world,Scene_Identity).next_entity_id
    native:=File_Batch_Native{reject=true}; ecs.insert_resource(&owner.world,Scene_Participant{&native,file_batch_prepare,file_batch_finish})
    operations:=[2]editor.Scene_Op{{kind=.Application,tool_name="prefab",value=prefab_args},{kind=.Spawn_Model,path=model_file,scale={1,1,1}}}
    result,group:=scene_action_execute_batch(&owner,operations[:]); testing.expect_value(t,result.error,editor.Scene_Error.Invalid_Operation); testing.expect(t,group.state==nil && owner.world.live_count==1 && ecs.entity_exists(&owner.world,before) && ecs.get_resource_mut(&owner.world,Scene_Identity).next_entity_id==key); editor.tool_result_destroy(&result)
    denied,denied_error:=script_source_read(&owner,{path=script_file,root=.File}); delete(denied); testing.expect_value(t,denied_error,editor.Scene_Error.Invalid_Field_Value)
    missing:=strings.concatenate({directory,"/models/missing.gltf"}); defer delete(missing)
    native.reject=false; operations[1].path=missing
    result,group=scene_action_execute_batch(&owner,operations[:]); testing.expect(t,result.error!=.None && group.state==nil && owner.world.live_count==1 && native.commits==0); editor.tool_result_destroy(&result)
    denied,denied_error=script_source_read(&owner,{path=script_file,root=.File}); delete(denied); testing.expect_value(t,denied_error,editor.Scene_Error.Invalid_Field_Value)
    operations[1].path=model_file
    result,group=scene_action_execute_batch(&owner,operations[:]); testing.expect_value(t,result.error,editor.Scene_Error.None); testing.expect(t,len(result.entities)==4 && owner.world.live_count==5 && native.commits==1); if result.error!=.None { editor.tool_result_destroy(&result); return }
    root,imported,child:=result.entities[0],result.entities[1],result.entities[2]
    source,has_source:=ecs.get_component(&owner.world,root,Script_Component); model,has_model:=ecs.get_component(&owner.world,imported,Scene_Model); mesh,has_mesh:=ecs.get_component(&owner.world,child,Scene_Mesh); parent,has_parent:=ecs.get_component(&owner.world,child,Scene_Parent)
    testing.expect(t,has_source && source.root==.File && source.path==script_file && has_model && model.source.root==.File && len(model.model.primitives)>0 && has_mesh && mesh.source.root==.File && mesh.source.path==mesh_file && has_parent && parent.entity==root)
    saved_file:=strings.concatenate({directory,"/templates/saved.katprefab"}); defer delete(saved_file)
    captured,capture_group:=asset_authoring_execute(&owner,{action=.Capture,path=saved_file,root_entity=root}); defer editor.tool_result_destroy(&captured); defer editor.undo_group_destroy(&capture_group); testing.expect_value(t,captured.error,editor.Scene_Error.None)
    read,read_group:=asset_authoring_execute(&owner,{action=.Read,path=saved_file}); defer editor.tool_result_destroy(&read); defer editor.undo_group_destroy(&read_group); testing.expect(t,read.error==.None && owner.world.live_count==5)
    editor.agent_record_action(&owner.agent.session,{kind=.Application,tool_name="asset_drop"},&result,&group)
    testing.expect(t,os.write_entire_file(model_file,"invalid model")==nil && os.write_entire_file(mesh_file,"invalid mesh")==nil)
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None); testing.expect(t,owner.world.live_count==1 && ecs.entity_exists(&owner.world,before))
    testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None); testing.expect(t,owner.world.live_count==5 && !ecs.entity_exists(&owner.world,root) && !ecs.entity_exists(&owner.world,imported) && !ecs.entity_exists(&owner.world,child))
    restored_root,restored_child:ecs.Entity_Id; found_model:=false
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    for entity in ids {
        if value,present:=ecs.get_component(&owner.world,entity,Script_Component); present { restored_root=entity; testing.expect(t,value.path==script_file) }
        if value,present:=ecs.get_component(&owner.world,entity,Scene_Mesh); present { restored_child=entity; testing.expect(t,len(value.geometry.vertices)>0 && value.source.path==mesh_file) }
        if value,present:=ecs.get_component(&owner.world,entity,Scene_Model); present { found_model=true; testing.expect(t,len(value.model.primitives)>0 && value.source.path==model_file) }
    }
    restored_parent,present:=ecs.get_component(&owner.world,restored_child,Scene_Parent); testing.expect(t,found_model && present && restored_parent.entity==restored_root)
}
