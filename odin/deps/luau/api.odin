//! Source-pinned Luau owns language execution; application policy stays in Odin.
package luau

import "core:dynlib"
import "core:mem"
import "core:sync"

State :: distinct rawptr
Type :: enum i32 { None=-1, Nil, Boolean, Light_Userdata, Number, Vector, String, Table, Function, Userdata, Thread, Buffer }
Error :: enum { None, Library, ABI, Wrong_Thread, Invalid, Native }
Callback :: proc "c"(State,rawptr)->i32
Destructor :: proc "c"(rawptr)
REGISTRY_INDEX :: i32(-10000)
GLOBALS_INDEX :: i32(-10002)
/// Raw Luau stack procedures are used only after the owner's thread guard succeeds.
API :: struct {
    abi:proc "c"()->u32,
    create:proc "c"()->State,
    destroy:proc "c"(State)->i32,
    owner_check:proc "c"(State)->i32,
    bytes:proc "c"(State)->uint,
    compile_load:proc "c"(State,[^]u8,uint,cstring,i32)->i32,
    run:proc "c"(State,i32,i32)->i32,
    callback:proc "c"(State,Callback,rawptr,cstring),
    sandbox:proc "c"(State),
    get_top:proc "c"(State)->i32,
    set_top:proc "c"(State,i32),
    abs_index:proc "c"(State,i32)->i32,
    push_value:proc "c"(State,i32),
    remove:proc "c"(State,i32),
    insert:proc "c"(State,i32),
    type:proc "c"(State,i32)->Type,
    number:proc "c"(State,i32,^i32)->f64,
    boolean:proc "c"(State,i32)->i32,
    string:proc "c"(State,i32,^uint)->[^]u8,
    userdata:proc "c"(State,i32)->rawptr,
    userdata_tag:proc "c"(State,i32)->i32,
    push_nil:proc "c"(State),
    push_number:proc "c"(State,f64),
    push_boolean:proc "c"(State,i32),
    push_string:proc "c"(State,[^]u8,uint),
    new_userdata:proc "c"(State,uint,i32)->rawptr,
    new_userdata_destroy:proc "c"(State,uint,Destructor)->rawptr,
    create_table:proc "c"(State,i32,i32),
    get_field:proc "c"(State,i32,cstring)->Type,
    set_field:proc "c"(State,i32,cstring),
    raw_get_i:proc "c"(State,i32,i32)->Type,
    raw_set_i:proc "c"(State,i32,i32),
    set_metatable:proc "c"(State,i32)->i32,
    set_environment:proc "c"(State,i32)->i32,
    reference:proc "c"(State,i32)->i32,
    unreference:proc "c"(State,i32),
    readonly:proc "c"(State,i32,i32),
    next:proc "c"(State,i32)->i32,
    obj_length:proc "c"(State,i32)->i32,
    collect:proc "c"(State,i32,i32)->i32,
    raw_set_field:proc "c"(State,i32,cstring),
    take_error:proc "c"(State)->cstring,
}
/// Exclusive VM owner; unload occurs only after userdata destructors and native storage finish.
VM :: struct { library:dynlib.Library,state:State,api:API,thread:int,allocator:mem.Allocator }
/// Loads the complete ABI before allocating a VM. No alternative evaluator is selected.
init :: proc(vm:^VM,path:string,allocator:=context.allocator)->Error {
    if vm.library!=nil { return .Invalid }
    loaded:bool; vm.library,loaded=dynlib.load_library(path,allocator=allocator); if !loaded { return .Library }
    success:=false; defer { if !success { dynlib.unload_library(vm.library); vm^={} } }
    names:=[42]string{
        "katla_luau_abi","katla_luau_create","katla_luau_destroy","katla_luau_owner_check","katla_luau_bytes","katla_luau_compile_load","katla_luau_run","katla_luau_push_callback","katla_luau_sandbox",
        "lua_gettop","katla_luau_settop","lua_absindex","katla_luau_pushvalue","katla_luau_remove","katla_luau_insert","lua_type","lua_tonumberx","lua_toboolean","katla_luau_tolstring","lua_touserdata","lua_userdatatag",
        "katla_luau_pushnil","katla_luau_pushnumber","katla_luau_pushboolean","katla_luau_pushlstring","katla_luau_newuserdatatagged","katla_luau_newuserdatadtor","katla_luau_createtable","katla_luau_getfield","katla_luau_setfield","katla_luau_rawgeti","katla_luau_rawseti","katla_luau_setmetatable","katla_luau_setfenv","katla_luau_ref","katla_luau_unref","katla_luau_setreadonly","katla_luau_next","lua_objlen","katla_luau_gc","katla_luau_rawsetfield","katla_luau_take_error",
    }
    #assert(size_of(API)==len(names)*size_of(rawptr))
    for name,i in names { address,found:=dynlib.symbol_address(vm.library,name,allocator=allocator); if !found { return .ABI }; (cast([^]rawptr)&vm.api)[i]=address }
    if vm.api.abi()!=2 { return .ABI }
    vm.state=vm.api.create(); if vm.state==nil { return .Native }
    vm.thread=sync.current_thread_id(); vm.allocator=allocator; success=true; return .None
}
/// Verifies ownership before application code reads or mutates the Lua stack.
owner_error :: proc(vm:^VM)->Error { if vm.state==nil { return .Invalid }; if vm.thread!=sync.current_thread_id() || vm.api.owner_check(vm.state)==0 { return .Wrong_Thread }; return .None }
/// Destroys the owner on its creating thread and confirms the native allocator returned to zero.
destroy :: proc(vm:^VM)->Error {
    err:=owner_error(vm); if err!=.None { return err }
    clean:=vm.api.destroy(vm.state)!=0; dynlib.unload_library(vm.library); vm^={}; if !clean { return .Native }; return .None
}
/// Borrowed string remains valid only while its stack value is retained.
to_string :: proc "contextless"(vm:^VM,index:i32)->string { count:uint; data:=vm.api.string(vm.state,index,&count); if data==nil { return "" }; return string(data[:count]) }
/// Pushes a length-delimited string without depending on NUL termination.
push_string :: proc "contextless"(vm:^VM,value:string) { vm.api.push_string(vm.state,raw_data(value),uint(len(value))) }
/// Pops a known number of stack values.
pop :: proc "contextless"(vm:^VM,count:i32=1) { vm.api.set_top(vm.state,-count-1) }
/// Pushes an owned registry reference onto the stack.
get_reference :: proc "contextless"(vm:^VM,reference:i32) { vm.api.raw_get_i(vm.state,REGISTRY_INDEX,reference) }
