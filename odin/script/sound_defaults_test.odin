#+test
package script
import "core:testing"
import "core:mem"
import luau "../deps/luau"
import km "../math"

@(private="file")
native_sound_defaults_and_strict_optional_arguments :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Runtime; testing.expect_value(t,init(&owner,LIBRARY),luau.Error.None)
    source:=`function on_spawn(entity, world)
        world:play_sound("default.ogg")
        world:play_sound("nil.ogg",nil,nil)
        world:play_sound("volume.ogg",.4)
        world:play_sound("loop.ogg",nil,true)
        local position = Vec3.new(1,2,3)
        world:play_sound_at("position.ogg",position)
        world:play_sound_at("position-nil.ogg",position,nil,nil)
        world:play_sound_at("position-volume.ogg",position,.25,false)
        world:play_sound_at("position-loop.ogg",position,nil,true)
        for _,invalid in {false,"0.5",{},function() end} do
            assert(not pcall(function() world:play_sound("bad.ogg",invalid) end))
            assert(not pcall(function() world:play_sound_at("bad.ogg",position,invalid) end))
        end
        for _,invalid in {0,"false",{},function() end} do
            assert(not pcall(function() world:play_sound("bad.ogg",nil,invalid) end))
            assert(not pcall(function() world:play_sound_at("bad.ogg",position,nil,invalid) end))
        end
        assert(not pcall(function() world:play_sound("bad.ogg",-1) end))
        assert(not pcall(function() world:play_sound("bad.ogg",2) end))
        assert(not pcall(function() world:play_sound("bad.ogg",0/0) end))
        assert(not pcall(function() world:play_sound("bad.ogg",math.huge) end))
    end`
    diagnostics,failure:=sync(&owner,{Attachment{1,"sound-defaults.luau",source}}); testing.expect(t,failure=="" && len(diagnostics)==0); delete(failure); for value in diagnostics { delete(value.path); delete(value.error) }; delete(diagnostics)
    entities:=[1]Entity_State{{id=1,transform=km.TRANSFORM_IDENTITY}}
    output,tick_error:=tick(&owner,0,entities[:]); testing.expect(t,tick_error=="" && len(output.diagnostics)==0 && len(output.commands)==8); delete(tick_error)
    expected:=[8]f32{1,1,.4,1,1,1,.25,1}
    for command,index in output.commands {
        testing.expect(t,index<len(expected)); if index>=len(expected) {continue}
        testing.expect(t,command.volume==expected[index] && command.looping==(index==3 || index==7))
        testing.expect(t,command.kind==(.Play_Sound if index<4 else .Play_Sound_At))
        if index>=4 {testing.expect(t,command.origin==km.Vec3{1,2,3})}
    }
    output_destroy(&owner,&output); testing.expect_value(t,destroy(&owner),luau.Error.None)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when LIBRARY!="" {
@(test)
test_native_sound_defaults_and_strict_optional_arguments :: proc(t:^testing.T) { native_sound_defaults_and_strict_optional_arguments(t) }
}
