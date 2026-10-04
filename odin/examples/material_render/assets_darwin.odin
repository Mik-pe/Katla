#+build darwin
//! File and prefab acceptance joins actual retained assets to native publication and rollback.
package main

import app "../../app"
import render "../../app/render"
import asset "../../agent/assets"
import ecs "../../ecs"
import editor "../../editor"
import gfx "../../gfx"
import resources "../../resources"
import "core:fmt"
import "core:mem"
import "core:os"
import "core:strings"

@(private="file")
asset_document_pixels :: proc(consumer:^render.Native_Consumer($R),capture:Capture_Ops(R),frame:render.Frame_Data)->gfx.Readback_Data {
    submission,error:=render_frame(consumer.active,frame,consumer.batch.objects,consumer.batch.draws); assert(error=={})
    pixels:=read_pixels(consumer.active,capture,submission); assert(render.native_scene_wait(consumer.active,submission)==.None); return pixels
}
@(private="file")
asset_document_same_pixels :: proc(consumer:^render.Native_Consumer($R),capture:Capture_Ops(R),frame:render.Frame_Data,expected:^gfx.Readback_Data) {
    actual:=asset_document_pixels(consumer,capture,frame); defer gfx.readback_data_destroy(&actual)
    assert(mem.compare(expected.bytes,actual.bytes)==0,"asset transaction changed preserved native pixels")
}
/// Proves real file and subtree publications using the existing world's canonical CPU and GPU consumers.
exercise_asset_documents :: proc(consumer:^render.Native_Consumer($R),owner:^app.Authoring,capture:Capture_Ops(R),frame:render.Frame_Data,resource_path:string) {
    directory,dir_error:=os.make_directory_temp("","katla-native-assets-*",context.allocator); assert(dir_error==nil); defer { os.remove_all(directory); delete(directory) }
    roots:=ecs.get_resource_mut(&owner.world,app.Asset_Roots); assert(roots!=nil)
    project_path:=strings.clone(roots.project.path); actual_resource_path:=strings.clone(roots.resource.path); defer delete(project_path); defer delete(actual_resource_path)
    fixture_root,fixture_root_error:=resources.root_open(resource_path); assert(fixture_root_error==.None); defer resources.root_destroy(&fixture_root)
    fixture,fixture_error:=resources.read_bytes(&fixture_root,"meshes/chair-frame.katmesh"); assert(fixture_error==.None); defer delete(fixture)
    temporary_resources:=strings.concatenate({directory,"/resources"}); defer delete(temporary_resources); assert(os.make_directory(temporary_resources)==nil)
    temporary_meshes:=strings.concatenate({temporary_resources,"/meshes"}); defer delete(temporary_meshes); assert(os.make_directory(temporary_meshes)==nil)
    fixture_copy:=strings.concatenate({temporary_meshes,"/chair-frame.katmesh"}); defer delete(fixture_copy); assert(os.write_entire_file(fixture_copy,fixture)==nil)
    assert(app.asset_resources_init(owner,directory,temporary_resources)==resources.Error.None)
    defer { ecs.remove_resource(&owner.world,app.Scene_File_State); assert(app.asset_resources_init(owner,project_path,actual_resource_path)==resources.Error.None) }
    baseline:=asset_document_pixels(consumer,capture,frame); defer gfx.readback_data_destroy(&baseline)
    old_ids:=ecs.entity_ids(&owner.world); defer delete(old_ids); assert(len(old_ids)==3)
    saved,save_history:=app.scene_file_execute(owner,{action=.Save,path="native.katla",has_path=true}); defer editor.tool_result_destroy(&saved); defer editor.undo_group_destroy(&save_history)
    assert(saved.error==.None && strings.contains(string(saved.data),`"published":true`))
    state,_:=ecs.get_resource(&owner.world,app.Scene_File_State); assert(state.path=="native.katla")
    identity,_:=ecs.get_resource(&owner.world,app.Scene_Identity)
    previous_scene:=consumer.active; real_upload:=consumer.operations.create_buffer; consumer.operations.create_buffer=fail_upload
    failed_load,failed_load_history:=app.scene_file_execute(owner,{action=.Load,path="native.katla",has_path=true}); defer editor.tool_result_destroy(&failed_load); defer editor.undo_group_destroy(&failed_load_history)
    after_identity,_:=ecs.get_resource(&owner.world,app.Scene_Identity); after_state,_:=ecs.get_resource(&owner.world,app.Scene_File_State)
    assert(failed_load.error!=.None && consumer.last_error.gpu==.Allocation_Failed && consumer.active==previous_scene && owner.world.live_count==3 && identity==after_identity && after_state.path==state.path)
    for id in old_ids { assert(ecs.entity_exists(&owner.world,id)) }
    consumer.operations.create_buffer=real_upload; asset_document_same_pixels(consumer,capture,frame,&baseline)
    loaded,load_history:=app.scene_file_execute(owner,{action=.Load,path="native.katla",has_path=true}); defer editor.tool_result_destroy(&loaded); defer editor.undo_group_destroy(&load_history)
    assert(loaded.error==.None && len(loaded.entities)==3 && consumer.active!=previous_scene && owner.world.live_count==3)
    for id in old_ids { assert(!ecs.entity_exists(&owner.world,id)) }; asset_document_same_pixels(consumer,capture,frame,&baseline)
    chair:ecs.Entity_Id; found_chair:=false
    for id in loaded.entities { mesh,present:=ecs.get_component(&owner.world,id,app.Scene_Mesh); if present && mesh.source.kind==.Recipe { chair=id; found_chair=true; break } }; assert(found_chair)
    captured,capture_history:=app.asset_authoring_execute(owner,asset.Prefab_Request{action=.Capture,path="native.katprefab",root_entity=chair}); defer editor.tool_result_destroy(&captured); defer editor.undo_group_destroy(&capture_history)
    assert(captured.error==.None && strings.contains(string(captured.data),`"published":true`) && owner.world.live_count==3); asset_document_same_pixels(consumer,capture,frame,&baseline)
    request:=asset.Prefab_Request{action=.Instantiate,path="native.katprefab",position={0,-0.05,0.5},rotation={0,0,0,1},scale={0.5,0.5,0.5}}
    previous_scene=consumer.active; consumer.operations.create_buffer=fail_upload
    rejected,rejected_history:=app.asset_authoring_execute(owner,request); defer editor.tool_result_destroy(&rejected); defer editor.undo_group_destroy(&rejected_history)
    assert(rejected.error!=.None && consumer.active==previous_scene && owner.world.live_count==3 && consumer.last_error.gpu==.Allocation_Failed)
    consumer.operations.create_buffer=real_upload; asset_document_same_pixels(consumer,capture,frame,&baseline)
    inserted,insert_history:=app.asset_authoring_execute(owner,request); defer editor.tool_result_destroy(&inserted); defer editor.undo_group_destroy(&insert_history)
    assert(inserted.error==.None && len(inserted.entities)==1 && consumer.active!=previous_scene && owner.world.live_count==4)
    changed:=asset_document_pixels(consumer,capture,frame); defer gfx.readback_data_destroy(&changed); assert(mem.compare(baseline.bytes,changed.bytes)!=0,"actual prefab geometry did not reach native drawing")
    inserted_root:=inserted.entities[0]; previous_scene=consumer.active; consumer.operations.create_buffer=fail_upload
    rejected_remove,rejected_remove_history:=app.asset_authoring_execute(owner,{action=.Remove,root_entity=inserted_root}); defer editor.tool_result_destroy(&rejected_remove); defer editor.undo_group_destroy(&rejected_remove_history)
    assert(rejected_remove.error!=.None && consumer.active==previous_scene && owner.world.live_count==4 && ecs.entity_exists(&owner.world,inserted_root) && consumer.last_error.gpu==.Allocation_Failed)
    consumer.operations.create_buffer=real_upload; asset_document_same_pixels(consumer,capture,frame,&changed)
    removed,remove_history:=app.asset_authoring_execute(owner,{action=.Remove,root_entity=inserted_root}); defer editor.tool_result_destroy(&removed); defer editor.undo_group_destroy(&remove_history)
    assert(removed.error==.None && consumer.active!=previous_scene && owner.world.live_count==3 && !ecs.entity_exists(&owner.world,inserted_root)); asset_document_same_pixels(consumer,capture,frame,&baseline)
    assert(editor.undo_group(&owner.world,&owner.registry,&remove_history)==.None && render.native_consumer_refresh(consumer)=={}); asset_document_same_pixels(consumer,capture,frame,&changed)
    assert(editor.redo_group(&owner.world,&owner.registry,&remove_history)==.None && render.native_consumer_refresh(consumer)=={}); asset_document_same_pixels(consumer,capture,frame,&baseline)
    current,_:=ecs.get_resource(&owner.world,app.Scene_File_State); assert(current.path=="native.katla")
    fmt.println("Native assets: exact-pixel load failure rollback, fresh-ID load, actual captured chair insertion/removal failure rollback, undo and redo PASS")
}
