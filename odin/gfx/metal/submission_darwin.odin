#+build darwin, arm64
//! Submitted commands retain every native object until exact terminal feedback.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:sync"
import "core:log"

@(private="package")
clear_frame :: proc(r:^Renderer,slot:^Native_Frame,submitted:bool) {
    for buffer in slot.buffers { if submitted { buffer.pending-=1; if buffer.heap!=nil { buffer.heap.pending-=1 } }; release_buffer(r,buffer) }
    for pipeline in slot.pipelines { release_pipeline(r,pipeline) }
    for table in slot.tables { table->release() }
    for object in slot.auxiliary { object->release() }
    for texture in slot.textures { if submitted { texture.pending-=1; if texture.heap!=nil { texture.heap.pending-=1 } }; release_texture(r,texture) }
    for pipeline in slot.graphics { release_graphics(r,pipeline) }
    for sampler in slot.samplers { release_sampler(r,sampler) }
    clear(&slot.buffers); clear(&slot.pipelines); clear(&slot.tables); clear(&slot.auxiliary); clear(&slot.textures); clear(&slot.graphics); clear(&slot.samplers)
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
@(private="package")
acquired_token_valid :: proc(r:^Renderer,token:gfx.Frame_Token)->bool {
    if token.owner!=&r.frames || token.slot<0 || token.slot>=len(r.slots) { return false }
    slot:=r.frames.slots[token.slot]
    return slot.state==.Acquired && slot.generation==token.generation
}

/// Acquires an idle slot explicitly; native retirement is chosen by the caller.
acquire :: proc(r:^Renderer)->(gfx.Frame_Token,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    token,err:=gfx.frame_acquire(&r.frames,r.next_slot)
    if err==.Busy { return {},.Busy }
    if err!=.None { return {},.Native_Failure }
    return token,.None
}

/// Abandons an acquisition without publishing exports or advancing the native slot.
abort :: proc(r:^Renderer,token:gfx.Frame_Token)->gfx.Gpu_Error {
    if !acquired_token_valid(r,token) { return .Invalid_Resource }
    if gfx.frame_abort(&r.frames,token)!=.None { return .Invalid_Resource }
    return .None
}

/// Validates and submits authored work without waiting for another frame's completion.
submit :: proc(r:^Renderer,token:gfx.Frame_Token,g:^gfx.Graph,plan:^gfx.Compiled_Graph,inputs:[]gfx.Buffer_Input,textures:[]gfx.Texture_Input)->(gfx.Submission,gfx.Gpu_Error,gfx.Packet_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure,.None }
    if !acquired_token_valid(r,token) { return {},.Invalid_Resource,.None }
    upload_error:=retire_uploads(r,false); if upload_error!=.None { return {},upload_error,.None }
    prepared,preflight:=gfx.graph_prepare(g,plan,inputs,resource_query(r),textures,graphics_query(r))
    if preflight!=.None { return {},.Invalid_Graph,preflight }
    defer gfx.prepared_graph_destroy(&prepared)
    if !content_epochs_available(r,&prepared) { return {},.Native_Failure,.None }
    journals,journal_error:=prepare_texture_journals(r,&prepared)
    if journal_error!=.None { return {},journal_error,.None }
    defer journal_destroy(r,journals)
    if r.frames.next_submission==max(u64) { return {},.Native_Failure,.None }
    index:=token.slot
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
        if buffer.heap!=nil { send(nil,slot.residency,"addAllocation:",buffer.heap.object) }
    }
    for input in prepared.textures {
        entry,ok:=gfx.storage_get(&r.textures,input.handle); if !ok { return {},.Invalid_Resource,.None }
        texture:=entry^; retained:=false
        for previous in slot.textures { if previous==texture { retained=true; break } }
        if retained { continue }
        texture.refs+=1; append(&slot.textures,texture)
        send(nil,slot.residency,"addAllocation:",texture.object)
        if texture.heap!=nil { send(nil,slot.residency,"addAllocation:",texture.heap.object) }
    }
    send(nil,slot.command,"beginCommandBufferWithAllocator:",slot.allocator)
    send(nil,slot.command,"useResidencySet:",slot.residency)
    for pass in prepared.passes {
        if packet,rendering:=pass.packet.(gfx.Render); rendering {
            err:=encode_render(r,slot,&prepared,pass,packet); if err!=.None { return {},err,.None }; continue
        }
        encoder:=send(^NS.Object,slot.command,"computeCommandEncoder")
        if encoder==nil { return {},.Allocation_Failed,.None }
        visibility:=NS.UInteger(1)
        for buffer in slot.buffers { if buffer.heap!=nil { visibility|=2 } }
        for texture in slot.textures { if texture.heap!=nil { visibility|=2 } }
        for alias in prepared.aliases { if alias.after==pass.id { visibility|=2 } }
        send(nil,encoder,"barrierAfterQueueStages:beforeStages:visibilityOptions:",NS.UInteger(max(int)),NS.UInteger(max(int)),visibility)
        #partial switch packet in pass.packet {
        case gfx.Dispatch:
            err:=encode_dispatch(r,slot,&prepared,encoder,packet)
            if err!=.None { send(nil,encoder,"endEncoding"); return {},err,.None }
        case gfx.Fill_Buffer:
            err:=encode_fill_words(r,slot,&prepared,encoder,packet.destination,packet.offset,packet.size,packet.value)
            if err!=.None { send(nil,encoder,"endEncoding"); return {},err,.None }
        case gfx.Generate_Mips:
            err:=encode_mips(r,slot,&prepared,encoder,packet)
            if err!=.None { send(nil,encoder,"endEncoding"); return {},err,.None }
        case gfx.Copy_Image_Buffer:
            err:=encode_image_copy(r,encoder,&prepared,packet)
            if err!=.None { send(nil,encoder,"endEncoding"); return {},err,.None }
        case gfx.Copy_Buffer_Image:
            err:=encode_buffer_image_copy(r,encoder,&prepared,packet)
            if err!=.None { send(nil,encoder,"endEncoding"); return {},err,.None }
        case gfx.Copy_Buffer:
            source,source_ok:=resolve_buffer(r,&prepared,packet.source)
            destination,destination_ok:=resolve_buffer(r,&prepared,packet.destination)
            if !source_ok || !destination_ok { send(nil,encoder,"endEncoding"); return {},.Invalid_Resource,.None }
            send(nil,encoder,"copyFromBuffer:sourceOffset:toBuffer:destinationOffset:size:",source.object,NS.UInteger(packet.source_offset),destination.object,NS.UInteger(packet.destination_offset),NS.UInteger(packet.size))
        }
        send(nil,encoder,"endEncoding")
    }
    send(nil,slot.command,"endCommandBuffer")
    send(nil,slot.residency,"commit"); send(nil,slot.residency,"requestResidency"); slot.resident=true
    if gfx.frame_recorded(&r.frames,token)!=.None { return {},.Native_Failure,.None }
    slot.options=new_object("MTL4CommitOptions"); if slot.options==nil { return {},.Allocation_Failed,.None }
    slot.completion=new(Completion,r.allocator); sync.wait_group_add(&slot.completion.wait_group,1)
    slot.block=NS.Block.createLocalWithParam(slot.completion,feedback)
    if slot.block==nil { return {},.Allocation_Failed,.None }
    send(nil,slot.options,"addFeedbackHandler:",slot.block)
    commands:=[1]^NS.Object{slot.command}
    consumes_surface:=false
    for image in prepared.images {
        if image.input.handle==r.surface_texture && r.surface.drawable!=nil { consumes_surface=true; break }
    }
    if consumes_surface {
        send(nil,r.queue,"waitForDrawable:",r.surface.drawable)
        r.surface.drawable->retain(); append(&slot.auxiliary,cast(^NS.Object)r.surface.drawable)
    }
    for buffer in slot.buffers { buffer.pending+=1; if buffer.heap!=nil { buffer.heap.pending+=1 } }
    for texture in slot.textures { texture.pending+=1; if texture.heap!=nil { texture.heap.pending+=1 } }
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
    if consumes_surface { r.surface.submitted={r,token,submission} }
    commit_content_epochs(r,&prepared)
    for journal in journals { invalidate_heap_content(r,journal.texture.heap,nil) }
    for journal in journals { copy(journal.texture.initialized,journal.initialized) }
    publish_sources(r,&prepared,gfx.Submission{r,token,submission})
    r.next_slot=(index+1)%len(r.slots)
    return {r,token,submission},.None,.None
}
