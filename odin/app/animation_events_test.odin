package app
import ecs "../ecs"
import editor "../editor"
import "core:testing"
import "core:mem"

@(test)
test_animation_preview_long_running_events_are_owned_and_bounded :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); animation_register(&owner.world,&owner.registry)
    entity:=attach_test_animation(&owner); testing.expect(t,animation_play(&owner.world,entity,"Walk",0,true,1)==.None)
    for _ in 0..<12000 {
        testing.expect(t,animation_editor_step(&owner,.1)==.None && animation_events_dispatch(&owner)==.None)
        player:=ecs.get_component_mut(&owner.world,entity,Animation_Player); testing.expect(t,len(player.events)==0)
    }
    runtime:=ecs.get_resource_mut(&owner.world,Animation_Event_Runtime)
    testing.expect(t,runtime.delivered==12000 && len(runtime.feedback)==4096 && runtime.retired==7904)
    feedback:=animation_feedback_drain(&owner)
    testing.expect(t,len(feedback.events)==4096 && feedback.retired==7904 && feedback.events[0].event.loop_count==7905 && feedback.events[4095].event.loop_count==12000)
    ecs.destroy_entity(&owner.world,entity)
    testing.expect(t,feedback.events[0].entity==entity && feedback.events[0].event.clip=="Walk")
    animation_feedback_destroy(&feedback,owner.world.allocator)
    entity=attach_test_animation(&owner); animation_play(&owner.world,entity,"Walk",0,true,1)
    for count in 1..=12000 {
        animation_editor_step(&owner,.1); animation_events_dispatch(&owner)
        packet:=animation_feedback_drain(&owner)
        testing.expect(t,packet.retired==0 && len(packet.events)==1 && packet.events[0].event.loop_count==u32(count))
        animation_feedback_destroy(&packet,owner.world.allocator)
    }
    testing.expect(t,runtime.delivered==24000 && len(runtime.feedback)==0 && runtime.bytes==0)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_animation_feedback_completion_order_single_delivery_and_control_history :: proc(t:^testing.T) {
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); animation_register(&owner.world,&owner.registry)
    entity:=attach_test_animation(&owner)
    animation_play(&owner.world,entity,"Walk",0,false,1); animation_play(&owner.world,entity,"Run",1,false,1)
    animation_editor_step(&owner,.25)
    player:=ecs.get_component_mut(&owner.world,entity,Animation_Player); testing.expect(t,len(player.events)==2)
    result,undo:=animation_execute(&owner,{action=.Speed,entity=entity,speed=2}); defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&undo)
    testing.expect(t,editor.undo_group(&owner.world,&owner.registry,&undo)==.None && len(player.events)==2)
    testing.expect(t,editor.redo_group(&owner.world,&owner.registry,&undo)==.None && len(player.events)==2)
    testing.expect(t,animation_events_dispatch(&owner)==.None && len(player.events)==0)
    packet:=animation_feedback_drain(&owner); defer animation_feedback_destroy(&packet,owner.world.allocator)
    testing.expect(t,len(packet.events)==2 && packet.events[0].event.clip=="Walk" && packet.events[1].event.clip=="Run")
    for notice in packet.events { testing.expect(t,notice.entity==entity && notice.event.kind==.Completed) }
    animation_editor_step(&owner,1); animation_events_dispatch(&owner); animation_events_dispatch(&owner)
    empty:=animation_feedback_drain(&owner); defer animation_feedback_destroy(&empty,owner.world.allocator); testing.expect(t,len(empty.events)==0)
}
