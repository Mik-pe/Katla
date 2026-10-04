#+test
#+build darwin, linux
package document

import app ".."
import ecs "../../ecs"
import editor "../../editor"
import km "../../math"
import resources "../../resources"
import "core:testing"
import "core:strings"
import "core:os"

@(test)
test_document_real_file_save_discard_cancel_overwrite_baseline_and_play_gating :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-document-*",context.allocator); testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_path:=strings.concatenate({directory,"/resources"}); defer delete(resource_path); testing.expect_value(t,os.make_directory(resource_path),os.Error(nil))
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); testing.expect_value(t,app.authoring_services_init(&owner),editor.Scene_Error.None); testing.expect_value(t,app.asset_resources_init(&owner,directory,resource_path),resources.Error.None)
    state:State; testing.expect_value(t,init(&state,&owner),editor.Scene_Error.None); defer destroy(&state); testing.expect(t,!dirty(&state))
    entity:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform}{{local=km.TRANSFORM_IDENTITY}}); testing.expect(t,dirty(&state))
    testing.expect_value(t,request(&state,{kind=.New}),editor.Scene_Error.None); testing.expect(t,state.dialog==.Unsaved && state.has_pending && ecs.entity_exists(&owner.world,entity))
    testing.expect_value(t,respond(&state,.Cancel),editor.Scene_Error.None); testing.expect(t,state.dialog==.None && !state.has_pending && ecs.entity_exists(&owner.world,entity))
    testing.expect_value(t,request(&state,{kind=.New}),editor.Scene_Error.None); testing.expect_value(t,respond(&state,.Save),editor.Scene_Error.None); testing.expect(t,state.dialog==.Save_As && state.has_pending)
    testing.expect_value(t,submit_path(&state,"  saved.katla  "),editor.Scene_Error.None); testing.expect(t,state.dialog==.None && !state.has_pending && owner.world.live_count==0 && !dirty(&state) && !ecs.contains_resource(&owner.world,app.Scene_File_State))
    testing.expect_value(t,request(&state,{kind=.Open,path="saved.katla"}),editor.Scene_Error.None); testing.expect(t,owner.world.live_count==1 && !dirty(&state))
    ids:=ecs.entity_ids(&owner.world); defer delete(ids); current:=ids[0]; ecs.get_component_mut(&owner.world,current,app.Scene_Transform).local.position={1,2,3}; testing.expect(t,dirty(&state))
    testing.expect_value(t,save(&state,"missing/no.katla"),editor.Scene_Error.Invalid_Operation); origin,_:=ecs.get_resource(&owner.world,app.Scene_File_State); testing.expect(t,origin.path=="saved.katla" && dirty(&state) && ecs.entity_exists(&owner.world,current))
    testing.expect_value(t,choose_path(&state,true),editor.Scene_Error.None); testing.expect_value(t,submit_path(&state,"saved.katla"),editor.Scene_Error.None); testing.expect(t,state.dialog==.Overwrite && dirty(&state))
    testing.expect_value(t,respond(&state,.Cancel),editor.Scene_Error.None); testing.expect(t,dirty(&state))
    testing.expect_value(t,choose_path(&state,true),editor.Scene_Error.None); testing.expect_value(t,submit_path(&state,"saved.katla"),editor.Scene_Error.None); testing.expect_value(t,respond(&state,.Overwrite),editor.Scene_Error.None); testing.expect(t,!dirty(&state))
    ecs.get_component_mut(&owner.world,current,app.Scene_Transform).local.position={4,5,6}
    testing.expect_value(t,request(&state,{kind=.Open,path="saved.katla"}),editor.Scene_Error.None); testing.expect(t,state.dialog==.Unsaved)
    testing.expect_value(t,respond(&state,.Discard),editor.Scene_Error.None); testing.expect(t,!ecs.entity_exists(&owner.world,current) && owner.world.live_count==1 && !dirty(&state))
    owner.mode=.Playing; testing.expect_value(t,request(&state,{kind=.Quit}),editor.Scene_Error.Editing_Required); testing.expect_value(t,save(&state),editor.Scene_Error.Editing_Required); testing.expect(t,!state.quit_requested && owner.world.live_count==1); owner.mode=.Editing
    direct,direct_history:=app.scene_file_execute(&owner,{action=.Save,path="agent.katla",has_path=true}); defer editor.tool_result_destroy(&direct); defer editor.undo_group_destroy(&direct_history); testing.expect(t,direct.error==.None && !dirty(&state))
}

@(test)
test_document_animation_progress_is_clean_and_replacement_preflight_failure_preserves_baseline :: proc(t:^testing.T) {
    directory,dir_error:=os.make_directory_temp("","katla-document-animation-*",context.allocator); testing.expect(t,dir_error==nil); if dir_error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource_path:=strings.concatenate({directory,"/resources"}); defer delete(resource_path); testing.expect_value(t,os.make_directory(resource_path),os.Error(nil))
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); testing.expect_value(t,app.authoring_services_init(&owner),editor.Scene_Error.None); testing.expect_value(t,app.asset_resources_init(&owner,directory,resource_path),resources.Error.None)
    entity:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,animation:app.Animation_Player}{{local=km.TRANSFORM_IDENTITY},app.animation_player_stopped()})
    state:State; testing.expect_value(t,init(&state,&owner),editor.Scene_Error.None); defer destroy(&state)
    testing.expect_value(t,save(&state,"animation.katla"),editor.Scene_Error.None); testing.expect(t,!dirty(&state))
    animation:=ecs.get_component_mut(&owner.world,entity,app.Animation_Player); animation.time=3; animation.duration=7; animation.target_time=2; animation.blend_time=1; animation.blend_weight=0.5; animation.loop_count=4; animation.completed=true; testing.expect(t,!dirty(&state))
    animation.speed=2; testing.expect(t,dirty(&state)); animation.speed=1; testing.expect(t,!dirty(&state))
    previous:=ecs.get_resource_mut(&owner.world,app.Scene_File_Observer); callback:=previous.prepare; previous.prepare=proc(state:rawptr,owner:^app.Authoring,snapshot:^app.Scene_Snapshot)->(rawptr,editor.Scene_Error) { return nil,.Invalid_Field_Value }
    testing.expect_value(t,request(&state,{kind=.New}),editor.Scene_Error.Invalid_Field_Value); testing.expect(t,state.dialog==.Error && owner.world.live_count==1 && ecs.entity_exists(&owner.world,entity) && !dirty(&state))
    failed,failed_history:=app.scene_file_execute(&owner,{action=.Save,path="rejected.katla",has_path=true}); defer editor.tool_result_destroy(&failed); defer editor.undo_group_destroy(&failed_history); testing.expect_value(t,failed.error,editor.Scene_Error.Invalid_Field_Value)
    origin,_:=ecs.get_resource(&owner.world,app.Scene_File_State); testing.expect(t,origin.path=="animation.katla" && !dirty(&state)); previous.prepare=callback
    roots:=ecs.get_resource_mut(&owner.world,app.Asset_Roots); missing,error:=resources.read_bytes(&roots.project,"rejected.katla"); defer delete(missing); testing.expect_value(t,error,resources.Error.IO)
}
