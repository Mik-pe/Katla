package app

import "core:testing"
import "core:mem"
import "core:strings"
import ecs "../ecs"
import km "../math"

@(private="file")
make_test_animation_model :: proc()->Animation_Model {
    return {clips=make([]Animation_Clip,2),bind_pose=make([]km.Transform,1),parents=make([]i32,1)}
}
@(private="package")
attach_test_animation :: proc(app:^Authoring)->ecs.Entity_Id {
    model:=make_test_animation_model(); model.bind_pose[0]=km.TRANSFORM_IDENTITY; model.parents[0]=-1
    model.clips[0]={name=strings.clone("Walk"),duration=0.1}; model.clips[1]={name=strings.clone("Run"),duration=0.2}
    id:=ecs.create_entity(&app.world); ecs.add_component(&app.world,id,model); return id
}
@(test)
test_animation_fade_zero_speed_completion_and_rejection :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app); animation_register(&app.world,&app.registry)
    id:=attach_test_animation(&app); testing.expect(t,animation_play(&app.world,id,"Walk",0,false,1)==.None)
    testing.expect(t,animation_play(&app.world,id,"Run",1,false,1)==.None)
    animation_update(&app.world,0.25); player:=ecs.get_component_mut(&app.world,id,Animation_Player); testing.expect(t,player.blend_weight==0.75 && player.target_time==0.2 && player.blending)
    old:=player^; testing.expect(t,animation_play(&app.world,id,"Walk",0.5,true,1)==.Invalid_Operation); testing.expect(t,player.blend_time==old.blend_time && player.target_clip==old.target_clip)
    for dt in ([3]f32{-1,f32(transmute(f32)u32(0x7f800000)),f32(transmute(f32)u32(0x7fc00000))}) { animation_update(&app.world,dt) }; testing.expect(t,player.blend_weight==0.75)
    animation_update(&app.world,0.75); testing.expect(t,player.clip=="Run" && !player.blending && !player.playing && player.completed && player.time==0.2)
    events:=animation_take_events(player); defer animation_events_destroy(&events); testing.expect(t,len(events)==2)
    testing.expect(t,animation_play(&app.world,id,"Walk",0,true,0)==.None); testing.expect(t,animation_play(&app.world,id,"Run",0.5,false,0)==.None)
    animation_update(&app.world,0.5); testing.expect(t,player.clip=="Run" && player.time==0 && !player.blending && player.playing)
}
@(test)
test_animation_multiple_loops_constant_duration_and_owned_snapshot :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    app:Authoring; authoring_init(&app); animation_register(&app.world,&app.registry); id:=attach_test_animation(&app)
    model:=ecs.get_component_mut(&app.world,id,Animation_Model); model.clips[1].duration=0.25
    testing.expect(t,animation_play(&app.world,id,"Run",0,true,1)==.None); animation_update(&app.world,1)
    player:=ecs.get_component_mut(&app.world,id,Animation_Player); testing.expect(t,player.loop_count==4 && player.time==0)
    snapshot,err:=scene_snapshot_capture(&app); testing.expect(t,err==.None)
    testing.expect(t,scene_snapshot_restore(&app,&snapshot)==.None); scene_snapshot_destroy(&snapshot); testing.expect(t,!ecs.entity_exists(&app.world,id))
    ids:=ecs.entity_ids(&app.world); restored:=ecs.get_component_mut(&app.world,ids[0],Animation_Player); testing.expect(t,restored!=nil && restored.clip=="Run" && len(restored.events)==1); delete(ids)
    authoring_destroy(&app); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_animation_pose_interpolation_skinning_and_crossfade :: proc(t:^testing.T) {
    model:=make_test_animation_model(); defer animation_model_destroy(&model)
    model.bind_pose[0]=km.TRANSFORM_IDENTITY; model.parents[0]=-1
    channel:=Animation_Channel{node=0,path=.Translation,interpolation=.Linear,times=make([]f32,2),values=make([][4]f32,2)}; channel.times[0]=0; channel.times[1]=1; channel.values[0]={0,0,0,0}; channel.values[1]={2,4,6,0}
    model.clips[0]={name=strings.clone("Move"),duration=1,channels=make([]Animation_Channel,1)}; model.clips[0].channels[0]=channel
    model.clips[1]={name=strings.clone("Rest"),duration=1}
    player:=animation_player_stopped(); player.clip="Move"; player.time=0.5
    pose,err:=animation_sample_pose(&model,&player); testing.expect(t,err==.None); defer delete(pose); testing.expect(t,pose[0].position==km.Vec3{1,2,3})
    skin,skin_err:=animation_skin_matrices(&model,pose); testing.expect(t,skin_err==.None); defer delete(skin); testing.expect(t,km.transform_point(skin[0],km.VEC3_ZERO)==km.Vec3{1,2,3})
    player.blending=true; player.target_clip="Rest"; player.blend_weight=0.25
    blended,blend_err:=animation_sample_pose(&model,&player); testing.expect(t,blend_err==.None); defer delete(blended); testing.expect(t,blended[0].position==km.Vec3{0.25,0.5,0.75})
    model.parents[0]=0; invalid,invalid_err:=animation_sample_pose(&model,&player); testing.expect(t,invalid==nil && invalid_err==.Invalid_Operation)
}

@(test)
test_animation_cubic_tangent_scale_and_invalid_sample_rejection :: proc(t:^testing.T) {
    model:=make_test_animation_model(); defer animation_model_destroy(&model)
    model.bind_pose[0]=km.TRANSFORM_IDENTITY; model.parents[0]=-1
    model.clips[0]={name=strings.clone("Cubic"),duration=2,channels=make([]Animation_Channel,1)}; model.clips[1]={name=strings.clone("Rest"),duration=2}
    channel:=&model.clips[0].channels[0]; channel^={path=.Translation,interpolation=.Cubic_Spline,times=make([]f32,2),values=make([][4]f32,6)}
    channel.times[1]=2; channel.values[2]={4,0,0,0}; channel.values[4]={2,0,0,0}
    player:=animation_player_stopped(); player.clip="Cubic"; player.time=1
    pose,err:=animation_sample_pose(&model,&player); testing.expect(t,err==.None && pose[0].position.x==2); delete(pose)
    player.time=transmute(f32)u32(0x7fc00000); invalid,invalid_err:=animation_sample_pose(&model,&player); testing.expect(t,invalid==nil && invalid_err==.Invalid_Operation)
    player.time=1; player.blending=true; player.target_clip="Rest"; player.blend_weight=1.5; invalid,invalid_err=animation_sample_pose(&model,&player); testing.expect(t,invalid==nil && invalid_err==.Invalid_Operation)
    player.blending=false; channel.path=.Rotation; channel.values[2]={}; channel.values[1]={0,0,0,1}; channel.values[4]={0,0,0,-1}
    testing.expect(t,animation_model_valid(&model)); invalid,invalid_err=animation_sample_pose(&model,&player); testing.expect(t,invalid==nil && invalid_err==.Invalid_Operation)
    bad_pose:=[]km.Transform{km.TRANSFORM_IDENTITY}; bad_pose[0].position.x=transmute(f32)u32(0x7f800000); skin,skin_err:=animation_skin_matrices(&model,bad_pose); testing.expect(t,skin==nil && skin_err==.Invalid_Operation)
}

@(test)
test_animation_arbitrary_morph_weights_cubic_and_bind_crossfade :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    model:=make_test_animation_model(); model.bind_pose[0]=km.TRANSFORM_IDENTITY; model.parents[0]=-1
    model.clips[0]={name=strings.clone("Morph"),duration=2,channels=make([]Animation_Channel,1)}; model.clips[1]={name=strings.clone("Rest"),duration=2}
    channel:=&model.clips[0].channels[0]; channel^={path=.Weights,interpolation=.Cubic_Spline,times=make([]f32,2),weight_count=5,weight_values=make([]f32,30)}; channel.times[1]=2
    for i in 0..<5 { channel.weight_values[10+i]=f32(i+1)*4; channel.weight_values[20+i]=f32(i+1)*2 }
    player:=animation_player_stopped(); player.clip="Morph"; player.time=1
    bind:=[5]f32{}; weights,err:=animation_sample_weights(&model,&player,0,bind[:]); testing.expect(t,err==.None && len(weights)==5); for weight,i in weights { testing.expect(t,weight==f32(i+1)*2) }; delete(weights)
    player.blending=true; player.target_clip="Rest"; player.blend_weight=0.25; weights,err=animation_sample_weights(&model,&player,0,bind[:]); testing.expect(t,err==.None); for weight,i in weights { testing.expect(t,weight==f32(i+1)*0.5) }; delete(weights)
    pose,pose_error:=animation_sample_pose(&model,&player); testing.expect(t,pose_error==.None && pose[0]==km.TRANSFORM_IDENTITY); delete(pose)
    channel.weight_values[0]=transmute(f32)u32(0x7fc00000); weights,err=animation_sample_weights(&model,&player,0,bind[:]); testing.expect(t,weights==nil && err==.Invalid_Operation)
    animation_model_destroy(&model); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
