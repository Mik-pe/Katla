#+test
package script

import "core:testing"
import "core:mem"
import "core:strings"
import luau "../deps/luau"

@(private="file")
native_oom_boundary :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator
    mem.tracking_allocator_init(&tracker,context.allocator)
    defer mem.tracking_allocator_destroy(&tracker)
    context.allocator=mem.tracking_allocator(&tracker)
    owner:Runtime
    testing.expect_value(t,init(&owner,LIBRARY),luau.Error.None)
    source:=`buffers={}
        function on_spawn(entity,world)
            for _,size in {65536,8192,256} do
                for i=1,1000000 do
                    local ok,value=pcall(buffer.create,size)
                    if not ok then break end
                    buffers[#buffers+1]=value
                end
            end
            world:get_all_with("Transform")
        end`
    diagnostics,failure:=sync(&owner,{Attachment{1,"oom.luau",source}})
    testing.expect(t,failure=="")
    delete(failure)
    for entry in diagnostics { delete(entry.path); delete(entry.error) }; delete(diagnostics)
    entities:=make([]Entity_State,4096)
    for &entity,i in entities { entity.id=u64(i+1); entity.components={"Transform"} }
    output,error:=tick(&owner,0,entities)
    testing.expect(t,error=="" && len(output.diagnostics)==1)
    testing.expect(t,len(output.diagnostics)==1 && strings.contains(output.diagnostics[0].error,"memory"))
    testing.expect(t,owner.vm.api.bytes(owner.vm.state)>120*1024*1024)
    delete(error); output_destroy(&owner,&output); delete(entities)
    cleanup:=reset(&owner)
    for entry in cleanup { delete(entry.path); delete(entry.error) }; delete(cleanup)
    owner.vm.api.collect(owner.vm.state,2,0)
    recovered:=validate(&owner,"assert(1+1==2)","recovered.luau")
    testing.expect(t,recovered==""); delete(recovered)
    testing.expect_value(t,destroy(&owner),luau.Error.None)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(private="file")
native_host_environment_error_boundary :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator
    mem.tracking_allocator_init(&tracker,context.allocator)
    defer mem.tracking_allocator_destroy(&tracker)
    context.allocator=mem.tracking_allocator(&tracker)
    owner:Runtime
    testing.expect_value(t,init(&owner,LIBRARY),luau.Error.None)
    failed:=validate(&owner,`local fail=error; setmetatable(getfenv(), {__index=function() fail("host lookup denied") end})`,"metamethod.luau")
    testing.expect(t,strings.contains(failed,"host lookup denied")); delete(failed)
    exhausted:=validate(&owner,`setmetatable(getfenv(), {__index=function() while true do end end})`,"metamethod-loop.luau")
    testing.expect(t,strings.contains(exhausted,"budget exhausted")); delete(exhausted)
    accepted:=validate(&owner,"local value=2; assert(value==2)","recovered.luau")
    testing.expect(t,accepted==""); delete(accepted)
    testing.expect_value(t,destroy(&owner),luau.Error.None)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when LIBRARY!="" {
    @(test)
    test_native_oom_preserves_odin_cleanup_and_recovers :: proc(t:^testing.T) { native_oom_boundary(t) }
    @(test)
    test_native_host_environment_errors_and_budget_recover :: proc(t:^testing.T) { native_host_environment_error_boundary(t) }
}
