#+test
package app
import ecs "../ecs"
import editor "../editor"
import resources "../resources"
import script "../script"
import "core:testing"
import "core:mem"
import "core:os"
import "core:strings"

@(private="file")
native_animation_events_reach_luau_and_retry_without_replay :: proc(t:^testing.T) {
    directory,error:=os.make_directory_temp("","katla-animation-events-*",context.allocator); testing.expect(t,error==nil); if error!=nil { return }; defer { os.remove_all(directory); delete(directory) }
    resource:=strings.concatenate({directory,"/resources"}); defer delete(resource); testing.expect(t,os.make_directory(resource)==nil)
    scripts:=strings.concatenate({resource,"/scripts"}); defer delete(scripts); testing.expect(t,os.make_directory(scripts)==nil)
    path:=strings.concatenate({scripts,"/events.luau"}); defer delete(path)
    source:=`loop_events=0; completed_events=0; last_count=0
        function on_spawn(entity,world)
            world:on_event("animation_looped",function(name,data,current)
                assert(data.entity:id()==data.entity_id and data.trigger==data.entity and data.other==data.entity)
                assert(data.clip_name=="Walk" and data.loop_count==last_count+1)
                loop_events+=1; last_count=data.loop_count
            end)
            world:on_event("animation_completed",function(name,data,current)
                assert(data.clip_name=="Run" and data.loop_count==0)
                completed_events+=1
            end)
        end`
    testing.expect(t,os.write_entire_file(path,source)==nil)
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Authoring; authoring_init(&owner); testing.expect(t,authoring_services_init(&owner)==.None)
    testing.expect_value(t,asset_resources_init(&owner,directory,resource),resources.Error.None)
    testing.expect_value(t,script_native_init(&owner,LUAU_APP_LIBRARY),editor.Scene_Error.None)
    entity:=attach_test_animation(&owner); ecs.add_component(&owner.world,entity,Script_Component{path=strings.clone("events.luau")})
    testing.expect(t,physics_select_box3d(&owner,BOX3D_LIBRARY)==.None)
    animation_play(&owner.world,entity,"Walk",0,true,1)
    testing.expect(t,execute_test_simulation(t,&owner,.Play)==.None)
    signals:=ecs.get_resource_mut(&owner.world,Script_Signals)
    for _ in 0..<4096 { append(&signals.pending,Script_Signal{name=strings.clone("unsubscribed"),trigger=entity,other=entity}) }
    testing.expect_value(t,animation_events_preflight(&owner,.1),editor.Scene_Error.Invalid_Operation)
    model:=ecs.get_component_mut(&owner.world,entity,Animation_Model)
    delete(model.clips[0].name); model.clips[0].name=strings.clone("Reloaded Walk")
    testing.expect_value(t,animation_events_preflight(&owner,.1),editor.Scene_Error.Invalid_Operation)
    delete(model.clips[0].name); model.clips[0].name=strings.clone("Walk")
    player:=ecs.get_component_mut(&owner.world,entity,Animation_Player); testing.expect(t,player.time==0 && len(player.events)==0)
    testing.expect(t,simulation_step(&owner,.1)==.None && len(signals.pending)==0 && player.loop_count==1)
    packet0:=animation_feedback_drain(&owner); animation_feedback_destroy(&packet0,owner.world.allocator)
    for _ in 0..<999 {
        testing.expect(t,animation_events_preflight(&owner,.1)==.None)
        animation_update(&owner.world,.1); testing.expect(t,animation_events_dispatch(&owner)==.None && len(player.events)==0 && len(signals.pending)==1)
        packet:=animation_feedback_drain(&owner); animation_feedback_destroy(&packet,owner.world.allocator)
        testing.expect(t,script_native_step(&owner,.1)==.None && len(signals.pending)==0)
    }
    animation_play(&owner.world,entity,"Run",0,false,1); animation_update(&owner.world,.2); animation_events_dispatch(&owner)
    testing.expect(t,len(signals.pending)==1 && signals.pending[0].animation_clip=="Run")
    // A rejected reload leaves the owned packet for the next accepted native tick.
    native:=ecs.get_resource_mut(&owner.world,Script_Native_Runtime)
    testing.expect(t,os.write_entire_file(path,"function broken(")==nil)
    saved_mode:=owner.mode; owner.mode=.Editing
    testing.expect(t,script_native_step(&owner,0)!=.None && len(signals.pending)==1)
    owner.mode=saved_mode; testing.expect(t,os.write_entire_file(path,source)==nil)
    testing.expect(t,script_native_step(&owner,0)==.None && len(signals.pending)==0)
    testing.expect(t,script_native_step(&owner,0)==.None)
    handle,present:=script.handle(native.runtime,u64(entity)); testing.expect(t,present)
    values,failure:=script.inspect(native.runtime,handle); testing.expect(t,failure==""); delete(failure)
    found:=0
    for value in values { if value.name=="loop_events" { count,valid:=value.value.(f64); testing.expect(t,valid && count==1000); found+=1 }; if value.name=="completed_events" { count,valid:=value.value.(f64); testing.expect(t,valid && count==1); found+=1 } }
    testing.expect(t,found==2); script.variables_destroy(values,owner.world.allocator)
    component:=ecs.get_component_mut(&owner.world,entity,Script_Component); testing.expect(t,len(component.last_errors)==0 && !component.disabled)
    authoring_destroy(&owner); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when LUAU_APP_LIBRARY!="" && BOX3D_LIBRARY!="" {
@(test)
test_native_animation_events_reach_luau_and_retry_without_replay :: proc(t:^testing.T) { native_animation_events_reach_luau_and_retry_without_replay(t) }
}
