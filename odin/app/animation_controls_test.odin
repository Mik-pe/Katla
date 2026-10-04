package app
import scene "../agent/scene"
import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:testing"
import "core:encoding/json"

@(private="file")
execute_animation_control_test :: proc(owner:^Authoring,op:scene.Animation_Op)->editor.Scene_Error {
    result,undo:=animation_execute(owner,op);defer editor.tool_result_destroy(&result);defer editor.undo_group_destroy(&undo);return result.error
}
@(test)
test_animation_timeline_pause_seek_speed_loop_fade_pose_and_events :: proc(t:^testing.T) {
    owner:Authoring;authoring_init(&owner);defer authoring_destroy(&owner);animation_register(&owner.world,&owner.registry)
    entity:=attach_test_animation(&owner);model:=ecs.get_component_mut(&owner.world,entity,Animation_Model)
    for &clip,index in model.clips {
        clip.channels=make([]Animation_Channel,1);channel:=&clip.channels[0]
        channel^={path=.Translation,interpolation=.Linear,times=make([]f32,2),values=make([][4]f32,2)}
        channel.times[1]=clip.duration;channel.values[1][index]=2
    }
    testing.expect_value(t,execute_animation_control_test(&owner,{action=.Play,entity=entity,clip="Walk",speed=1,looping=true}),editor.Scene_Error.None)
    testing.expect_value(t,animation_editor_step(&owner,.05),editor.Scene_Error.None)
    player:=ecs.get_component_mut(&owner.world,entity,Animation_Player)
    pose,error:=animation_sample_pose(model,player);testing.expect(t,error==.None&&pose[0].position==km.Vec3{1,0,0});delete(pose)
    testing.expect_value(t,execute_animation_control_test(&owner,{action=.Pause,entity=entity}),editor.Scene_Error.None)
    animation_editor_step(&owner,.2);testing.expect_value(t,player.time,.05)
    testing.expect_value(t,execute_animation_control_test(&owner,{action=.Resume,entity=entity}),editor.Scene_Error.None)
    testing.expect_value(t,execute_animation_control_test(&owner,{action=.Fade,entity=entity,clip="Run",fade_seconds=.2,speed=0,looping=false}),editor.Scene_Error.None)
    animation_editor_step(&owner,.1);testing.expect(t,player.time==.05&&player.target_time==0&&player.blend_weight==.5)
    execute_animation_control_test(&owner,{action=.Pause,entity=entity});animation_editor_step(&owner,.1)
    testing.expect(t,player.blend_time==.1&&player.time==.05)
    testing.expect_value(t,execute_animation_control_test(&owner,{action=.Seek,entity=entity,time_seconds= -1}),editor.Scene_Error.None)
    testing.expect(t,player.time==0&&player.target_time==0&&player.blend_weight==.5&&!player.playing)
    execute_animation_control_test(&owner,{action=.Seek,entity=entity,time_seconds=100})
    testing.expect(t,player.time==.1&&!player.completed)
    execute_animation_control_test(&owner,{action=.Speed,entity=entity,speed=1})
    execute_animation_control_test(&owner,{action=.Loop,entity=entity,looping=false})
    execute_animation_control_test(&owner,{action=.Resume,entity=entity});animation_editor_step(&owner,.1)
    testing.expect(t,player.clip=="Run"&&!player.blending&&player.time==.1&&player.playing&&!player.looping)
    pose,error=animation_sample_pose(model,player);testing.expect(t,error==.None&&pose[0].position==km.Vec3{0,1,0});delete(pose)
    animation_editor_step(&owner,.1);testing.expect(t,player.completed&&!player.playing&&len(player.events)==2)
    testing.expect_value(t,execute_animation_control_test(&owner,{action=.Stop,entity=entity}),editor.Scene_Error.None)
    testing.expect(t,player.clip=="Run"&&player.time==0&&!player.completed&&!player.blending&&player.target_clip==""&&player.loop_count==0&&len(player.events)==2)
    events:=animation_take_events(player);defer animation_events_destroy(&events)
    testing.expect(t,events[0].clip=="Walk"&&events[1].clip=="Run"&&events[0].kind==.Completed&&events[1].kind==.Completed)
    result,undo:=animation_execute(&owner,{action=.Inspect,entity=entity});defer editor.tool_result_destroy(&result);defer editor.undo_group_destroy(&undo)
    tree,parse:=json.parse(result.data);testing.expect(t,parse==nil);defer json.destroy_value(tree)
    object,_:=tree.(json.Object);clips,_:=object["clips"].(json.Array);first,_:=clips[0].(json.Object)
    name,is_name:=first["name"].(string);testing.expect(t,is_name&&name=="Run")
    playback,_:=object["playback"].(json.Object);completed,is_completed:=playback["completed"].(bool);testing.expect(t,playback["duration_seconds"]!=nil&&playback["loop_count"]!=nil&&is_completed&&!completed)
}
@(test)
test_animation_timeline_failed_controls_preserve_state_and_generations :: proc(t:^testing.T) {
    owner:Authoring;authoring_init(&owner);defer authoring_destroy(&owner);animation_register(&owner.world,&owner.registry)
    entity:=attach_test_animation(&owner)
    execute_animation_control_test(&owner,{action=.Play,entity=entity,clip="Walk",speed=1,looping=true})
    execute_animation_control_test(&owner,{action=.Fade,entity=entity,clip="Run",fade_seconds=.2,speed=1,looping=true});animation_editor_step(&owner,.05)
    player:=ecs.get_component_mut(&owner.world,entity,Animation_Player);before_time,before_target,before_blend:=player.time,player.target_time,player.blend_time
    for op in ([]scene.Animation_Op{
        {action=.Speed,entity=entity,speed= -1},{action=.Speed,entity=entity,speed=transmute(f32)u32(0x7fc00000)},
        {action=.Seek,entity=entity,time_seconds=transmute(f32)u32(0x7f800000)},
        {action=.Fade,entity=entity,clip="Walk",fade_seconds=.1,speed=1},{action=.Play,entity=entity,clip="missing",speed=1},
    }) {
        testing.expect(t,execute_animation_control_test(&owner,op)!=.None)
        testing.expect(t,player.time==before_time&&player.target_time==before_target&&player.blend_time==before_blend&&player.playing&&player.clip=="Walk"&&player.target_clip=="Run"&&player.speed==1)
    }
    ecs.add_component(&owner.world,entity,Editor_Hidden{})
    testing.expect_value(t,execute_animation_control_test(&owner,{action=.Stop,entity=entity}),editor.Scene_Error.Protected_Entity)
    ecs.destroy_entity(&owner.world,entity)
    testing.expect_value(t,execute_animation_control_test(&owner,{action=.Stop,entity=entity}),editor.Scene_Error.Entity_Not_Found)
    empty:=attach_test_animation(&owner)
    testing.expect_value(t,execute_animation_control_test(&owner,{action=.Resume,entity=empty}),editor.Scene_Error.Component_Not_Found)
    ecs.add_component(&owner.world,empty,animation_player_stopped())
    testing.expect_value(t,execute_animation_control_test(&owner,{action=.Resume,entity=empty}),editor.Scene_Error.Invalid_Operation)
}
@(test)
test_animation_editor_preview_play_pause_and_stop_snapshot_contract :: proc(t:^testing.T) {
    owner:Authoring;authoring_init(&owner);defer authoring_destroy(&owner);animation_register(&owner.world,&owner.registry);simulation_init(&owner)
    entity:=attach_test_animation(&owner)
    execute_animation_control_test(&owner,{action=.Play,entity=entity,clip="Run",speed=1,looping=true})
    animation_editor_step(&owner,.05);player:=ecs.get_component_mut(&owner.world,entity,Animation_Player)
    testing.expect_value(t,player.time,.05)
    testing.expect_value(t,execute_test_simulation(t,&owner,.Play),editor.Scene_Error.None)
    animation_editor_step(&owner,.05);testing.expect_value(t,player.time,.05)
    testing.expect_value(t,simulation_step(&owner,.05),editor.Scene_Error.None);testing.expect_value(t,player.time,.1)
    execute_test_simulation(t,&owner,.Pause);animation_editor_step(&owner,.1);simulation_step(&owner,.1);testing.expect_value(t,player.time,.1)
    execute_test_simulation(t,&owner,.Resume);simulation_step(&owner,.05);testing.expect(t,abs(player.time-.15)<1e-6)
    testing.expect_value(t,execute_test_simulation(t,&owner,.Stop),editor.Scene_Error.None)
    testing.expect(t,!ecs.entity_exists(&owner.world,entity))
    ids:=ecs.entity_ids(&owner.world);defer delete(ids)
    found:=false
    for id in ids {if restored:=ecs.get_component_mut(&owner.world,id,Animation_Player);restored!=nil {found=true;testing.expect(t,restored.time==.05&&restored.clip=="Run"&&restored.speed==1&&restored.looping&&restored.playing)}}
    testing.expect(t,found)
}

@(test)
test_animation_authored_speed_loop_history_preserves_live_pose_clocks :: proc(t:^testing.T) {
    owner:Authoring;authoring_init(&owner);defer authoring_destroy(&owner);animation_register(&owner.world,&owner.registry)
    entity:=attach_test_animation(&owner);execute_animation_control_test(&owner,{action=.Play,entity=entity,clip="Run",speed=1,looping=true})
    result,speed_history:=animation_execute(&owner,{action=.Speed,entity=entity,speed=2});defer editor.tool_result_destroy(&result);defer editor.undo_group_destroy(&speed_history)
    testing.expect(t,result.error==.None&&speed_history.state!=nil)
    animation_editor_step(&owner,.025);player:=ecs.get_component_mut(&owner.world,entity,Animation_Player);testing.expect_value(t,player.time,.05)
    testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&speed_history),editor.Scene_Error.None)
    testing.expect(t,player.time==.05&&player.speed==1&&player.looping&&player.playing)
    testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&speed_history),editor.Scene_Error.None);testing.expect(t,player.speed==2&&player.time==.05)
    loop_result,loop_history:=animation_execute(&owner,{action=.Loop,entity=entity,looping=false});defer editor.tool_result_destroy(&loop_result);defer editor.undo_group_destroy(&loop_history)
    testing.expect(t,loop_result.error==.None&&loop_history.state!=nil&&!player.looping)
    testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&loop_history),editor.Scene_Error.None);testing.expect(t,player.looping&&player.time==.05&&player.speed==2)
    ecs.destroy_entity(&owner.world,entity)
    testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&loop_history),editor.Scene_Error.Entity_Not_Found)
}
