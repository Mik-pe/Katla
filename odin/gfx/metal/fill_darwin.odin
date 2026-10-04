#+build darwin, arm64
//! Native byte fills and prepared word-pattern compute fills share explicit accepted ownership.
package metal

import gfx ".."
import MTL "vendor:darwin/Metal"
import NS "core:sys/darwin/Foundation"
import "core:mem"

@(private="package")
fill_source :: `#include <metal_stdlib>
using namespace metal;
kernel void fill_words(device uint *dst [[buffer(0)]], constant uint4 &p [[buffer(1)]], uint i [[thread_position_in_grid]]) {
    if (i < p.x) dst[i] = p.y;
}`

@(private="package")
prepare_fill_pipeline :: proc(r:^Renderer)->gfx.Gpu_Error {
    handle,err:=create_pipeline(r,{entry="fill_words",metal_entry="fill_words",metal_source=fill_source,local_size={64,1,1},runtime_sizes_index=-1,buffers={{group=0,slot=0,metal_index=0,size_index=-1,usage=.Storage,mode=.Write,minimum_size=4},{group=0,slot=1,metal_index=1,size_index=-1,usage=.Uniform,mode=.Read,minimum_size=16}}})
    if err==.None { r.fill_pipeline=handle }
    return err
}

@(private="package")
encode_fill_words :: proc(r:^Renderer,slot:^Native_Frame,prepared:^gfx.Prepared_Graph,encoder:^NS.Object,destination:gfx.Resource_Id,offset,size:u64,value:u32)->gfx.Gpu_Error {
    buffer,ok:=resolve_buffer(r,prepared,destination); if !ok { return .Invalid_Resource }
    if value==u32(byte(value))*0x01010101 {
        send(nil,encoder,"fillBuffer:range:value:",buffer.object,NS.Range{NS.UInteger(offset),NS.UInteger(size)},byte(value))
        capture_transfer_buffer(r,slot,cast(^NS.Object)buffer.object,destination,{offset,size},0,value)
        return .None
    }
    if size/4>u64(max(u32)) { return .Invalid_Range }
    entry,present:=gfx.storage_get(&r.pipelines,r.fill_pipeline); if !present { return .Invalid_Resource }
    pipeline:=entry^; retain_pipeline(slot,pipeline)
    slot.capture_pipeline=gfx.capture_object(&r.capture,pipeline.object);slot.capture_layout=gfx.capture_object(&r.capture,&pipeline.desc)
    constants:=[4]u32{u32(size/4),value,0,0}
    parameters:=r.device->newBufferWithLength(16,MTL.ResourceOptions{.HazardTrackingModeUntracked})
    if parameters==nil { return .Allocation_Failed }
    copy(parameters->contents(),mem.slice_to_bytes(constants[:]))
    append(&slot.auxiliary,cast(^NS.Object)parameters); capture_residency(r,slot,cast(^NS.Object)parameters)
    descriptor:=new_object("MTL4ArgumentTableDescriptor"); if descriptor==nil { return .Allocation_Failed }; defer descriptor->release()
    send(nil,descriptor,"setMaxBufferBindCount:",NS.UInteger(2))
    native_error:^NS.Error
    table:=send(^NS.Object,r.device,"newArgumentTableWithDescriptor:error:",descriptor,&native_error)
    if table==nil { return .Allocation_Failed }; append(&slot.tables,table)
    capture_table(r,slot,table,.Compute)
    send(nil,table,"setAddress:atIndex:",buffer.object->gpuAddress()+offset,NS.UInteger(0))
    send(nil,table,"setAddress:atIndex:",parameters->gpuAddress(),NS.UInteger(1))
    capture_buffer_binding(r,slot,table,cast(^NS.Object)buffer.object,{destination,{offset,size},.Write,.Transfer_Destination},0,0,0)
    capture_constant_binding(r,slot,table,cast(^NS.Object)parameters,0,1,1,16,"fill parameters")
    send(nil,encoder,"setComputePipelineState:",pipeline.object); send(nil,encoder,"setArgumentTable:",table)
    capture_emit(r,slot,{kind=.Bind_Pipeline,resource_index= -1,object=gfx.capture_object(&r.capture,pipeline.object),emitted=true,label="word-fill pipeline state"})
    send(nil,encoder,"dispatchThreadgroups:threadsPerThreadgroup:",MTL.Size{NS.Integer((size/4+63)/64),1,1},MTL.Size{64,1,1})
    capture_transfer_buffer(r,slot,cast(^NS.Object)buffer.object,destination,{offset,size},0,value)
    return .None
}
