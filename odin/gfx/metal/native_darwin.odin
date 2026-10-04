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
Native_Buffer :: struct { object:^MTL.Buffer, desc:gfx.Buffer_Desc, refs,pending:int, heap:^Native_Heap }
@(private="package")
Native_Pipeline :: struct { object:^NS.Object, requirements:[]gfx.Binding_Requirement, images:[]gfx.Image_Binding_Requirement, samplers:[]gfx.Sampler_Requirement, desc:gfx.Compute_Desc, local_size:[3]u32, max_threads:u64, refs:int }
@(private="package")
Native_Frame :: struct {
    allocator,command,residency,options:^NS.Object,
    block:^NS.Block,
    completion:^Completion,
    buffers:[dynamic]^Native_Buffer,
    pipelines:[dynamic]^Native_Pipeline,
    tables:[dynamic]^NS.Object,
    auxiliary:[dynamic]^NS.Object,
    textures:[dynamic]^Native_Texture,
    graphics:[dynamic]^Native_Graphics,
    samplers:[dynamic]^Native_Sampler,
    token:gfx.Frame_Token,
    submission:u64,
    resident:bool,
}
/// Stationary, thread-affine headless owner with three exact native submission slots.
Renderer :: struct {
    device:^MTL.Device,
    queue,compiler:^NS.Object,
    buffers:gfx.Resource_Storage(^Native_Buffer,gfx.Buffer_Kind),
    fill_pipeline:gfx.Pipeline_Handle,
    pipelines:gfx.Resource_Storage(^Native_Pipeline,gfx.Pipeline_Kind),
    textures:gfx.Resource_Storage(^Native_Texture,gfx.Texture_Kind),
    graphics:gfx.Resource_Storage(^Native_Graphics,gfx.Graphics_Pipeline_Kind),
    samplers:gfx.Resource_Storage(^Native_Sampler,gfx.Sampler_Kind),
    readbacks:gfx.Resource_Storage(^Native_Texture_Transfer,gfx.Readback_Kind),
    exports:[dynamic]Published_Source,
    uploads:[dynamic]^Native_Texture_Transfer,
    frames:gfx.Frames,
    slots:[3]Native_Frame,
    next_slot:int,
    content_epoch:u64,
    failed:bool,
    surface:Surface,
    surface_texture:gfx.Texture_Handle,
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
    gfx.storage_init(&r.buffers,allocator); gfx.storage_init(&r.pipelines,allocator); gfx.storage_init(&r.textures,allocator); gfx.frames_init(&r.frames,3,allocator)
    gfx.storage_init(&r.graphics,allocator); gfx.storage_init(&r.samplers,allocator)
    gfx.storage_init(&r.readbacks,allocator); r.exports=make([dynamic]Published_Source,allocator); r.uploads=make([dynamic]^Native_Texture_Transfer,allocator)
    for &slot in r.slots {
        slot.allocator=send(^NS.Object,r.device,"newCommandAllocator")
        if slot.allocator==nil { return .Allocation_Failed }
        slot.buffers=make([dynamic]^Native_Buffer,allocator)
        slot.pipelines=make([dynamic]^Native_Pipeline,allocator)
        slot.tables=make([dynamic]^NS.Object,allocator)
        slot.auxiliary=make([dynamic]^NS.Object,allocator)
        slot.textures=make([dynamic]^Native_Texture,allocator)
        slot.graphics=make([dynamic]^Native_Graphics,allocator)
        slot.samplers=make([dynamic]^Native_Sampler,allocator)
    }
    fill_error:=prepare_fill_pipeline(r); if fill_error!=.None { return fill_error }
    success=true; log.info("Odin Metal 4 headless renderer initialized")
    return .None
}
@(private="package")
release_buffer :: proc(r:^Renderer,buffer:^Native_Buffer) {
    buffer.refs-=1
    if buffer.refs==0 { send(nil,buffer.object,"release"); if buffer.heap!=nil { release_heap(r,buffer.heap) }; free(buffer,r.allocator) }
}
@(private="package")
release_pipeline :: proc(r:^Renderer,pipeline:^Native_Pipeline) {
    pipeline.refs-=1
    if pipeline.refs==0 { pipeline.object->release(); delete(pipeline.requirements,r.allocator); delete(pipeline.images,r.allocator); delete(pipeline.samplers,r.allocator); delete(pipeline.desc.buffers,r.allocator); delete(pipeline.desc.images,r.allocator); delete(pipeline.desc.samplers,r.allocator); free(pipeline,r.allocator) }
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
        delete(slot.auxiliary); delete(slot.textures); delete(slot.graphics); delete(slot.samplers)
    }
    for &slot in r.buffers.slots { if slot.occupied { release_buffer(r,slot.value); slot.occupied=false } }
    for &slot in r.readbacks.slots { if slot.occupied { err:=release_texture_transfer(r,slot.value); if err!=.None { outcome=err }; slot.occupied=false } }
    upload_error:=retire_uploads(r,true); if upload_error!=.None { outcome=upload_error }; delete(r.uploads)
    for published in r.exports { release_texture(r,published.texture) }; delete(r.exports)
    for &slot in r.pipelines.slots { if slot.occupied { release_pipeline(r,slot.value); slot.occupied=false } }
    err:=detach_surface(r); if err!=.None { outcome=err }
    for &slot in r.textures.slots { if slot.occupied { release_texture(r,slot.value); slot.occupied=false } }
    for &slot in r.graphics.slots { if slot.occupied { release_graphics(r,slot.value); slot.occupied=false } }
    for &slot in r.samplers.slots { if slot.occupied { release_sampler(r,slot.value); slot.occupied=false } }
    r.buffers.count=0; r.pipelines.count=0; r.textures.count=0
    r.graphics.count=0; r.samplers.count=0
    r.readbacks.count=0
    gfx.storage_destroy(&r.buffers); gfx.storage_destroy(&r.pipelines); gfx.storage_destroy(&r.textures)
    gfx.storage_destroy(&r.graphics); gfx.storage_destroy(&r.samplers)
    gfx.storage_destroy(&r.readbacks)
    if r.frames.slots!=nil { gfx.frames_destroy(&r.frames) }
    if r.queue!=nil { r.queue->release() }; if r.compiler!=nil { r.compiler->release() }
    if r.device!=nil { send(nil,r.device,"release") }
    r^={}; return outcome
}
/// Creates CPU-visible shared storage; its initial bytes remain undefined until written.
create_buffer :: proc(r:^Renderer,desc:gfx.Buffer_Desc)->(gfx.Buffer_Handle,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    _,requirement_error:=allocation_buffer_requirements(r,desc)
    if requirement_error!=.None { return {},requirement_error }
    object:=r.device->newBufferWithLength(NS.UInteger(desc.size),buffer_options(desc.memory))
    if object==nil { return {},.Allocation_Failed }
    buffer:=new(Native_Buffer,r.allocator); buffer^={object=object,desc=desc,refs=1}
    return gfx.storage_insert(&r.buffers,buffer),.None
}
/// Creates fully initialized immutable bytes without acquiring a mutable frame slot.
create_buffer_with_data :: proc(r:^Renderer,desc:gfx.Buffer_Desc,data:[]byte)->(gfx.Buffer_Handle,gfx.Gpu_Error) {
    if u64(len(data))!=desc.size { return {},.Invalid_Range }
    handle,err:=create_buffer(r,desc)
    if err!=.None { return {},err }
    entry,_:=gfx.storage_get(&r.buffers,handle)
    if desc.memory==.GPU_Private {
        err=initialize_private_buffer(r,entry^,data)
        if err!=.None { destroy_buffer(r,handle); return {},err }
    } else { copy(entry^.object->contents(),data) }
    return handle,.None
}
/// Validates both byte bounds and native pending ownership before a CPU write.
write_buffer :: proc(r:^Renderer,token:gfx.Frame_Token,handle:gfx.Buffer_Handle,offset:u64,data:[]byte)->gfx.Gpu_Error {
    if !acquired_token_valid(r,token) { return .Invalid_Resource }
    entry,ok:=gfx.storage_get(&r.buffers,handle); if !ok { return .Invalid_Resource }
    buffer:=entry^
    if buffer.desc.memory!=.CPU_Visible { return .Unsupported }
    if buffer.pending!=0 || (buffer.heap!=nil && buffer.heap.pending!=0) { return .Busy }
    if offset>buffer.desc.size || u64(len(data))>buffer.desc.size-offset { return .Invalid_Range }
    if buffer.heap!=nil && r.content_epoch==max(u64) { return .Native_Failure }
    bytes:=buffer.object->contents(); copy(bytes[int(offset):int(offset)+len(data)],data)
    mark_buffer_written(r,buffer)
    return .None
}
/// Copies completed shared bytes only after every native consumer of this allocation retires.
read_buffer :: proc(r:^Renderer,handle:gfx.Buffer_Handle,offset:u64,data:[]byte)->gfx.Gpu_Error {
    entry,ok:=gfx.storage_get(&r.buffers,handle); if !ok { return .Invalid_Resource }
    buffer:=entry^
    if buffer.desc.memory!=.CPU_Visible { return .Unsupported }
    if buffer.pending!=0 || (buffer.heap!=nil && buffer.heap.pending!=0) { return .Busy }
    if offset>buffer.desc.size || u64(len(data))>buffer.desc.size-offset { return .Invalid_Range }
    bytes:=buffer.object->contents(); copy(data,bytes[int(offset):int(offset)+len(data)])
    return .None
}
@(private="package")
query_buffer :: proc(state:rawptr,handle:gfx.Buffer_Handle)->(gfx.Buffer_Info,bool) {
    r:=cast(^Renderer)state; buffer,ok:=gfx.storage_get(&r.buffers,handle)
    if !ok { return {},false }
    identity:=rawptr(buffer^.object)
    if buffer^.heap!=nil { identity=buffer^.heap.object }
    return {buffer^.desc,identity},true
}
@(private="package")
query_pipeline :: proc(state:rawptr,handle:gfx.Pipeline_Handle)->(gfx.Pipeline_Info,bool) {
    r:=cast(^Renderer)state; pipeline,ok:=gfx.storage_get(&r.pipelines,handle)
    if !ok { return {},false }
    return {bindings=pipeline^.requirements,images=pipeline^.images,samplers=pipeline^.samplers,local_size=pipeline^.local_size,max_threads=pipeline^.max_threads},true
}
/// Returns the required native lookup callbacks for portable preflight.
resource_query :: proc(r:^Renderer)->gfx.Resource_Query { return {r,query_buffer,query_pipeline,{65535,65535,65535}} }
