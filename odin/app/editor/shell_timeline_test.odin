package editor_app
import app ".."
import ecs "../../ecs"
import editor "../../editor"
import ui "../../ui"
import km "../../math"
import "core:strings"
import "core:testing"

@(private="file")
timeline_ui_frame :: proc(t:^testing.T,shell:^Shell,input:[]ui.Input_Event=nil) {
    shell_frame_destroy(shell)
    descriptor:=shell_timeline(shell)
    _,frame:=ui.frame(shell.ctx,descriptor,{events=input},{680,800})
    testing.expect_value(t,frame.error,ui.Frame_Error.None)
    shell_actions(shell)
}
@(private="file")
timeline_ui_click :: proc(t:^testing.T,shell:^Shell,entity:ecs.Entity_Id,name:string) {
    node:=shell.ctx.nodes[key(110,name,u64(entity))]
    testing.expect(t,node!=nil);if node==nil {return}
    point:=ui.Vec2{node.bounds.x+node.bounds.width*.5,node.bounds.y+node.bounds.height*.5}
    timeline_ui_frame(t,shell,{ui.Pointer_Down{position=point},ui.Pointer_Up{position=point}})
}
@(test)
test_timeline_retained_controls_drive_sampled_pose_history_and_stale_ids :: proc(t:^testing.T) {
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner)
    testing.expect_value(t,app.authoring_services_init(&owner),editor.Scene_Error.None)
    entity:=ecs.create_entity(&owner.world)
    model:=app.Animation_Model{clips=make([]app.Animation_Clip,2),bind_pose=make([]km.Transform,1),parents=make([]i32,1)}
    model.bind_pose[0]=km.TRANSFORM_IDENTITY;model.parents[0]= -1
    for &clip,index in model.clips {
        clip.name=strings.clone("Walk" if index==0 else "Run");clip.duration=1;clip.channels=make([]app.Animation_Channel,1)
        channel:=&clip.channels[0];channel^={path=.Translation,interpolation=.Linear,times=make([]f32,2),values=make([][4]f32,2)};channel.times[1]=1;channel.values[1][index]=2
    }
    ecs.add_component(&owner.world,entity,model)
    state:State;state_init(&state,&owner);defer state_destroy(&state);selection_set(&state,entity)
    ctx:ui.Context;testing.expect(t,ui.context_init(&ctx,{measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme})==.None);defer ui.context_destroy(&ctx)
    shell:Shell;testing.expect(t,shell_init(&shell,&state,&ctx,nil)==.None);defer shell_destroy(&shell)
    timeline_ui_frame(t,&shell);timeline_ui_click(t,&shell,entity,"Play clip")
    player:=ecs.get_component_mut(&owner.world,entity,app.Animation_Player);testing.expect(t,player!=nil&&player.playing&&player.clip=="Walk");if player==nil {return}
    testing.expect_value(t,app.animation_editor_step(&owner,.25),editor.Scene_Error.None)
    timeline_ui_frame(t,&shell);timeline_ui_click(t,&shell,entity,"Pause clip")
    testing.expect(t,!player.playing&&player.time==.25)
    append(&ctx.actions,ui.Number_Action{action=u64(Action.Timeline_Seek),payload=u64(entity),value=.5,started=true,finished=true});shell_actions(&shell)
    actual_model:=ecs.get_component_mut(&owner.world,entity,app.Animation_Model)
    pose,error:=app.animation_sample_pose(actual_model,player);testing.expect(t,error==.None&&pose[0].position==km.Vec3{1,0,0});delete(pose)
    append(&ctx.actions,ui.Number_Action{action=u64(Action.Timeline_Speed),payload=u64(entity),value=2,finished=true});shell_actions(&shell)
    testing.expect_value(t,player.speed,f32(2));testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None);testing.expect(t,player.speed==1&&player.time==.5)
    append(&ctx.actions,ui.Selection_Action{action=u64(Action.Timeline_Fade),payload=u64(entity),index=1});shell_actions(&shell)
    timeline_ui_frame(t,&shell);timeline_ui_click(t,&shell,entity,"Fade to clip")
    testing.expect(t,player.blending&&player.target_clip=="Run")
    timeline_ui_click(t,&shell,entity,"Resume clip");app.animation_editor_step(&owner,.125)
    testing.expect(t,player.blend_time==.125&&abs(player.blend_weight-.5)<1e-6)
    owner.mode=.Paused;clock:=player.time;timeline_ui_frame(t,&shell);timeline_ui_click(t,&shell,entity,"Stop clip");app.animation_editor_step(&owner,.2);testing.expect_value(t,player.time,clock)
    owner.mode=.Editing;timeline_ui_frame(t,&shell);timeline_ui_click(t,&shell,entity,"Stop clip");testing.expect(t,player.time==0&&!player.playing&&!player.blending)
    ecs.destroy_entity(&owner.world,entity)
    replacement:=ecs.create_entity(&owner.world);selection_set(&state,replacement)
    append(&ctx.actions,ui.Click_Action{action=u64(Action.Timeline_Play),payload=u64(entity)});shell_actions(&shell);testing.expect_value(t,state.last_error,editor.Scene_Error.Entity_Not_Found)
}
