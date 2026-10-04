//! Owned canonical WGSL compilation through the versioned Naga dependency ABI.
package shader

import "core:dynlib"
import "core:encoding/json"
import "core:mem"
import "core:sync"

/// Shader entry stages are independent of native renderer types.
Stage :: enum { Vertex, Fragment, Compute }
/// One selected entry; unselected stages do not participate in its resource interface.
Selection :: struct { name:string, stage:Stage }
/// Named or numeric WGSL override identity with an explicit pipeline value.
Constant :: struct { name:string, value:f64 }
/// Compiler and dependency failures never publish a replacement artifact.
Error :: enum { None, Load_Failed, ABI_Mismatch, Invalid_Request, Parse, Validation, Missing_Entry, Constants, Reflection, Unsupported, Metal_Generation, Spirv_Generation, Internal_Failure, Invalid_Reply, Busy, Closed }
/// Logical resource kind; binding arrays can use a distinct Metal argument-buffer kind.
Binding_Kind :: enum { Buffer, Texture, Sampler }
/// Actual selected-entry data access; query-only resources preserve ownership without reading contents.
Access :: enum { None, Read, Write, Read_Write }
/// Reflected image dimension; None belongs to buffer and sampler bindings.
Dimension :: enum { None, D1, D2, D3, Cube }
/// Shader numeric type, including built-in boolean IO.
Scalar :: enum { None, Sint, Uint, Float, Bool, Abstract }
/// Exact selected-entry resource and Metal argument ABI.
Binding :: struct {
    group,binding:u32,
    name:string,
    kind:Binding_Kind,
    access,declared_access:Access,
    uniform:bool,
    minimum_size:u64,
    alignment:u32,
    runtime_array:bool,
    runtime_array_offset,runtime_array_stride,array_count:u32,
    dimension:Dimension,
    arrayed,multisampled,depth,comparison:bool,
    sample_type:Scalar,
    storage_format:string,
    metal_kind:Binding_Kind,
    metal_index:u32,
    metal_minimum_size:u64,
    size_index:i32,
}
/// Scalar/vector entry IO, with either a location or an exact Naga built-in spelling.
IO :: struct {
    name:string,
    location:i32,
    builtin:string,
    scalar:Scalar,
    width,components:u8,
    interpolation,sampling:string,
    blend_source:i32,
    per_primitive:bool,
}
/// Owns selected-stage binaries and reflection from the same validated module.
Entry :: struct {
    name:string,
    stage:Stage,
    metal_name,metal_source:string,
    spirv:[]u32,
    workgroup_size:[3]u32,
    bindings:[]Binding,
    inputs,outputs:[]IO,
    sizes_buffer:i32,
    sizes_word_count:u32,
}
/// Owns all strings, binaries, reflection and a diagnostic, including failed compiles.
Compiled :: struct { compiler,message:string, entries:[]Entry, allocator:mem.Allocator }
@(private="package")
ABI :: 1
@(private="package")
COMPILER :: "naga-29.0.1;msl-3.0;binding-abi-1;bounds-readzero;binding-arrays-restrict"
@(private="package")
MAX_REQUEST :: 8*1024*1024
@(private="package")
MAX_REPLY :: 128*1024*1024
@(private="package")
Native_Buffer :: struct { data:[^]u8, length:uint }
@(private="package")
Native_Compile :: #type proc "c" (data:[^]u8,length:uint)->Native_Buffer
@(private="package")
Native_Free :: #type proc "c" (buffer:Native_Buffer)
/// Stationary compiler dependency owner; concurrent compiles are allowed, unload waits for callers.
Compiler :: struct {
    library:dynlib.Library,
    compile:Native_Compile,
    release:Native_Free,
    mutex:sync.Mutex,
    active:int,
}
@(private="package")
Request :: struct { abi:u32, source:string, selections:[]Selection, constants:map[string]f64 }
@(private="package")
Reply :: struct { abi:u32, compiler:string, error:Error, message:string, entries:[]Entry }
/// Opens only the explicit dependency path and validates every mandatory ABI symbol.
compiler_init :: proc(compiler:^Compiler,path:string)->Error {
    if compiler.library!=nil { return .Busy }
    library,loaded:=dynlib.load_library(path)
    if !loaded { return .Load_Failed }
    success:=false; defer { if !success { dynlib.unload_library(library) } }
    abi_ptr,has_abi:=dynlib.symbol_address(library,"katla_naga_abi")
    compile_ptr,has_compile:=dynlib.symbol_address(library,"katla_naga_compile")
    free_ptr,has_free:=dynlib.symbol_address(library,"katla_naga_free")
    if !has_abi || !has_compile || !has_free { return .ABI_Mismatch }
    abi:=cast(proc "c" ()->u32)abi_ptr
    if abi()!=ABI { return .ABI_Mismatch }
    compiler.library=library
    compiler.compile=cast(Native_Compile)compile_ptr
    compiler.release=cast(Native_Free)free_ptr
    success=true; return .None
}
/// Rejects unload while any accepted compiler call is using the library.
compiler_destroy :: proc(compiler:^Compiler)->Error {
    sync.mutex_lock(&compiler.mutex); defer sync.mutex_unlock(&compiler.mutex)
    if compiler.active!=0 { return .Busy }
    if compiler.library!=nil {
        if !dynlib.unload_library(compiler.library) { return .Load_Failed }
        compiler.library=nil; compiler.compile=nil; compiler.release=nil
    }
    return .None
}
@(private="package")
io_destroy :: proc(values:[]IO,allocator:mem.Allocator) {
    for value in values { delete(value.name,allocator); delete(value.builtin,allocator); delete(value.interpolation,allocator); delete(value.sampling,allocator) }
    delete(values,allocator)
}
/// Releases immutable artifacts with their captured allocator after all consumers retire.
compiled_destroy :: proc(compiled:^Compiled) {
    allocator:=compiled.allocator
    for entry in compiled.entries {
        delete(entry.name,allocator); delete(entry.metal_name,allocator); delete(entry.metal_source,allocator); delete(entry.spirv,allocator)
        for binding in entry.bindings { delete(binding.name,allocator); delete(binding.storage_format,allocator) }
        delete(entry.bindings,allocator)
        io_destroy(entry.inputs,allocator); io_destroy(entry.outputs,allocator)
    }
    delete(compiled.entries,allocator); delete(compiled.compiler,allocator); delete(compiled.message,allocator)
    compiled^={}
}
@(private="package")
entry_valid :: proc(entry:Entry)->bool {
    if len(entry.name)==0 || len(entry.metal_name)==0 || len(entry.metal_source)==0 || len(entry.spirv)<5 || entry.spirv[0]!=0x07230203 { return false }
    if entry.sizes_buffer< -1 || entry.sizes_buffer>30 || (entry.sizes_buffer== -1)!=(entry.sizes_word_count==0) { return false }
    if entry.stage==.Compute { for count in entry.workgroup_size { if count==0 { return false } } }
    for binding,i in entry.bindings {
        if binding.array_count==0 || binding.alignment==0 || (binding.alignment&(binding.alignment-1))!=0 { return false }
        for prior in entry.bindings[:i] {
            if prior.group==binding.group && prior.binding==binding.binding { return false }
            if prior.metal_kind==binding.metal_kind && prior.metal_index==binding.metal_index { return false }
        }
        limit:u32
        switch binding.metal_kind {
        case .Buffer: limit=31
        case .Texture: limit=128
        case .Sampler: limit=16
        }
        if binding.metal_index>=limit { return false }
        if binding.kind==.Buffer && (binding.minimum_size==0 || binding.metal_minimum_size==0) { return false }
        if binding.runtime_array {
            if binding.runtime_array_stride==0 || binding.size_index<0 || u32(binding.size_index)>=entry.sizes_word_count || entry.sizes_buffer<0 { return false }
        } else if binding.size_index!= -1 { return false }
        if binding.metal_kind==.Buffer && i32(binding.metal_index)==entry.sizes_buffer { return false }
    }
    return true
}
/// Parses, validates and lowers arbitrary WGSL, resolving overrides before either backend emits code.
compile :: proc(compiler:^Compiler,source:string,selections:[]Selection,constants:[]Constant=nil,allocator:=context.allocator)->(Compiled,Error) {
    context.allocator=allocator
    if len(source)==0 || len(source)>MAX_REQUEST || len(selections)==0 || len(selections)>16 || len(constants)>1024 { return {},.Invalid_Request }
    for selected,i in selections {
        if len(selected.name)==0 { return {},.Invalid_Request }
        for previous in selections[:i] { if previous==selected { return {},.Invalid_Request } }
    }
    values:=make(map[string]f64,allocator); defer delete(values)
    for constant in constants {
        if len(constant.name)==0 || !(constant.value>= -max(f64) && constant.value<=max(f64)) { return {},.Invalid_Request }
        if _,exists:=values[constant.name]; exists { return {},.Invalid_Request }
        values[constant.name]=constant.value
    }
    bytes,marshal_error:=json.marshal(Request{ABI,source,selections,values},opt={spec=.JSON,use_enum_names=true},allocator=allocator)
    if marshal_error!=nil { return {},.Invalid_Request }; defer delete(bytes,allocator)
    if len(bytes)>MAX_REQUEST { return {},.Invalid_Request }
    sync.mutex_lock(&compiler.mutex)
    if compiler.library==nil { sync.mutex_unlock(&compiler.mutex); return {},.Closed }
    compiler.active+=1
    native_compile,native_free:=compiler.compile,compiler.release
    sync.mutex_unlock(&compiler.mutex)
    defer { sync.mutex_lock(&compiler.mutex); compiler.active-=1; sync.mutex_unlock(&compiler.mutex) }
    buffer:=native_compile(raw_data(bytes),uint(len(bytes))); defer native_free(buffer)
    if buffer.data==nil || buffer.length==0 || buffer.length>MAX_REPLY { return {},.Invalid_Reply }
    reply:Reply
    decode_error:=json.unmarshal(buffer.data[:int(buffer.length)],&reply,spec=.JSON,allocator=allocator)
    owned:=Compiled{reply.compiler,reply.message,reply.entries,allocator}
    if decode_error!=nil || reply.abi!=ABI || reply.compiler!=COMPILER { compiled_destroy(&owned); return {},.Invalid_Reply }
    if reply.error!=.None {
        if len(reply.entries)!=0 || len(reply.message)==0 { compiled_destroy(&owned); return {},.Invalid_Reply }
        return owned,reply.error
    }
    if len(reply.entries)!=len(selections) || len(reply.message)!=0 { compiled_destroy(&owned); return {},.Invalid_Reply }
    for entry,i in reply.entries { if entry.name!=selections[i].name || entry.stage!=selections[i].stage || !entry_valid(entry) { compiled_destroy(&owned); return {},.Invalid_Reply } }
    return owned,.None
}
/// Finds one immutable selected entry without borrowing the compiler dependency.
find_entry :: proc(compiled:^Compiled,name:string,stage:Stage)->(^Entry,bool) {
    for &entry in compiled.entries { if entry.name==name && entry.stage==stage { return &entry,true } }
    return nil,false
}
