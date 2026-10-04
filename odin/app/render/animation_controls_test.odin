#+test
package render
import app ".."
import scene "../../agent/scene"
import ecs "../../ecs"
import editor "../../editor"
import km "../../math"
import resources "../../resources"
import "core:testing"
import "core:mem"
import "core:slice"

@(private="file")
model_timeline_command_test :: proc(owner:^app.Authoring,op:scene.Animation_Op)->editor.Scene_Error {
    result,undo:=app.animation_execute(owner,op);defer editor.tool_result_destroy(&result);defer editor.undo_group_destroy(&undo);return result.error
}
@(test)
test_actual_fox_timeline_seek_pause_speed_and_stop_drive_owned_skin_batch :: proc(t:^testing.T) {
    root,root_error:=resources.root_open(#config(GLTF_RESOURCE_ROOT,"resources"));testing.expect_value(t,root_error,resources.Error.None);if root_error!=.None {return};defer resources.root_destroy(&root)
    model,load_error:=app.gltf_load(&root,"models/Fox.glb");testing.expect_value(t,load_error,app.Gltf_Error.None);if load_error!=.None {return}
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner);app.scene_model_register(&owner);app.animation_register(&owner.world,&owner.registry)
    entity:=ecs.spawn(&owner.world,struct {model:app.Scene_Model,transform:app.Scene_Transform}{{model=model},{km.TRANSFORM_IDENTITY}})
    entry:=owner.registry.entries["AnimationModel"]
    cloned:=editor.editor_clone_value(entry,&model.animation,owner.world.allocator)
    testing.expect(t,ecs.insert_component_value(&owner.world,entity,app.Animation_Model,cloned));mem.free(cloned,owner.world.allocator)
    clip:=model.animation.clips[0];testing.expect(t,clip.duration>0)
    testing.expect_value(t,model_timeline_command_test(&owner,{action=.Play,entity=entity,clip=clip.name,speed=1,looping=true}),editor.Scene_Error.None)
    batch,error:=model_batch_prepare(&owner);testing.expect_value(t,error,Model_Batch_Error{});if error.kind!=.None {return};defer model_batch_destroy(&batch)
    initial:=slice.clone(batch.vertices);defer delete(initial)
    revision:=batch.geometry_revision
    testing.expect_value(t,model_timeline_command_test(&owner,{action=.Seek,entity=entity,time_seconds=clip.duration*.5}),editor.Scene_Error.None)
    testing.expect_value(t,model_batch_refresh(&batch,&owner),Model_Batch_Error{})
    changed:=false;for vertex,i in batch.vertices {if vertex.position!=initial[i].position {changed=true;break}}
    testing.expect(t,changed&&batch.geometry_changed&&batch.geometry_revision==revision+1)
    testing.expect_value(t,model_timeline_command_test(&owner,{action=.Pause,entity=entity}),editor.Scene_Error.None)
    app.animation_editor_step(&owner,.1);testing.expect_value(t,model_batch_refresh(&batch,&owner),Model_Batch_Error{})
    testing.expect(t,!batch.geometry_changed&&batch.geometry_revision==revision+1)
    testing.expect_value(t,model_timeline_command_test(&owner,{action=.Stop,entity=entity}),editor.Scene_Error.None)
    testing.expect_value(t,model_batch_refresh(&batch,&owner),Model_Batch_Error{})
    for vertex,i in batch.vertices {testing.expect_value(t,vertex.position,initial[i].position)}
    testing.expect_value(t,model_timeline_command_test(&owner,{action=.Speed,entity=entity,speed=0}),editor.Scene_Error.None)
    testing.expect_value(t,model_timeline_command_test(&owner,{action=.Resume,entity=entity}),editor.Scene_Error.None)
    app.animation_editor_step(&owner,.1);testing.expect_value(t,model_batch_refresh(&batch,&owner),Model_Batch_Error{});testing.expect(t,!batch.geometry_changed)
    testing.expect_value(t,model_timeline_command_test(&owner,{action=.Speed,entity=entity,speed=2}),editor.Scene_Error.None)
    app.animation_editor_step(&owner,.1);testing.expect_value(t,model_batch_refresh(&batch,&owner),Model_Batch_Error{})
    player:=ecs.get_component_mut(&owner.world,entity,app.Animation_Player);testing.expect(t,player.time==.2&&batch.geometry_changed)
}
