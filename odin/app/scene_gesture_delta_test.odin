#+test
package app
import "core:testing"
import "core:mem"
import "core:strings"
import ecs "../ecs"
import editor "../editor"
import km "../math"

@(private="file")
gesture_delta_position :: proc(position:f32)->editor.Scene_Op {
    value:=`{"position":[1,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}`
    if position==2 { value=`{"position":[2,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}` }
    if position==3 { value=`{"position":[3,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}` }
    return {kind=.Set_Field,component="SceneTransform",field="local",value=transmute([]byte)value}
}
@(test)
test_scene_gesture_animation_preview_clocks_events_and_unrelated_component_survive_history :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    entity:=attach_test_animation(&owner); ecs.add_component(&owner.world,entity,Scene_Transform{km.TRANSFORM_IDENTITY})
    testing.expect_value(t,animation_play(&owner.world,entity,"Walk",0,true,1),editor.Scene_Error.None)
    gesture:Scene_Gesture
    testing.expect_value(t,scene_gesture_begin(&owner,&gesture,{entity}),editor.Scene_Error.None)
    testing.expect_value(t,animation_editor_step(&owner,.06),editor.Scene_Error.None)
    testing.expect_value(t,scene_gesture_preview(&owner,&gesture,gesture_delta_position(1)),editor.Scene_Error.None)
    testing.expect_value(t,animation_editor_step(&owner,.06),editor.Scene_Error.None)
    testing.expect_value(t,scene_gesture_preview(&owner,&gesture,gesture_delta_position(2)),editor.Scene_Error.None)
    testing.expect_value(t,scene_gesture_finish(&owner,&gesture),editor.Scene_Error.None)
    testing.expect_value(t,len(owner.agent.session.actions),1)
    animation_editor_step(&owner,.03)
    player:=ecs.get_component_mut(&owner.world,entity,Animation_Player); time,loops,count:=player.time,player.loop_count,len(player.events)
    testing.expect(t,time>0 && loops==1 && count==1)
    ecs.add_component(&owner.world,entity,Scene_Name{strings.clone("Unrelated live name")})
    testing.expect_value(t,authoring_undo_last(&owner),editor.Scene_Error.None)
    player=ecs.get_component_mut(&owner.world,entity,Animation_Player)
    testing.expect(t,player.time==time && player.loop_count==loops && len(player.events)==count && player.events[0].kind==.Looped)
    testing.expect(t,ecs.get_component_mut(&owner.world,entity,Scene_Transform).local.position==km.VEC3_ZERO && ecs.get_component_mut(&owner.world,entity,Scene_Name).name=="Unrelated live name")
    animation_editor_step(&owner,.1); player=ecs.get_component_mut(&owner.world,entity,Animation_Player); time,loops,count=player.time,player.loop_count,len(player.events)
    testing.expect_value(t,authoring_redo_last(&owner),editor.Scene_Error.None)
    player=ecs.get_component_mut(&owner.world,entity,Animation_Player)
    testing.expect(t,player.time==time && player.loop_count==loops && len(player.events)==count && ecs.get_component_mut(&owner.world,entity,Scene_Transform).local.position==km.Vec3{2,0,0})
    testing.expect_value(t,scene_gesture_begin(&owner,&gesture,{entity}),editor.Scene_Error.None)
    animation_editor_step(&owner,.06)
    testing.expect_value(t,scene_gesture_preview(&owner,&gesture,gesture_delta_position(3)),editor.Scene_Error.None)
    animation_editor_step(&owner,.06); player=ecs.get_component_mut(&owner.world,entity,Animation_Player); time,loops,count=player.time,player.loop_count,len(player.events)
    testing.expect_value(t,scene_gesture_cancel(&owner,&gesture),editor.Scene_Error.None)
    player=ecs.get_component_mut(&owner.world,entity,Animation_Player)
    testing.expect(t,player.time==time && player.loop_count==loops && len(player.events)==count && ecs.get_component_mut(&owner.world,entity,Scene_Transform).local.position==km.Vec3{2,0,0})
    scene_gesture_destroy(&gesture); authoring_destroy(&owner)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(test)
test_scene_gesture_edited_component_conflicts_reject_without_clobbering :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    entity:=attach_test_animation(&owner); ecs.add_component(&owner.world,entity,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&owner.world,entity,Scene_Name{strings.clone("Before")})
    animation_play(&owner.world,entity,"Walk",0,true,1)
    gesture:Scene_Gesture; defer scene_gesture_destroy(&gesture)
    testing.expect_value(t,scene_gesture_begin(&owner,&gesture,{entity}),editor.Scene_Error.None)
    ecs.get_component_mut(&owner.world,entity,Scene_Transform).local.position={9,0,0}
    testing.expect_value(t,scene_gesture_preview(&owner,&gesture,gesture_delta_position(1)),editor.Scene_Error.Invalid_Operation)
    testing.expect(t,ecs.get_component_mut(&owner.world,entity,Scene_Transform).local.position==km.Vec3{9,0,0})
    scene_gesture_destroy(&gesture)
    testing.expect_value(t,scene_gesture_begin(&owner,&gesture,{entity}),editor.Scene_Error.None)
    testing.expect_value(t,scene_gesture_preview(&owner,&gesture,gesture_delta_position(1)),editor.Scene_Error.None)
    ecs.add_component(&owner.world,entity,Scene_Name{strings.clone("External name")})
    name_op:=editor.Scene_Op{kind=.Set_Field,component="SceneName",field="name",value=transmute([]byte)string(`"Gesture name"`)}
    testing.expect_value(t,scene_gesture_preview(&owner,&gesture,name_op),editor.Scene_Error.Invalid_Operation)
    testing.expect(t,ecs.get_component_mut(&owner.world,entity,Scene_Name).name=="External name")
    animation_editor_step(&owner,.06)
    ecs.get_component_mut(&owner.world,entity,Scene_Transform).local.position={8,0,0}
    testing.expect_value(t,scene_gesture_preview(&owner,&gesture,gesture_delta_position(2)),editor.Scene_Error.Invalid_Operation)
    testing.expect_value(t,scene_gesture_finish(&owner,&gesture),editor.Scene_Error.Invalid_Operation)
    testing.expect_value(t,scene_gesture_cancel(&owner,&gesture),editor.Scene_Error.Invalid_Operation)
    testing.expect(t,gesture.active && len(owner.agent.session.actions)==0 && ecs.get_component_mut(&owner.world,entity,Scene_Transform).local.position==km.Vec3{8,0,0} && ecs.get_component_mut(&owner.world,entity,Animation_Player).time==.06)
}

@(private="file")
Gesture_Delta_Admission :: struct { reject:bool,commits:int }
@(private="file")
gesture_delta_prepare :: proc(state:rawptr,owner:^Authoring,ids:[]ecs.Entity_Id,mode:Scene_Preparation_Mode)->(rawptr,editor.Scene_Error) {
    admission:=cast(^Gesture_Delta_Admission)state
    if admission.reject { return nil,.Invalid_Operation }; return admission,.None
}
@(private="file")
gesture_delta_finish :: proc(state,token:rawptr,accepted:bool) { if accepted { (cast(^Gesture_Delta_Admission)state).commits+=1 } }

@(test)
test_scene_history_delta_native_rejection_and_creation_remap_preserve_live_animation :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); testing.expect_value(t,authoring_services_init(&owner),editor.Scene_Error.None)
    target:=ecs.create_entity(&owner.world); ecs.add_component(&owner.world,target,Scene_Transform{km.TRANSFORM_IDENTITY})
    entity:=attach_test_animation(&owner); ecs.add_component(&owner.world,entity,Scene_Transform{km.TRANSFORM_IDENTITY}); ecs.add_component(&owner.world,entity,Scene_Parent{target}); animation_play(&owner.world,entity,"Walk",0,true,1)
    command:=scene_action_command_new(&owner)
    before:=editor.entity_components_capture(&owner.world,&owner.registry,entity)
    proposal:=scene_action_proposal_clone(&owner,before[:]); ecs.remove_component(&owner.world,proposal,Scene_Parent)
    after:=editor.entity_components_capture(&owner.world,&owner.registry,proposal); ecs.destroy_entity(&owner.world,proposal)
    append(&command.rows,Scene_Action_Row{entity,true,true,before,after})
    append(&command.rows,Scene_Action_Row{entity=target,before_exists=true,before=editor.entity_components_capture(&owner.world,&owner.registry,target)})
    group:=scene_action_command_group(command)
    admission:=Gesture_Delta_Admission{reject=true}; ecs.insert_resource(&owner.world,Scene_Participant{&admission,gesture_delta_prepare,gesture_delta_finish})
    animation_editor_step(&owner,.12); player:=ecs.get_component_mut(&owner.world,entity,Animation_Player); time,count:=player.time,len(player.events)
    testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.Invalid_Operation)
    player=ecs.get_component_mut(&owner.world,entity,Animation_Player)
    testing.expect(t,player.time==time && len(player.events)==count && ecs.entity_exists(&owner.world,target) && ecs.get_component_mut(&owner.world,entity,Scene_Parent).entity==target && admission.commits==0)
    admission.reject=false; testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.None)
    testing.expect(t,!ecs.entity_exists(&owner.world,target) && ecs.get_component_mut(&owner.world,entity,Scene_Parent)==nil)
    animation_editor_step(&owner,.06); player=ecs.get_component_mut(&owner.world,entity,Animation_Player); time,count=player.time,len(player.events)
    admission.reject=true; testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.Invalid_Operation)
    testing.expect(t,!ecs.entity_exists(&owner.world,target) && ecs.get_component_mut(&owner.world,entity,Scene_Parent)==nil && owner.world.live_count==1)
    admission.reject=false; testing.expect_value(t,editor.undo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.None)
    parent:=ecs.get_component_mut(&owner.world,entity,Scene_Parent); player=ecs.get_component_mut(&owner.world,entity,Animation_Player)
    testing.expect(t,parent!=nil && parent.entity!=target && ecs.entity_exists(&owner.world,parent.entity) && player.time==time && len(player.events)==count)
    animation_editor_step(&owner,.1); player=ecs.get_component_mut(&owner.world,entity,Animation_Player); time,count=player.time,len(player.events)
    testing.expect_value(t,editor.redo_group(&owner.world,&owner.registry,&group),editor.Scene_Error.None)
    player=ecs.get_component_mut(&owner.world,entity,Animation_Player); testing.expect(t,player.time==time && len(player.events)==count && owner.world.live_count==1)
    editor.undo_group_destroy(&group); authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
