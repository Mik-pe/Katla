#+test
package script
import "core:testing"
import "core:mem"
import km "../math"
import luau "../deps/luau"

LIBRARY :: #config(LUAU_LIBRARY,"")
@(private="file")
free_diagnostics :: proc(values:[dynamic]Diagnostic) { for value in values { delete(value.path); delete(value.error) }; delete(values) }
@(private="file")
native_owned_luau_math_lifecycle_full_u64_events_and_world_commands :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Runtime; testing.expect_value(t,init(&owner,LIBRARY),luau.Error.None)
    source:=`local spawned = 0
        local ray_index
        function on_spawn(entity, world)
            spawned += 1
            assert(entity:id() == "18446744073709551615")
            assert(entity == world:find_entity("Actor"))
            assert(typeof(entity) == "userdata" and typeof(Vec3.new(1,2,3)) == "userdata")
            local v = Vec3.new(1,2,3); v.x = 2
            assert(v:length_squared() == 17 and v:dot(Vec3.new(1,0,0)) == 2)
            assert((v * Vec3.new(2,3,4)).z == 12 and (-v).y == -2)
            assert(Vec3.new(1,0,0):cross(Vec3.new(0,1,0)).z == 1)
            local q = Quat.from_axis_angle(Vec3.new(0,1,0), math.pi/2)
            assert(math.abs((q * Vec3.new(1,0,0)).z + 1) < .0001)
            assert(not pcall(function() q.x = 3 end))
            local c = Color.rgb(.1,.2,.3); c.a = .5
            assert(math.abs(c:with_alpha(.7).a - .7) < .0001)
            assert(Color.from_rgb_hex(0xff0000).r == 1)
            local transform = world:get_transform(entity)
            assert(typeof(transform) == "userdata" and transform.scale.x == 1)
            transform.position = Vec3.new(4,5,6)
            world:set_transform(entity, transform)
            assert(world:entity_exists(entity) and #world:get_all_with("Transform") == 1)
            local dx,dy = world:get_mouse_delta(); assert(dx == 2 and dy == 3 and world:get_mouse_wheel() == 1)
            assert(world:is_action_pressed("move_forward") and world:is_key_pressed("W"))
            assert(world:get_velocity(entity).x == 1)
            world:on_event("activated",function(name,data,current)
                assert(data.trigger == entity and data.other == entity)
                current:set_particles_active(entity,true)
                current:burst_particles(entity,32)
                current:emit("follow",{value=17,entity=entity})
            end)
            world:on_event("follow",function(name,data,current)
                assert(data.value == 17 and data.entity == entity)
                current:apply_force(entity,Vec3.new(1,0,0))
            end)
            ray_index = world:raycast(Vec3.new(0,0,0),Vec3.new(0,0,-2),10)
            world:set_velocity(entity,Vec3.new(2,0,0))
            world:apply_impulse(entity,Vec3.new(1,0,0))
            world:play_sound("a.wav",.5,false)
            world:play_sound_at("b.wav",Vec3.new(1,2,3),.5,true)
            world:play_sound_cue("jump")
        end
        function on_update(entity, world, dt)
            assert(spawned == 1 and dt == .25)
            if ray_index then
                local result = world:get_raycast_result(ray_index)
                if result then assert(result.entity == entity and result.distance == 4) end
            end
        end`
    diagnostics,error:=sync(&owner,{Attachment{max(u64),"full.luau",source}}); testing.expect(t,error=="" && len(diagnostics)==0); delete(error); free_diagnostics(diagnostics)
    entities:=[1]Entity_State{ {id=max(u64),name="Actor",transform=km.TRANSFORM_IDENTITY,velocity={1,0,0},has_velocity=true,components={"Transform"}} }
    output,tick_error:=tick(&owner,.25,entities[:],{Event{"activated",max(u64),max(u64),-1}},Input{actions={"move_forward"},keys={"W"},mouse_delta={2,3},mouse_wheel=1})
    testing.expect(t,tick_error=="" && len(output.diagnostics)==0 && len(output.commands)==10 && output.instances[0].consecutive_errors==0); delete(tick_error)
    ray_index:=-1; for command in output.commands { if command.kind==.Raycast { ray_index=command.index; testing.expect(t,command.vector==km.Vec3{0,0,-1}) } }; output_destroy(&owner,&output)
    queries:=Query_Results{rays=make(map[Query_Key]Ray_Result)}; queries.rays[{max(u64),ray_index}]={hit=true,entity=max(u64),distance=4}
    second,second_error:=tick(&owner,.25,entities[:],queries=queries)
    testing.expect(t,second_error=="" && len(second.diagnostics)==0 && len(second.commands)==1 && second.commands[0].kind==.Apply_Force); delete(second_error); output_destroy(&owner,&second)
    resetting:=reset(&owner); free_diagnostics(resetting); testing.expect_value(t,destroy(&owner),luau.Error.None)
    delete(queries.rays); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(private="file")
native_owned_luau_atomic_reload_error_disable_and_retained_proxy_expiry :: proc(t:^testing.T) {
    owner:Runtime; testing.expect_value(t,init(&owner,LIBRARY),luau.Error.None); defer testing.expect_value(t,destroy(&owner),luau.Error.None)
    source:=`local old
        function on_spawn(entity,world) old=world end
        function on_update(entity,world,dt)
            world:burst_particles(entity,3)
            if dt == 1 then error("failed hook") end
            if dt == 2 then old:burst_particles(entity,4) end
        end`
    diagnostics,error:=sync(&owner,{Attachment{1,"error.luau",source}}); testing.expect(t,error==""); delete(error); free_diagnostics(diagnostics)
    entities:=[1]Entity_State{{id=1,transform=km.TRANSFORM_IDENTITY}}
    first,first_error:=tick(&owner,0,entities[:]); testing.expect(t,first_error=="" && len(first.commands)==1); delete(first_error); output_destroy(&owner,&first)
    original:=owner.instances[1].environment
    bad_diagnostics,bad_error:=sync(&owner,{Attachment{1,"error.luau","function broken("}})
    testing.expect(t,bad_error!="" && owner.instances[1].environment==original); delete(bad_error); free_diagnostics(bad_diagnostics)
    stale,stale_error:=tick(&owner,2,entities[:]); testing.expect(t,stale_error=="" && len(stale.commands)==0 && len(stale.diagnostics)==1); delete(stale_error); output_destroy(&owner,&stale)
    for _ in 0..<9 { failed,failure:=tick(&owner,1,entities[:]); testing.expect(t,failure=="" && len(failed.commands)==0); delete(failure); output_destroy(&owner,&failed) }
    testing.expect(t,owner.instances[1].disabled && owner.instances[1].errors==10 && len(owner.instances[1].subscriptions)==0)
    removed,removed_error:=tick(&owner,0,nil); testing.expect(t,removed_error=="" && len(owner.instances)==0); delete(removed_error); output_destroy(&owner,&removed)
}
when LIBRARY!="" {
@(test)
test_native_owned_luau_math_lifecycle_full_u64_events_and_world_commands :: proc(t:^testing.T) { native_owned_luau_math_lifecycle_full_u64_events_and_world_commands(t) }
@(test)
test_native_owned_luau_atomic_reload_error_disable_and_retained_proxy_expiry :: proc(t:^testing.T) { native_owned_luau_atomic_reload_error_disable_and_retained_proxy_expiry(t) }
}
