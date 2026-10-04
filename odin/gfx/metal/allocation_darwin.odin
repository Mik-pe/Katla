#+build darwin, arm64
//! Compiled lifetime groups map to real native placement heaps and domain-specific storage.
package metal

import gfx ".."
import MTL "vendor:darwin/Metal"
import NS "core:sys/darwin/Foundation"
import "core:sync"

@(private="package")
buffer_options :: proc(domain:gfx.Memory_Domain)->MTL.ResourceOptions {
    options:=MTL.ResourceOptions{.HazardTrackingModeUntracked}
    if domain==.GPU_Private { options|={.StorageModePrivate} }
    return options
}

@(private="package")
allocation_buffer_requirements :: proc(state:rawptr,desc:gfx.Buffer_Desc)->(gfx.Memory_Requirements,gfx.Gpu_Error) {
    r:=cast(^Renderer)state
    if r.device==nil || r.failed { return {},.Native_Failure }
    if desc.size==0 || desc.usage=={} || desc.size>u64(send(NS.UInteger,r.device,"maxBufferLength")) { return {},.Invalid_Range }
    size,alignment:=r.device->heapBufferSizeAndAlignWithLength(NS.UInteger(desc.size),buffer_options(desc.memory))
    return {u64(size),u64(alignment),1 if desc.memory==.CPU_Visible else 2,desc.memory},.None
}

@(private="package")
allocation_texture_requirements :: proc(state:rawptr,desc:gfx.Texture_Desc)->(gfx.Memory_Requirements,gfx.Gpu_Error) {
    r:=cast(^Renderer)state
    if r.device==nil || r.failed { return {},.Native_Failure }
    if !texture_supported(r,desc) { return {},.Unsupported }
    if !gfx.texture_desc_valid(desc) || desc.width>16384 || desc.height>16384 || desc.layers>2048 || desc.depth>2048 || (desc.depth>1 && (desc.width>2048 || desc.height>2048)) || .Present in desc.usage { return {},.Invalid_Range }
    descriptor:=texture_descriptor(desc); if descriptor==nil { return {},.Allocation_Failed }; defer descriptor->release()
    size,alignment:=r.device->heapTextureSizeAndAlignWithDescriptor(descriptor)
    return {u64(size),u64(alignment),2,.GPU_Private},.None
}

/// Exposes actual native size, alignment and storage compatibility to the generic planner.
allocation_query :: proc(r:^Renderer)->gfx.Allocation_Query { return {r,allocation_buffer_requirements,allocation_texture_requirements} }

@(private="package")
allocate_group :: proc(state:rawptr,group:gfx.Allocation_Group,requests:[]gfx.Allocation_Request)->(gfx.Allocation_Result,gfx.Gpu_Error) {
    r:=cast(^Renderer)state
    if r.device==nil || r.failed { return {},.Native_Failure }
    if len(requests)==0 || group.size==0 || group.alignment==0 { return {},.Invalid_Range }
    for request in requests {
        requirement:gfx.Memory_Requirements; err:gfx.Gpu_Error
        switch resource in request {
        case gfx.Buffer_Allocation: requirement,err=allocation_buffer_requirements(r,resource.desc)
        case gfx.Image_Allocation: requirement,err=allocation_texture_requirements(r,resource.desc)
        }
        if err!=.None { return {},err }
        if requirement.domain!=group.domain || requirement.memory_types&group.memory_types==0 || requirement.size>group.size || group.alignment%requirement.alignment!=0 { return {},.Invalid_Range }
    }
    descriptor:=MTL.HeapDescriptor.alloc()->init(); if descriptor==nil { return {},.Allocation_Failed }; defer descriptor->release()
    descriptor->setType(.Placement); descriptor->setSize(NS.UInteger(group.size)); descriptor->setHazardTrackingMode(.Untracked)
    descriptor->setStorageMode(.Shared if group.domain==.CPU_Visible else .Private)
    object:=send(^MTL.Heap,r.device,"newHeapWithDescriptor:",descriptor); if object==nil { return {},.Allocation_Failed }
    heap:=new(Native_Heap,r.allocator); heap^={object=object,refs=1}; defer release_heap(r,heap)
    handles:=make([]gfx.Allocation_Handle,len(requests),r.allocator)
    success:=false
    defer {
        if !success {
            for handle in handles {
                #partial switch h in handle {
                case gfx.Buffer_Handle: if h.owner!=nil { destroy_buffer(r,h) }
                case gfx.Texture_Handle: if h.owner!=nil { destroy_texture(r,h) }
                }
            }
            delete(handles,r.allocator)
        }
    }
    for request,i in requests {
        switch resource in request {
        case gfx.Buffer_Allocation:
            native:=send(^MTL.Buffer,object,"newBufferWithLength:options:offset:",NS.UInteger(resource.desc.size),buffer_options(resource.desc.memory),NS.UInteger(0))
            if native==nil { return {},.Allocation_Failed }
            buffer:=new(Native_Buffer,r.allocator); buffer^={object=native,desc=resource.desc,refs=1,heap=heap}; heap.refs+=1
            handles[i]=gfx.storage_insert(&r.buffers,buffer)
        case gfx.Image_Allocation:
            texture_desc:=texture_descriptor(resource.desc); if texture_desc==nil { return {},.Allocation_Failed }
            native:=send(^MTL.Texture,object,"newTextureWithDescriptor:offset:",texture_desc,NS.UInteger(0)); texture_desc->release()
            if native==nil { return {},.Allocation_Failed }
            texture:=new(Native_Texture,r.allocator); texture^={object=native,desc=resource.desc,refs=1,heap=heap}; heap.refs+=1
            new_texture_content(r,texture)
            handles[i]=gfx.storage_insert(&r.textures,texture)
        }
    }
    success=true
    return {handles,r.allocator},.None
}

@(private="package")
allocation_destroy_buffer :: proc(state:rawptr,handle:gfx.Buffer_Handle)->gfx.Gpu_Error { return destroy_buffer(cast(^Renderer)state,handle) }
@(private="package")
allocation_destroy_texture :: proc(state:rawptr,handle:gfx.Texture_Handle)->gfx.Gpu_Error { return destroy_texture(cast(^Renderer)state,handle) }
/// Allocates and removes typed identities while native submissions retain physical heap ownership.
allocation_api :: proc(r:^Renderer)->gfx.Allocation_API { return {r,allocate_group,allocation_destroy_buffer,allocation_destroy_texture} }

@(private="package")
enqueue_buffer_upload :: proc(r:^Renderer,buffer:^Native_Buffer,initial:[]byte)->(^Native_Texture_Transfer,gfx.Gpu_Error) {
    readback:=new(Native_Texture_Transfer,r.allocator)
    readback.buffer_owner=buffer; buffer.refs+=1
    success:=false; defer { if !success { release_texture_transfer(r,readback) } }
    readback.row_pitch=u64(len(initial))
    bytes:=readback.row_pitch
    if bytes==0 || bytes>u64(max(int)) { return nil,.Invalid_Range }
    readback.destination=r.device->newBufferWithLength(NS.UInteger(bytes),MTL.ResourceOptions{.HazardTrackingModeUntracked})
    if readback.destination==nil { return nil,.Allocation_Failed }
    if len(initial)>0 { copy(readback.destination->contents()[:len(initial)],initial) }
    readback.allocator=send(^NS.Object,r.device,"newCommandAllocator")
    if readback.allocator==nil { return nil,.Allocation_Failed }
    readback.command=send(^NS.Object,r.device,"newCommandBuffer")
    if readback.command==nil { return nil,.Allocation_Failed }
    descriptor:=new_object("MTLResidencySetDescriptor")
    if descriptor==nil { return nil,.Allocation_Failed }; defer descriptor->release()
    native_error:^NS.Error
    readback.residency=send(^NS.Object,r.device,"newResidencySetWithDescriptor:error:",descriptor,&native_error)
    if readback.residency==nil { return nil,.Allocation_Failed }
    send(nil,readback.residency,"addAllocation:",buffer.object)
    if buffer.heap!=nil { send(nil,readback.residency,"addAllocation:",buffer.heap.object) }
    send(nil,readback.residency,"addAllocation:",readback.destination)
    send(nil,readback.residency,"commit"); send(nil,readback.residency,"requestResidency"); readback.resident=true
    send(nil,readback.command,"beginCommandBufferWithAllocator:",readback.allocator)
    send(nil,readback.command,"useResidencySet:",readback.residency)
    encoder:=send(^NS.Object,readback.command,"computeCommandEncoder")
    if encoder==nil { send(nil,readback.command,"endCommandBuffer"); return nil,.Allocation_Failed }
    send(nil,encoder,"barrierAfterQueueStages:beforeStages:visibilityOptions:",NS.UInteger(max(int)),NS.UInteger(1<<28),NS.UInteger(1))
    send(nil,encoder,"copyFromBuffer:sourceOffset:toBuffer:destinationOffset:size:",readback.destination,NS.UInteger(0),buffer.object,NS.UInteger(0),NS.UInteger(bytes))
    send(nil,encoder,"endEncoding"); send(nil,readback.command,"endCommandBuffer")
    readback.options=new_object("MTL4CommitOptions")
    if readback.options==nil { return nil,.Allocation_Failed }
    readback.completion=new(Completion,r.allocator); sync.wait_group_add(&readback.completion.wait_group,1)
    readback.block=NS.Block.createLocalWithParam(readback.completion,feedback)
    if readback.block==nil { return nil,.Allocation_Failed }
    send(nil,readback.options,"addFeedbackHandler:",readback.block)
    commands:=[1]^NS.Object{readback.command}
    buffer.pending+=1; if buffer.heap!=nil { buffer.heap.pending+=1 }
    send(nil,r.queue,"commit:count:options:",raw_data(commands[:]),NS.UInteger(1),readback.options)
    readback.submitted=true; success=true
    return readback,.None
}

@(private="package")
initialize_private_buffer :: proc(r:^Renderer,buffer:^Native_Buffer,data:[]byte)->gfx.Gpu_Error {
    transfer,err:=enqueue_buffer_upload(r,buffer,data); if err!=.None { return err }
    sync.wait_group_wait(&transfer.completion.wait_group)
    return release_texture_transfer(r,transfer)
}
