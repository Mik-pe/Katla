#+test
package script
import "core:testing"
import "core:mem"
import "core:strings"
import luau "../deps/luau"

@(private="file")
native_inspect_reload_logs_and_budget :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    owner:Runtime; testing.expect_value(t,init(&owner,LIBRARY),luau.Error.None)
    source:=`speed=2; label="before"; active=true
        function on_spawn(entity,world)
            assert(type(getmetatable(entity)) == "string")
            assert(type(getmetatable(Vec3.new(1,2,3))) == "string")
            assert(type(getmetatable(world)) == "string")
            print("spawn",entity:id()); warn("warning")
        end
        function on_update(entity,world,dt) speed+=1 end`
    diagnostics,error:=sync(&owner,{Attachment{1,"inspect.luau",source}}); testing.expect(t,error==""); delete(error); for entry in diagnostics { delete(entry.path); delete(entry.error) }; delete(diagnostics)
    entities:=[1]Entity_State{{id=1}}
    first,failure:=tick(&owner,0,entities[:]); testing.expect(t,failure=="" && len(first.diagnostics)==0 && len(first.logs)==2 && first.logs[0].message=="spawn\t1" && first.logs[1].level==.Warn); delete(failure); output_destroy(&owner,&first)
    current,exists:=handle(&owner,1); testing.expect(t,exists)
    values,inspect_error:=inspect(&owner,current); testing.expect(t,inspect_error=="" && len(values)==3); delete(inspect_error); variables_destroy(values)
    set_error:=set_variable(&owner,current,"speed",f64(27)); testing.expect(t,set_error==""); delete(set_error)
    changed:=strings.concatenate({source,"\n--changed"})
    reload,reload_error:=sync(&owner,{Attachment{1,"inspect.luau",changed}}); testing.expect(t,reload_error==""); delete(reload_error); for entry in reload { delete(entry.path); delete(entry.error) }; delete(reload)
    _,stale_error:=inspect(&owner,current); testing.expect(t,stale_error!=""); delete(stale_error)
    replacement,_:=handle(&owner,1); preserved,preserved_error:=inspect(&owner,replacement); testing.expect(t,preserved_error==""); delete(preserved_error)
    for value in preserved { if value.name=="speed" { numeric,valid:=value.value.(f64); testing.expect(t,valid && numeric==27) } }; variables_destroy(preserved)
    budget_error:=validate(&owner,"while true do end","infinite.luau"); testing.expect(t,budget_error!=""); delete(budget_error)
    recovered:=validate(&owner,"local x: number=4; assert(x==4)","recovered.luau"); testing.expect(t,recovered==""); delete(recovered)
    delete(changed); testing.expect_value(t,destroy(&owner),luau.Error.None); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
when LIBRARY!="" {
@(test)
test_native_inspect_reload_logs_and_budget :: proc(t:^testing.T) { native_inspect_reload_logs_and_budget(t) }
}
