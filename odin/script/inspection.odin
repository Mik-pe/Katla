//! Editor variable access resolves a generation before reading or editing an instance environment.
package script
import luau "../deps/luau"
import "core:strings"
import "core:fmt"
import "core:slice"
import "core:math"

Scalar :: union { f64, bool, string }
Variable :: struct { name:string,value:Scalar }
Handle :: struct { entity,serial:u64 }
/// Returns a current instance identity; replacement invalidates every older handle.
handle :: proc(owner:^Runtime,entity:u64)->(Handle,bool) { if luau.owner_error(&owner.vm)!=.None { return {},false }; instance,present:=owner.instances[entity]; if !present { return {},false }; return {entity,instance.serial},true }
/// Frees the names and scalar strings copied for editor use.
variables_destroy :: proc(values:[]Variable,allocator:=context.allocator) { for value in values { delete(value.name,allocator); if text,valid:=value.value.(string); valid { delete(text,allocator) } }; delete(values,allocator) }
@(private="package")
environment_variables :: proc(owner:^Runtime,environment:i32)->([]Variable,string) {
    vm:=&owner.vm; base:=vm.api.get_top(vm.state)
    values,failure:=environment_variables_inner(owner,environment)
    if native:=vm.api.take_error(vm.state); native!=nil { variables_destroy(values,owner.allocator); values=nil; failure=strings.clone(string(native),owner.allocator) }
    vm.api.set_top(vm.state,base); return values,failure
}
@(private="package")
environment_variables_inner :: proc(owner:^Runtime,environment:i32)->(values:[]Variable,failure:string) {
    context.allocator=owner.allocator; vm:=&owner.vm
    result:=make([dynamic]Variable,owner.allocator); defer delete(result)
    luau.get_reference(vm,environment); table:=vm.api.abs_index(vm.state,-1); vm.api.push_nil(vm.state)
    for vm.api.next(vm.state,table)!=0 {
        value_type:=vm.api.type(vm.state,-1); key_type:=vm.api.type(vm.state,-2)
        if (key_type==.String || key_type==.Number) && (value_type==.String || value_type==.Number || value_type==.Boolean) {
            key:string; if key_type==.String { key=strings.clone(luau.to_string(vm,-2)) } else { numeric:i32; key=fmt.aprintf("%g",vm.api.number(vm.state,-2,&numeric)) }
            value:Scalar
            #partial switch value_type {
            case .String: value=strings.clone(luau.to_string(vm,-1))
            case .Number: numeric:i32; value=vm.api.number(vm.state,-1,&numeric)
            case .Boolean: value=vm.api.boolean(vm.state,-1)!=0
            }
            append(&result,Variable{key,value})
        }
        luau.pop(vm)
    }
    slice.sort_by(result[:],proc(a,b:Variable)->bool { return a.name<b.name }); values=make([]Variable,len(result),owner.allocator); copy(values,result[:]); return
}
/// Copies scalar environment state. Functions, userdata and tables stay inside the VM.
inspect :: proc(owner:^Runtime,instance_handle:Handle)->([]Variable,string) {
    if luau.owner_error(&owner.vm)!=.None { return nil,strings.clone("Script runtime thread mismatch",owner.allocator) }
    instance,present:=owner.instances[instance_handle.entity]; if !present || instance.serial!=instance_handle.serial { return nil,strings.clone("Script instance is stale",owner.allocator) }
    return environment_variables(owner,instance.environment)
}
@(private="package")
environment_set :: proc(owner:^Runtime,environment:i32,name:string,value:Scalar)->(failure:string) {
    vm:=&owner.vm; base:=vm.api.get_top(vm.state)
    defer vm.api.set_top(vm.state,base)
    luau.get_reference(vm,environment); key:=strings.clone_to_cstring(name,owner.allocator); defer delete(key,owner.allocator)
    switch scalar in value {
    case f64: vm.api.push_number(vm.state,scalar)
    case bool: vm.api.push_boolean(vm.state,i32(scalar))
    case string: luau.push_string(vm,scalar)
    }
    vm.api.raw_set_field(vm.state,-2,key)
    if native:=vm.api.take_error(vm.state); native!=nil { return strings.clone(string(native),owner.allocator) }; return ""
}
/// Edits one scalar only after generation, name and value admission succeeds.
set_variable :: proc(owner:^Runtime,instance_handle:Handle,name:string,value:Scalar)->string {
    if luau.owner_error(&owner.vm)!=.None { return strings.clone("Script runtime thread mismatch",owner.allocator) }
    instance,present:=owner.instances[instance_handle.entity]; if !present || instance.serial!=instance_handle.serial { return strings.clone("Script instance is stale",owner.allocator) }
    if len(name)==0 || len(name)>256 || strings.contains(name,"\x00") { return strings.clone("Variable name requires 1..256 bytes",owner.allocator) }
    #partial switch scalar in value {
    case f64: if math.is_nan(scalar) || math.is_inf(scalar) { return strings.clone("Variable number must be finite",owner.allocator) }
    case string: if len(scalar)>1024*1024 { return strings.clone("Variable string exceeds budget",owner.allocator) }
    }
    return environment_set(owner,instance.environment,name,value)
}
