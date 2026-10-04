#+build darwin, arm64
//! Submitted commands retain every native object until exact terminal feedback.
package metal

import gfx ".."
import MTL "vendor:darwin/Metal"
import NS "core:sys/darwin/Foundation"
import "core:sync"
import "core:log"

@(private="package")
clear_frame :: proc(r:^Renderer,slot:^Native_Frame,submitted:bool) {
    for buffer in slot.buffers { if submitted { buffer.pending-=1 }; release_buffer(r,buffer) }
    for pipeline in slot.pipelines { release_pipeline(r,pipeline) }
    for table in slot.tables { table->release() }
    clear(&slot.buffers); clear(&slot.pipelines); clear(&slot.tables)
    if slot.command!=nil { slot.command->release() }
    if slot.residency!=nil { if slot.resident { send(nil,slot.residency,"endResidency") }; slot.residency->release() }
    if slot.options!=nil { slot.options->release() }
    if slot.block!=nil { slot.block->release() }
    if slot.completion!=nil { free(slot.completion,r.allocator) }
    slot.command=nil; slot.residency=nil; slot.options=nil; slot.block=nil; slot.completion=nil; slot.submission=0; slot.resident=false
}
@(private="package")
retire_slot :: proc(r:^Renderer,slot:^Native_Frame)->gfx.Gpu_Error {
    completion:=slot.completion
    if completion==nil { return .Invalid_Resource }
    sync.mutex_lock(&completion.mutex)
    done,failed:=completion.done,completion.failed
    sync.mutex_unlock(&completion.mutex)
    if !done { return .Busy }
    if failed { log.error("Metal 4 terminal GPU failure",completion.code,string(completion.message[:completion.message_length])); r.failed=true }
    frame_error:=gfx.frame_completed(&r.frames,slot.token,slot.submission)
    clear_frame(r,slot,true)
    if failed || frame_error!=.None { return .Native_Failure }
    return .None
}
@(private="package")
submission_slot :: proc(r:^Renderer,submission:gfx.Submission)->(^Native_Frame,bool) {
    if submission.owner!=r || submission.token.owner!=&r.frames || submission.token.slot<0 || submission.token.slot>=len(r.slots) { return nil,false }
    slot:=&r.slots[submission.token.slot]
    return slot,slot.submission!=0 && slot.submission==submission.id && slot.token==submission.token
}
/// Polls exact terminal feedback; successful retirement permits CPU access and slot reuse.
poll :: proc(r:^Renderer,submission:gfx.Submission)->(bool,gfx.Gpu_Error) {
    slot,ok:=submission_slot(r,submission); if !ok { return false,.Invalid_Resource }
    err:=retire_slot(r,slot)
    if err==.Busy { return false,.None }
    return true,err
}
/// Waits only for this accepted submission and retires its retained native objects.
wait :: proc(r:^Renderer,submission:gfx.Submission)->gfx.Gpu_Error {
    slot,ok:=submission_slot(r,submission); if !ok { return .Invalid_Resource }
    sync.wait_group_wait(&slot.completion.wait_group)
    return retire_slot(r,slot)
}
@(private="package")
retain_pipeline :: proc(slot:^Native_Frame,pipeline:^Native_Pipeline) {
    for previous in slot.pipelines { if previous==pipeline { return } }
    pipeline.refs+=1; append(&slot.pipelines,pipeline)
}
@(private="package")
resolve_buffer :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph,id:gfx.Resource_Id)->(^Native_Buffer,bool) {
    handle,found:=gfx.prepared_buffer(prepared,id); if !found { return nil,false }
    buffer,ok:=gfx.storage_get(&r.buffers,handle); if !ok { return nil,false }
    return buffer^,true
}
/// Validates and submits authored work without waiting for another frame's completion.
submit :: proc(r:^Renderer,g:^gfx.Buffer_Graph,plan:^gfx.Compiled_Graph,inputs:[]gfx.Buffer_Input)->(gfx.Submission,gfx.Gpu_Error,gfx.Packet_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure,.None }
    prepared,preflight:=gfx.graph_prepare(g,plan,inputs,resource_query(r))
    if preflight!=.None { return {},.Invalid_Graph,preflight }
    defer gfx.prepared_graph_destroy(&prepared)
    if r.frames.next_submission==max(u64) { return {},.Native_Failure,.None }
    index:=r.next_slot
    token,acquire_error:=gfx.frame_acquire(&r.frames,index)
    if acquire_error!=.None {
        if acquire_error==.Busy { return {},.Busy,.None }
        r.failed=true; log.error("Metal frame acquisition failed",acquire_error); return {},.Native_Failure,.None
    }
    slot:=&r.slots[index]; slot.token=token
    committed:=false
    defer {
        if !committed { clear_frame(r,slot,false); gfx.frame_abort(&r.frames,token) }
    }
    send(nil,slot.allocator,"reset")
    slot.command=send(^NS.Object,r.device,"newCommandBuffer")
    if slot.command==nil { return {},.Allocation_Failed,.None }
    descriptor:=new_object("MTLResidencySetDescriptor"); if descriptor==nil { return {},.Allocation_Failed,.None }; defer descriptor->release()
    native_error:^NS.Error
    slot.residency=send(^NS.Object,r.device,"newResidencySetWithDescriptor:error:",descriptor,&native_error)
    if slot.residency==nil { report_error(native_error,"Metal residency allocation failed"); return {},.Allocation_Failed,.None }
    for input in prepared.buffers {
        entry,ok:=gfx.storage_get(&r.buffers,input.handle); if !ok { return {},.Invalid_Resource,.None }
        buffer:=entry^; buffer.refs+=1; append(&slot.buffers,buffer)
        send(nil,slot.residency,"addAllocation:",buffer.object)
    }
    send(nil,slot.residency,"commit"); send(nil,slot.residency,"requestResidency"); slot.resident=true
    send(nil,slot.command,"beginCommandBufferWithAllocator:",slot.allocator)
    send(nil,slot.command,"useResidencySet:",slot.residency)
    for pass in prepared.passes {
        encoder:=send(^NS.Object,slot.command,"computeCommandEncoder")
        if encoder==nil { return {},.Allocation_Failed,.None }
        after:u64
        for hazard in prepared.hazards {
            if hazard.after!=pass.id { continue }
            for source in prepared.passes { if source.id==hazard.before { after|=u64(1<<27) if source.kind==.Compute else u64(1<<28); break } }
        }
        // Earlier submissions on this queue can reference the same persistent allocation.
        if after==0 { after=u64((1<<27)|(1<<28)) }
        stage:=u64(1<<27) if pass.kind==.Compute else u64(1<<28)
        send(nil,encoder,"barrierAfterQueueStages:beforeStages:visibilityOptions:",after,stage,NS.UInteger(1))
        switch packet in pass.packet {
        case gfx.Dispatch:
            entry,ok:=gfx.storage_get(&r.pipelines,packet.pipeline); if !ok { send(nil,encoder,"endEncoding"); return {},.Invalid_Resource,.None }
            pipeline:=entry^; retain_pipeline(slot,pipeline)
            table_desc:=new_object("MTL4ArgumentTableDescriptor"); if table_desc==nil { send(nil,encoder,"endEncoding"); return {},.Allocation_Failed,.None }
            count:u32
            for binding in packet.bindings { count=max(count,binding.slot+1) }
            send(nil,table_desc,"setMaxBufferBindCount:",NS.UInteger(count))
            table:=send(^NS.Object,r.device,"newArgumentTableWithDescriptor:error:",table_desc,&native_error)
            table_desc->release()
            if table==nil { send(nil,encoder,"endEncoding"); return {},.Allocation_Failed,.None }
            append(&slot.tables,table)
            for binding in packet.bindings {
                buffer,present:=resolve_buffer(r,&prepared,binding.access.resource)
                if !present { send(nil,encoder,"endEncoding"); return {},.Invalid_Resource,.None }
                send(nil,table,"setAddress:atIndex:",buffer.object->gpuAddress()+binding.access.range.offset,NS.UInteger(binding.slot))
            }
            send(nil,encoder,"setComputePipelineState:",pipeline.object); send(nil,encoder,"setArgumentTable:",table)
            groups:=MTL.Size{NS.Integer(packet.groups[0]),NS.Integer(packet.groups[1]),NS.Integer(packet.groups[2])}
            local:=MTL.Size{NS.Integer(pipeline.local_size[0]),NS.Integer(pipeline.local_size[1]),NS.Integer(pipeline.local_size[2])}
            send(nil,encoder,"dispatchThreadgroups:threadsPerThreadgroup:",groups,local)
        case gfx.Copy_Buffer:
            source,source_ok:=resolve_buffer(r,&prepared,packet.source)
            destination,destination_ok:=resolve_buffer(r,&prepared,packet.destination)
            if !source_ok || !destination_ok { send(nil,encoder,"endEncoding"); return {},.Invalid_Resource,.None }
            send(nil,encoder,"copyFromBuffer:sourceOffset:toBuffer:destinationOffset:size:",source.object,NS.UInteger(packet.source_offset),destination.object,NS.UInteger(packet.destination_offset),NS.UInteger(packet.size))
        }
        send(nil,encoder,"endEncoding")
    }
    send(nil,slot.command,"endCommandBuffer")
    if gfx.frame_recorded(&r.frames,token)!=.None { return {},.Native_Failure,.None }
    slot.options=new_object("MTL4CommitOptions"); if slot.options==nil { return {},.Allocation_Failed,.None }
    slot.completion=new(Completion,r.allocator); sync.wait_group_add(&slot.completion.wait_group,1)
    slot.block=NS.Block.createLocalWithParam(slot.completion,feedback)
    if slot.block==nil { return {},.Allocation_Failed,.None }
    send(nil,slot.options,"addFeedbackHandler:",slot.block)
    commands:=[1]^NS.Object{slot.command}
    for buffer in slot.buffers { buffer.pending+=1 }
    send(nil,r.queue,"commit:count:options:",raw_data(commands[:]),NS.UInteger(1),slot.options)
    submission,frame_error:=gfx.frame_submitted(&r.frames,token)
    slot.submission=submission; committed=true
    if frame_error!=.None {
        sync.wait_group_wait(&slot.completion.wait_group)
        sync.mutex_lock(&slot.completion.mutex); sync.mutex_unlock(&slot.completion.mutex)
        clear_frame(r,slot,true); gfx.frame_abort(&r.frames,token)
        r.failed=true; log.error("Accepted Metal submission lost its frame token")
        return {},.Native_Failure,.None
    }
    r.next_slot=(index+1)%len(r.slots)
    return {r,token,submission},.None,.None
}
