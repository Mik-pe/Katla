#+test
package luau
import "core:testing"
import "core:strings"
import "core:thread"

LIBRARY :: #config(LUAU_LIBRARY,"")
@(private="file")
callback_add :: proc "c"(state:State,opaque:rawptr)->i32 {
    vm:=cast(^VM)opaque; valid:i32
    value:=vm.api.number(state,1,&valid); if valid==0 { push_string(vm,"number expected"); return -1 }
    vm.api.push_number(state,value+5); return 1
}
@(private="file")
native_luau_typed_source_callback_sandbox_and_allocator :: proc(t:^testing.T) {
    vm:VM; testing.expect_value(t,init(&vm,LIBRARY),Error.None); if vm.state==nil { return }; defer testing.expect_value(t,destroy(&vm),Error.None)
    vm.api.callback(vm.state,callback_add,&vm,"add"); vm.api.set_field(vm.state,GLOBALS_INDEX,"native_add"); vm.api.sandbox(vm.state)
    source:=`local sum:number = 0
        for i=1,10 do sum += i end
        assert(native_add(sum) == 60)
        assert(require == nil and debug == nil and io == nil and package == nil)
        assert(os.execute == nil and os.getenv == nil)
        local okay = pcall(function() math.pi = 0 end)
        assert(not okay)
        return sum`
    testing.expect_value(t,vm.api.compile_load(vm.state,raw_data(source),uint(len(source)),"native-proof.luau",0),i32(0))
    testing.expect_value(t,vm.api.run(vm.state,0,1),i32(0)); numeric:i32; testing.expect(t,vm.api.number(vm.state,-1,&numeric)==55 && numeric!=0); pop(&vm)
    bad:=`function missing(`; testing.expect(t,vm.api.compile_load(vm.state,raw_data(bad),uint(len(bad)),"invalid.luau",0)!=0); testing.expect(t,len(to_string(&vm,-1))>0); pop(&vm)
    throwing:=`native_add("bad")`; testing.expect_value(t,vm.api.compile_load(vm.state,raw_data(throwing),uint(len(throwing)),"error.luau",0),i32(0)); testing.expect(t,vm.api.run(vm.state,0,0)!=0 && strings.contains(to_string(&vm,-1),"number expected")); pop(&vm)
    testing.expect(t,vm.api.bytes(vm.state)>0)
    state:=struct {vm:^VM,error:Error,native_check,native_destroy:i32}{vm=&vm}
    worker:=thread.create_and_start_with_poly_data(&state,proc(p:^struct {vm:^VM,error:Error,native_check,native_destroy:i32}) { p.error=owner_error(p.vm); p.native_check=p.vm.api.owner_check(p.vm.state); p.native_destroy=p.vm.api.destroy(p.vm.state) })
    thread.join(worker); thread.destroy(worker)
    testing.expect(t,state.error==.Wrong_Thread && state.native_check==0 && state.native_destroy==0)
    testing.expect_value(t,owner_error(&vm),Error.None)
}
when LIBRARY!="" {
@(test)
test_native_luau_typed_source_callback_sandbox_and_allocator :: proc(t:^testing.T) { native_luau_typed_source_callback_sandbox_and_allocator(t) }
}
