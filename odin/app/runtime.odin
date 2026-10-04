//! A thread-affine dependency bridge owns Rapier/Luau internals without retaining scene pointers.
package app

import ecs "../ecs"
import editor "../editor"
import "core:dynlib"
import "core:encoding/json"
import "core:mem"
import "core:slice"

/// Captures only canonical runtime procedures and their exclusive dependency owner.
Scene_Runtime :: struct {
    library:dynlib.Library,
    instance:rawptr,
    destroy:proc "c"(rawptr),
    call:proc "c"(rawptr,[^]u8,uint,^uint)->[^]u8,
    free_result:proc "c"([^]u8,uint),
}
/// Owns a decoded response tree; caller releases it even on a dependency-reported failure.
Runtime_Response :: struct { tree:json.Value, result:json.Value, error:string, allocator:mem.Allocator, ok:bool }
/// Releases all response bytes/strings with the allocator that received them.
runtime_response_destroy :: proc(response:^Runtime_Response) { context.allocator=response.allocator; json.destroy_value(response.tree); response^={} }
@(private="package")
scene_runtime_destroy :: proc(value:rawptr) {
    runtime:=cast(^Scene_Runtime)value
    if runtime.instance!=nil { runtime.destroy(runtime.instance) }
    if runtime.library!=nil { dynlib.unload_library(runtime.library) }
    runtime^={}
}
/// Loads all ABI entries before creating the owner; unsupported/missing libraries return failure.
scene_runtime_init :: proc(app:^Authoring,path:string)->editor.Scene_Error {
    context.allocator=app.world.allocator
    if ecs.contains_resource(&app.world,Scene_Runtime) { return .Invalid_Operation }
    runtime:Scene_Runtime; loaded:bool; runtime.library,loaded=dynlib.load_library(path,allocator=app.world.allocator)
    if !loaded { return .Application_Owned }; success:=false; defer { if !success { scene_runtime_destroy(&runtime) } }
    abi_address,abi_found:=dynlib.symbol_address(runtime.library,"katla_scene_runtime_abi",allocator=app.world.allocator)
    create_address,create_found:=dynlib.symbol_address(runtime.library,"katla_scene_runtime_create",allocator=app.world.allocator)
    destroy_address,destroy_found:=dynlib.symbol_address(runtime.library,"katla_scene_runtime_destroy",allocator=app.world.allocator)
    call_address,call_found:=dynlib.symbol_address(runtime.library,"katla_scene_runtime_call",allocator=app.world.allocator)
    free_address,free_found:=dynlib.symbol_address(runtime.library,"katla_scene_runtime_free",allocator=app.world.allocator)
    if !(abi_found && create_found && destroy_found && call_found && free_found) { return .Application_Owned }
    abi:=cast(proc "c"()->u32)abi_address; if abi()!=1 { return .Application_Owned }
    create:=cast(proc "c"()->rawptr)create_address; runtime.destroy=cast(proc "c"(rawptr))destroy_address
    runtime.call=cast(proc "c"(rawptr,[^]u8,uint,^uint)->[^]u8)call_address; runtime.free_result=cast(proc "c"([^]u8,uint))free_address
    runtime.instance=create(); if runtime.instance==nil { return .Application_Owned }
    ecs.insert_resource(&app.world,runtime,ecs.Value_Ops{destroy=scene_runtime_destroy}); success=true; return .None
}
/// Calls the runtime with bounded JSON and copies its response before freeing foreign storage.
scene_runtime_call :: proc(app:^Authoring,request:$T)->Runtime_Response {
    context.allocator=app.world.allocator; response:=Runtime_Response{allocator=app.world.allocator}
    input,input_error:=json.marshal(request,allocator=app.world.allocator); if input_error!=nil { return response }; defer delete(input)
    return scene_runtime_call_json(app,input)
}
/// Executes a pre-serialized runtime request on the exclusive application owner.
scene_runtime_call_json :: proc(app:^Authoring,input:[]byte)->Runtime_Response {
    context.allocator=app.world.allocator
    response:=Runtime_Response{allocator=app.world.allocator}
    runtime:=ecs.get_resource_mut(&app.world,Scene_Runtime); if runtime==nil || len(input)>16*1024*1024 { return response }
    count:uint; data:=runtime.call(runtime.instance,raw_data(input),uint(len(input)),&count)
    if data==nil { return response }; defer runtime.free_result(data,count)
    if count>16*1024*1024 { return response }
    copied:=slice.clone(data[:count],app.world.allocator); defer delete(copied)
    tree,parse_error:=json.parse(copied,spec=.JSON,parse_integers=true,allocator=app.world.allocator); if parse_error!=nil { return response }
    response.tree=tree; object,is_object:=tree.(json.Object); if !is_object { return response }
    response.ok,_=object["ok"].(bool); response.result=object["result"]; response.error,_=object["error"].(string); return response
}
