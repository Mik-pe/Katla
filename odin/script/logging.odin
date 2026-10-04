//! Script log packets are owned and bounded before the host console consumes them.
package script
import luau "../deps/luau"
import "core:strings"

@(private="file")
print_callback :: proc "c"(state:luau.State,opaque:rawptr)->i32 {
    binding:=cast(^Binding)opaque; owner:=binding.owner; context=owner.ctx; vm:=&owner.vm
    count:=vm.api.get_top(state); builder:strings.Builder; strings.builder_init(&builder,owner.allocator); defer strings.builder_destroy(&builder)
    if len(owner.logs)>=4096 { return binding_error(owner,"Script log queue is full") }
    for i in i32(1)..=count {
        vm.api.get_field(state,luau.GLOBALS_INDEX,"tostring"); vm.api.push_value(state,i)
        if vm.api.run(state,1,1)!=0 { return -1 }
        text:=luau.to_string(vm,-1)
        if strings.builder_len(builder)+len(text)+1>8192 { return binding_error(owner,"Script log line exceeds 8192 bytes") }
        if i!=1 { strings.write_string(&builder,"\t") }; strings.write_string(&builder,text); luau.pop(vm)
    }
    level:=Log_Level.Info; if binding.name=="warn" { level=.Warn }
    append(&owner.logs,Log{owner.current_entity,level,strings.clone(strings.to_string(builder),owner.allocator)}); return 0
}
@(private="package")
register_logging :: proc(owner:^Runtime) {
    vm:=&owner.vm; vm.api.push_value(vm.state,luau.GLOBALS_INDEX); bind(owner,"print",print_callback,"print"); bind(owner,"warn",print_callback,"warn"); luau.pop(vm)
}
