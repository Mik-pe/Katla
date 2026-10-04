#+test
package script
import "core:testing"
import "core:mem"
import "core:strings"
import luau "../deps/luau"

@(private="file")
native_animation_packet_full_u64_owned_payload_and_callback_budget :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Runtime; testing.expect(t,init(&owner,LIBRARY)==.None)
    source:=`local retained
        function on_spawn(entity,world)
            world:on_event("animation_looped",function(name,data,current)
                assert(data.entity:id()=="18446744073709551615" and data.entity_id==data.entity:id())
                assert(data.trigger==data.entity and data.other==data.entity)
                assert(data.clip_name=="Owned Walk" and data.loop_count==4294967295)
                retained=data
                current:burst_particles(entity,17)
                error("event failure")
            end)
        end
        function on_update(entity,world,dt)
            if dt==1 then assert(retained.clip_name=="Owned Walk" and retained.entity_id=="18446744073709551615");print(retained.clip_name) end
        end`
    diagnostics,failure:=sync(&owner,{Attachment{max(u64),"animation.luau",source}}); testing.expect(t,failure=="" && len(diagnostics)==0); delete(failure); for diagnostic in diagnostics { delete(diagnostic.path); delete(diagnostic.error) }; delete(diagnostics)
    clip:=strings.clone("Owned Walk")
    events:=[1]Event{{name="animation_looped",trigger=max(u64),other=max(u64),payload=-1,has_animation=true,animation_clip=clip,animation_loop_count=max(u32)}}
    entities:=[1]Entity_State{{id=max(u64)}}
    first,error:=tick(&owner,0,entities[:],events[:]); delete(clip)
    testing.expect(t,error=="" && len(first.diagnostics)==1 && len(first.commands)==0 && first.instances[0].consecutive_errors==1); delete(error); output_destroy(&owner,&first)
    second,error2:=tick(&owner,1,entities[:]); testing.expect(t,error2=="" && len(second.diagnostics)==0 && second.instances[0].consecutive_errors==0 && len(second.logs)==1 && second.logs[0].message=="Owned Walk"); delete(error2); output_destroy(&owner,&second)
    testing.expect_value(t,destroy(&owner),luau.Error.None); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when LIBRARY!="" {
@(test)
test_native_animation_packet_full_u64_owned_payload_and_callback_budget :: proc(t:^testing.T) { native_animation_packet_full_u64_owned_payload_and_callback_budget(t) }
}
