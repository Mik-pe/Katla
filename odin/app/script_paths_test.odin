#+test
package app
import "core:testing"
import "core:mem"
import "core:os"
import "core:strings"
import ecs "../ecs"
import editor "../editor"
import script "../script"
import km "../math"

@(test)
test_script_source_names_and_explicit_file_capability_rollback :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-script-paths-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_path:=strings.concatenate({directory,"/resources"}); defer delete(resource_path)
    scripts_path:=strings.concatenate({resource_path,"/scripts"}); defer delete(scripts_path)
    testing.expect(t,os.make_directory(resource_path)==nil && os.make_directory(scripts_path)==nil)
    source_path:=strings.concatenate({scripts_path,"/player.luau"}); defer delete(source_path); testing.expect(t,os.write_entire_file(source_path,"speed=2")==nil)
    file_path:=strings.concatenate({directory,"/outside.luau"}); defer delete(file_path); testing.expect(t,os.write_entire_file(file_path,"external=true")==nil)
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner); testing.expect(t,asset_resources_init(&owner,directory,resource_path)==.None)
    for path in ([4]string{"player","player.luau","scripts/player.luau","player.lua"}) {
        resolved,resolve_error:=script_source_resolve(&owner,path); testing.expect(t,resolve_error==.None && resolved.path=="scripts/player.luau" && resolved.root==.Resource); delete(resolved.path)
    }
    absolute,absolute_error:=script_source_resolve(&owner,source_path); testing.expect(t,absolute_error==.None && absolute.root==.Resource && absolute.path=="scripts/player.luau"); delete(absolute.path)
    escaped,escape_error:=script_source_resolve(&owner,"../outside"); testing.expect(t,escape_error==.Invalid_Field_Value); delete(escaped.path)
    unauthorized,unauthorized_error:=script_source_read(&owner,{path=file_path,root=.File}); testing.expect(t,unauthorized_error==.Invalid_Field_Value); delete(unauthorized)
    snapshot:=Scene_Snapshot{entities=make([dynamic]Scene_Entity,owner.world.allocator),allocator=owner.world.allocator}
    row:=Scene_Entity{key=1,components=make([dynamic]Scene_Component,owner.world.allocator)}
    testing.expect(t,scene_row_component(&owner,&row,"Script",Script_Component{path=file_path,root=.File})==.None); append(&snapshot.entities,row)
    token,prepare_error:=script_sources_prepare(&owner,&snapshot); testing.expect(t,prepare_error==.None)
    retained,retained_error:=script_source_read(&owner,{path=file_path,root=.File}); testing.expect(t,retained_error==.None && string(retained)=="external=true"); delete(retained)
    script_sources_finish(&owner,&token,false)
    revoked,revoked_error:=script_source_read(&owner,{path=file_path,root=.File}); testing.expect(t,revoked_error==.Invalid_Field_Value); delete(revoked)
    committed,commit_error:=script_sources_prepare(&owner,&snapshot); testing.expect(t,commit_error==.None); script_sources_finish(&owner,&committed,true)
    scene_snapshot_destroy(&snapshot)
    persisted,persisted_error:=script_source_read(&owner,{path=file_path,root=.File}); testing.expect(t,persisted_error==.None && string(persisted)=="external=true"); delete(persisted)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(private="file")
native_normalized_script_attachment_and_file_reload :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-script-source-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_path:=strings.concatenate({directory,"/resources"}); defer delete(resource_path)
    scripts_path:=strings.concatenate({resource_path,"/scripts"}); defer delete(scripts_path)
    testing.expect(t,os.make_directory(resource_path)==nil && os.make_directory(scripts_path)==nil)
    source_path:=strings.concatenate({scripts_path,"/player.luau"}); defer delete(source_path); testing.expect(t,os.write_entire_file(source_path,"speed=2")==nil)
    outside:=strings.concatenate({directory,"/external.luau"}); defer delete(outside); testing.expect(t,os.write_entire_file(outside,"speed=3")==nil)
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); register_test_scene_runtime(&owner); testing.expect(t,asset_resources_init(&owner,directory,resource_path)==.None && script_native_init(&owner,LUAU_APP_LIBRARY)==.None)
    entity:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,entity,Scene_Transform{km.TRANSFORM_IDENTITY})
    result,undo:=behavior_execute(&owner,{action=.Set_Script,entity=entity,path="player"}); testing.expect(t,result.error==.None); editor.tool_result_destroy(&result); editor.undo_group_destroy(&undo)
    attached:=ecs.get_component_mut(&owner.world,entity,Script_Component); testing.expect(t,attached!=nil && attached.path=="scripts/player.luau")
    testing.expect(t,script_native_sync(&owner)==.None)
    snapshot:=Scene_Snapshot{allocator=owner.world.allocator}; token,token_error:=script_sources_prepare(&owner,&snapshot); testing.expect(t,token_error==.None && script_sources_admit(&owner,&token,outside)==.None); script_sources_finish(&owner,&token,true)
    replacement,replacement_undo:=behavior_execute(&owner,{action=.Set_Script,entity=entity,path=outside}); testing.expect(t,replacement.error==.None); editor.tool_result_destroy(&replacement); editor.undo_group_destroy(&replacement_undo)
    testing.expect(t,script_reload(&owner,entity)==.None)
    native:=ecs.get_resource_mut(&owner.world,Script_Native_Runtime); old,present:=script.handle(native.runtime,u64(entity)); testing.expect(t,present)
    message,check_error:=script_source_check(&owner,"function invalid(",outside); testing.expect(t,check_error==.Invalid_Operation && message!=""); delete(message)
    testing.expect(t,os.write_entire_file(outside,"function invalid(")==nil && script_reload(&owner,entity)==.Invalid_Operation)
    retained,retained_present:=script.handle(native.runtime,u64(entity)); testing.expect(t,retained_present && retained==old)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when LUAU_APP_LIBRARY!="" {
    @(test)
    test_script_normalized_attachment_and_file_reload_preserves_old_instance :: proc(t:^testing.T) { native_normalized_script_attachment_and_file_reload(t) }
}
