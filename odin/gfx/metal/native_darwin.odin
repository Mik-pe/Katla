#+build darwin, arm64
//! Metal 4 calls missing from Odin's bundled bindings, with explicit error propagation.
package metal

import gfx ".."
import MTL "vendor:darwin/Metal"
import NS "core:sys/darwin/Foundation"
import "base:intrinsics"
import "core:sync"
import "core:mem"
import "core:log"

@(private="package")
send :: intrinsics.objc_send
@(private="package")
new_object :: proc(name:cstring)->^NS.Object {
    cls:=NS.objc_lookUpClass(name)
    if cls==nil { return nil }
    object:=send(^NS.Object,cast(^NS.Object)cls,"alloc")
    if object==nil { return nil }
    return send(^NS.Object,object,"init")
}
@(private="package")
report_error :: proc(error:^NS.Error,label:string) {
    if error!=nil { log.error(label,error->localizedDescription()->odinString()) }
    else { log.error(label) }
}
@(private="package")
Completion :: struct { mutex:sync.Mutex, wait_group:sync.Wait_Group, done,failed:bool, code:NS.Integer, message:[512]byte, message_length:int }
@(private="package")
feedback :: proc "c" (data:rawptr,native:^NS.Object) {
    c:=cast(^Completion)data
    error:=send(^NS.Error,native,"error")
    sync.mutex_lock(&c.mutex)
    c.failed=error!=nil
    if error!=nil {
        c.code=error->code()
        message:=error->localizedDescription()->odinString()
        c.message_length=min(len(message),len(c.message))
        for i in 0..<c.message_length { c.message[i]=message[i] }
    }
    c.done=true
    sync.wait_group_done(&c.wait_group)
    sync.mutex_unlock(&c.mutex)
}
@(private="package")
Native_Buffer :: struct { object:^MTL.Buffer, desc:gfx.Buffer_Desc, refs,pending:int }
@(private="package")
Native_Pipeline :: struct { object:^NS.Object, requirements:[]gfx.Binding_Requirement, local_size:[3]u32, max_threads:u64, refs:int }
@(private="package")
Native_Frame :: struct {
    allocator,command,residency,options:^NS.Object,
    block:^NS.Block,
    completion:^Completion,
    buffers:[dynamic]^Native_Buffer,
    pipelines:[dynamic]^Native_Pipeline,
    tables:[dynamic]^NS.Object,
    token:gfx.Frame_Token,
    submission:u64,
    resident:bool,
}
/// Stationary, thread-affine headless owner with three exact native submission slots.
Renderer :: struct {
    device:^MTL.Device,
    queue,compiler:^NS.Object,
    buffers:gfx.Resource_Storage(^Native_Buffer,gfx.Buffer_Kind),
    pipelines:gfx.Resource_Storage(^Native_Pipeline,gfx.Pipeline_Kind),
    frames:gfx.Frames,
    slots:[3]Native_Frame,
    next_slot:int,
    failed:bool,
    allocator:mem.Allocator,
}
/// Requires Metal 4 before creating any compiler, queue or command allocator.
renderer_init :: proc(r:^Renderer,allocator:=context.allocator)->gfx.Gpu_Error {
    r.allocator=allocator
    r.device=MTL.CreateSystemDefaultDevice()
    if r.device==nil { return .No_Device }
    if !bool(r.device->supportsFamily(MTL.GPUFamily(5002))) { send(nil,r.device,"release"); r.device=nil; return .Unsupported }
    success:=false; defer { if !success { renderer_destroy(r) } }
    descriptor:=new_object("MTL4CompilerDescriptor")
    if descriptor==nil { return .Allocation_Failed }; defer descriptor->release()
    native_error:^NS.Error
    r.compiler=send(^NS.Object,r.device,"newCompilerWithDescriptor:error:",descriptor,&native_error)
    if r.compiler==nil { report_error(native_error,"Metal 4 compiler creation failed"); return .Allocation_Failed }
    r.queue=send(^NS.Object,r.device,"newMTL4CommandQueue")
    if r.queue==nil { return .Allocation_Failed }
    gfx.storage_init(&r.buffers,allocator); gfx.storage_init(&r.pipelines,allocator); gfx.frames_init(&r.frames,3,allocator)
    for &slot in r.slots {
        slot.allocator=send(^NS.Object,r.device,"newCommandAllocator")
        if slot.allocator==nil { return .Allocation_Failed }
        slot.buffers=make([dynamic]^Native_Buffer,allocator)
        slot.pipelines=make([dynamic]^Native_Pipeline,allocator)
        slot.tables=make([dynamic]^NS.Object,allocator)
    }
    success=true; log.info("Odin Metal 4 headless renderer initialized")
    return .None
}
@(private="package")
release_buffer :: proc(r:^Renderer,buffer:^Native_Buffer) {
    buffer.refs-=1
    if buffer.refs==0 { send(nil,buffer.object,"release"); free(buffer,r.allocator) }
}
@(private="package")
release_pipeline :: proc(r:^Renderer,pipeline:^Native_Pipeline) {
    pipeline.refs-=1
    if pipeline.refs==0 { pipeline.object->release(); delete(pipeline.requirements,r.allocator); free(pipeline,r.allocator) }
}
/// Removes registry identity now; accepted submissions retain their native buffer owner.
destroy_buffer :: proc(r:^Renderer,handle:gfx.Buffer_Handle)->gfx.Gpu_Error {
    buffer,ok:=gfx.storage_remove(&r.buffers,handle); if !ok { return .Invalid_Resource }
    release_buffer(r,buffer); return .None
}
/// Removes registry identity while previously accepted work keeps its pipeline alive.
destroy_pipeline :: proc(r:^Renderer,handle:gfx.Pipeline_Handle)->gfx.Gpu_Error {
    pipeline,ok:=gfx.storage_remove(&r.pipelines,handle); if !ok { return .Invalid_Resource }
    release_pipeline(r,pipeline); return .None
}
/// Drains every slot, releases native values, then releases their queue/device parents.
renderer_destroy :: proc(r:^Renderer)->gfx.Gpu_Error {
    outcome:=gfx.Gpu_Error.None
    for &slot in r.slots {
        if slot.submission!=0 {
            sync.wait_group_wait(&slot.completion.wait_group)
            err:=retire_slot(r,&slot); if err!=.None { outcome=err }
        }
        if slot.allocator!=nil { slot.allocator->release() }
        delete(slot.buffers); delete(slot.pipelines); delete(slot.tables)
    }
    for &slot in r.buffers.slots { if slot.occupied { release_buffer(r,slot.value); slot.occupied=false } }
    for &slot in r.pipelines.slots { if slot.occupied { release_pipeline(r,slot.value); slot.occupied=false } }
    r.buffers.count=0; r.pipelines.count=0
    gfx.storage_destroy(&r.buffers); gfx.storage_destroy(&r.pipelines)
    if r.frames.slots!=nil { gfx.frames_destroy(&r.frames) }
    if r.queue!=nil { r.queue->release() }; if r.compiler!=nil { r.compiler->release() }
    if r.device!=nil { send(nil,r.device,"release") }
    r^={}; return outcome
}
/// Creates CPU-visible shared storage; its initial bytes remain undefined until written.
create_buffer :: proc(r:^Renderer,desc:gfx.Buffer_Desc)->(gfx.Buffer_Handle,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    if desc.size==0 || desc.size>u64(max(int)) || desc.usage=={} { return {},.Invalid_Range }
    object:=r.device->newBufferWithLength(NS.UInteger(desc.size),MTL.ResourceStorageModeShared)
    if object==nil { return {},.Allocation_Failed }
    buffer:=new(Native_Buffer,r.allocator); buffer^={object,desc,1,0}
    return gfx.storage_insert(&r.buffers,buffer),.None
}
/// Validates both byte bounds and native pending ownership before a CPU write.
write_buffer :: proc(r:^Renderer,handle:gfx.Buffer_Handle,offset:u64,data:[]byte)->gfx.Gpu_Error {
    entry,ok:=gfx.storage_get(&r.buffers,handle); if !ok { return .Invalid_Resource }
    buffer:=entry^
    if buffer.pending!=0 { return .Busy }
    if offset>buffer.desc.size || u64(len(data))>buffer.desc.size-offset { return .Invalid_Range }
    bytes:=buffer.object->contents(); copy(bytes[int(offset):int(offset)+len(data)],data)
    return .None
}
/// Copies completed shared bytes only after every native consumer of this allocation retires.
read_buffer :: proc(r:^Renderer,handle:gfx.Buffer_Handle,offset:u64,data:[]byte)->gfx.Gpu_Error {
    entry,ok:=gfx.storage_get(&r.buffers,handle); if !ok { return .Invalid_Resource }
    buffer:=entry^
    if buffer.pending!=0 { return .Busy }
    if offset>buffer.desc.size || u64(len(data))>buffer.desc.size-offset { return .Invalid_Range }
    bytes:=buffer.object->contents(); copy(data,bytes[int(offset):int(offset)+len(data)])
    return .None
}
/// Compiles through MTL4Compiler and retains native reflection for generic packet preflight.
create_pipeline :: proc(r:^Renderer,desc:gfx.Compute_Desc)->(gfx.Pipeline_Handle,gfx.Gpu_Error) {
    if r.compiler==nil || r.failed { return {},.Native_Failure }
    if len(desc.entry)==0 || len(desc.metal_source)==0 || len(desc.buffers)>32 { return {},.Invalid_Shader }
    threads:u64=1
    limits:=r.device->maxThreadsPerThreadgroup()
    dimensions:=[3]NS.Integer{limits.width,limits.height,limits.depth}
    for count,i in desc.local_size { if count==0 || NS.Integer(count)>dimensions[i] || u64(count)>1024/threads { return {},.Invalid_Shader }; threads*=u64(count) }
    for buffer,i in desc.buffers {
        if buffer.slot>=32 || (buffer.usage!=.Storage && buffer.usage!=.Uniform) { return {},.Invalid_Shader }
        for previous in desc.buffers[:i] { if previous.slot==buffer.slot { return {},.Invalid_Shader } }
    }
    library_desc:=new_object("MTL4LibraryDescriptor"); if library_desc==nil { return {},.Allocation_Failed }; defer library_desc->release()
    source:=NS.String.alloc()->initWithOdinString(desc.metal_source); if source==nil { return {},.Allocation_Failed }; defer source->release()
    send(nil,library_desc,"setSource:",source)
    native_error:^NS.Error
    library:=send(^NS.Object,r.compiler,"newLibraryWithDescriptor:error:",library_desc,&native_error)
    if library==nil { report_error(native_error,"Metal shader compilation failed"); return {},.Shader_Compile_Failed }; defer library->release()
    function:=new_object("MTL4LibraryFunctionDescriptor"); if function==nil { return {},.Allocation_Failed }; defer function->release()
    name:=NS.String.alloc()->initWithOdinString(desc.entry); if name==nil { return {},.Allocation_Failed }; defer name->release()
    send(nil,function,"setLibrary:",library); send(nil,function,"setName:",name)
    pipeline_desc:=new_object("MTL4ComputePipelineDescriptor"); if pipeline_desc==nil { return {},.Allocation_Failed }; defer pipeline_desc->release()
    options:=new_object("MTL4PipelineOptions"); if options==nil { return {},.Allocation_Failed }; defer options->release()
    send(nil,options,"setShaderReflection:",NS.UInteger(3)); send(nil,pipeline_desc,"setOptions:",options)
    send(nil,pipeline_desc,"setComputeFunctionDescriptor:",function)
    object:=send(^NS.Object,r.compiler,"newComputePipelineStateWithDescriptor:compilerTaskOptions:error:",pipeline_desc,cast(^NS.Object)nil,&native_error)
    if object==nil { report_error(native_error,"Metal compute pipeline compilation failed"); return {},.Shader_Compile_Failed }
    success:=false; defer { if !success { object->release() } }
    reflection:=send(^MTL.ComputePipelineReflection,object,"reflection")
    if reflection==nil { return {},.Invalid_Shader }
    reflected:=reflection->bindings()
    requirements:=make([]gfx.Binding_Requirement,len(desc.buffers),r.allocator)
    defer { if !success { delete(requirements,r.allocator) } }
    used:=0
    for i in 0..<int(reflected->count()) {
        binding:=reflected->objectAs(NS.UInteger(i),^MTL.Binding)
        if !bool(binding->isUsed()) { continue }
        if binding->type()!=.Buffer { return {},.Unsupported }
        native_buffer:=cast(^MTL.BufferBinding)binding
        found:=false
        for requested,j in desc.buffers {
            if NS.UInteger(requested.slot)!=binding->index() { continue }
            if requested.usage==.Uniform && binding->access()!=.ReadOnly { return {},.Invalid_Shader }
            requirements[j]={requested.slot,requested.usage,max(u64(native_buffer->bufferDataSize()),1),max(u64(native_buffer->bufferAlignment()),1),u64(send(NS.UInteger,r.device,"maxBufferLength"))}
            found=true; used+=1
        }
        if !found { return {},.Invalid_Shader }
    }
    if used!=len(requirements) { return {},.Invalid_Shader }
    max_threads:=u64(send(NS.UInteger,object,"maxTotalThreadsPerThreadgroup"))
    if threads>max_threads { return {},.Invalid_Shader }
    pipeline:=new(Native_Pipeline,r.allocator); pipeline^={object,requirements,desc.local_size,max_threads,1}
    success=true; return gfx.storage_insert(&r.pipelines,pipeline),.None
}
@(private="package")
query_buffer :: proc(state:rawptr,handle:gfx.Buffer_Handle)->(gfx.Buffer_Info,bool) {
    r:=cast(^Renderer)state; buffer,ok:=gfx.storage_get(&r.buffers,handle)
    if !ok { return {},false }
    return {buffer^.desc,buffer^.object},true
}
@(private="package")
query_pipeline :: proc(state:rawptr,handle:gfx.Pipeline_Handle)->(gfx.Pipeline_Info,bool) {
    r:=cast(^Renderer)state; pipeline,ok:=gfx.storage_get(&r.pipelines,handle)
    if !ok { return {},false }
    return {pipeline^.requirements,pipeline^.local_size,pipeline^.max_threads},true
}
/// Returns the required native lookup callbacks for portable preflight.
resource_query :: proc(r:^Renderer)->gfx.Resource_Query { return {r,query_buffer,query_pipeline,{65535,65535,65535}} }
