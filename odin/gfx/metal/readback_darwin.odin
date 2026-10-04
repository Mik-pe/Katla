#+build darwin, arm64
//! Accepted image exports and readback copies own independent native completion lifetimes.
package metal

import gfx ".."
import MTL "vendor:darwin/Metal"
import NS "core:sys/darwin/Foundation"
import "core:sync"
import "core:log"

@(private="package")
Published_Source :: struct { source:gfx.Texture_Source, texture:^Native_Texture }
@(private="package")
Native_Texture_Transfer :: struct {
    source:gfx.Texture_Source,
    region:gfx.Image_Region,
    texture:^Native_Texture,
    buffer_owner:^Native_Buffer,
    destination:^MTL.Buffer,
    allocator,command,residency,options:^NS.Object,
    block:^NS.Block,
    completion:^Completion,
    row_pitch:u64,
    layout:gfx.Image_Layout,
    submitted,retired,failed,resident:bool,
}

@(private="package")
publish_sources :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph,submission:gfx.Submission) {
    for image in prepared.images {
        texture,ok:=resolve_texture(r,prepared,image.input.resource); assert(ok)
        if !image.exported { continue }
        source:=gfx.Texture_Source{r,image.input.resource,image.input.handle,submission,texture_epoch(texture),image.desc}
        replaced:=false
        for &published in r.exports {
            if published.source.resource!=source.resource || published.source.submission.token.slot!=submission.token.slot { continue }
            release_texture(r,published.texture)
            texture.refs+=1; published={source,texture}; replaced=true; break
        }
        if !replaced { texture.refs+=1; append(&r.exports,Published_Source{source,texture}) }
    }
}

/// Returns only the exact committed export belonging to the requested accepted submission.
graph_texture_source :: proc(r:^Renderer,submission:gfx.Submission,resource:gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error) {
    if submission.owner!=r || submission.token.owner!=&r.frames || submission.id==0 { return {},.Invalid_Resource }
    for published in r.exports {
        if published.source.resource==resource && published.source.submission==submission {
            if texture_epoch(published.texture)!=published.source.generation { return {},.Invalid_Resource }
            return published.source,.None
        }
    }
    return {},.Invalid_Resource
}

@(private="package")
retire_texture_transfer :: proc(r:^Renderer,readback:^Native_Texture_Transfer)->gfx.Gpu_Error {
    if readback.retired { return .Native_Failure if readback.failed else .None }
    completion:=readback.completion
    if completion==nil { return .Invalid_Resource }
    sync.mutex_lock(&completion.mutex)
    done,failed:=completion.done,completion.failed
    sync.mutex_unlock(&completion.mutex)
    if !done { return .Busy }
    readback.retired=true; readback.failed=failed
    if readback.texture!=nil { readback.texture.pending-=1; if readback.texture.heap!=nil { readback.texture.heap.pending-=1 } }
    if readback.buffer_owner!=nil { readback.buffer_owner.pending-=1; if readback.buffer_owner.heap!=nil { readback.buffer_owner.heap.pending-=1 } }
    if failed { log.error("Metal transfer GPU failure",completion.code,string(completion.message[:completion.message_length])); r.failed=true; return .Native_Failure }
    return .None
}

@(private="package")
release_texture_transfer :: proc(r:^Renderer,readback:^Native_Texture_Transfer)->gfx.Gpu_Error {
    outcome:=gfx.Gpu_Error.None
    if readback.submitted && !readback.retired { sync.wait_group_wait(&readback.completion.wait_group); outcome=retire_texture_transfer(r,readback) }
    if readback.texture!=nil { release_texture(r,readback.texture) }
    if readback.buffer_owner!=nil { release_buffer(r,readback.buffer_owner) }
    if readback.destination!=nil { readback.destination->release() }
    if readback.command!=nil { readback.command->release() }
    if readback.allocator!=nil { readback.allocator->release() }
    if readback.residency!=nil { if readback.resident { send(nil,readback.residency,"endResidency") }; readback.residency->release() }
    if readback.options!=nil { readback.options->release() }
    if readback.block!=nil { readback.block->release() }
    if readback.completion!=nil { free(readback.completion,r.allocator) }
    free(readback,r.allocator)
    return outcome
}

/// Queues a source-bound copy immediately; resize, handle removal and slot reuse cannot replace its pixels.
queue_texture_readback :: proc(r:^Renderer,source:gfx.Texture_Source,region:gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    if source.owner!=r || !gfx.image_region_valid(region,source.desc) || !(.Transfer_Source in source.desc.usage) { return {},.Invalid_Range }
    texture:^Native_Texture
    for published in r.exports { if published.source==source { texture=published.texture; break } }
    if texture==nil || texture_epoch(texture)!=source.generation { return {},.Invalid_Resource }
    if !content_known(texture.initialized,texture.desc,gfx.image_region_range(region)) { return {},.Invalid_Graph }
        transfer,err:=enqueue_texture_transfer(r,texture,region,nil)
    if err!=.None { return {},err }
    transfer.source=source
    return gfx.storage_insert(&r.readbacks,transfer),.None
}

@(private="package")
enqueue_texture_transfer :: proc(r:^Renderer,texture:^Native_Texture,region:gfx.Image_Region,initial:[]byte)->(^Native_Texture_Transfer,gfx.Gpu_Error) {
    readback:=new(Native_Texture_Transfer,r.allocator)
    readback.region=region; readback.texture=texture; texture.refs+=1
    success:=false; defer { if !success { release_texture_transfer(r,readback) } }
    layout,valid:=gfx.image_region_layout(region,texture.desc); if !valid { return nil,.Invalid_Range }
    readback.layout=layout; readback.row_pitch=layout.bytes_per_row
    bytes:=layout.required_bytes
    if bytes==0 || bytes>u64(max(int)) { return nil,.Invalid_Range }
    readback.destination=r.device->newBufferWithLength(NS.UInteger(bytes),MTL.ResourceOptions{.HazardTrackingModeUntracked})
    if readback.destination==nil { return nil,.Allocation_Failed }
    if len(initial)>0 { copy(readback.destination->contents()[:len(initial)],initial) }
    else { for &value in readback.destination->contents()[:int(bytes)] { value=0 } }
    readback.allocator=send(^NS.Object,r.device,"newCommandAllocator")
    if readback.allocator==nil { return nil,.Allocation_Failed }
    readback.command=send(^NS.Object,r.device,"newCommandBuffer")
    if readback.command==nil { return nil,.Allocation_Failed }
    descriptor:=new_object("MTLResidencySetDescriptor")
    if descriptor==nil { return nil,.Allocation_Failed }; defer descriptor->release()
    native_error:^NS.Error
    readback.residency=send(^NS.Object,r.device,"newResidencySetWithDescriptor:error:",descriptor,&native_error)
    if readback.residency==nil { return nil,.Allocation_Failed }
    send(nil,readback.residency,"addAllocation:",texture.object)
    if texture.heap!=nil { send(nil,readback.residency,"addAllocation:",texture.heap.object) }
    send(nil,readback.residency,"addAllocation:",readback.destination)
    send(nil,readback.residency,"commit"); send(nil,readback.residency,"requestResidency"); readback.resident=true
    send(nil,readback.command,"beginCommandBufferWithAllocator:",readback.allocator)
    send(nil,readback.command,"useResidencySet:",readback.residency)
    encoder:=send(^NS.Object,readback.command,"computeCommandEncoder")
    if encoder==nil { send(nil,readback.command,"endCommandBuffer"); return nil,.Allocation_Failed }
    send(nil,encoder,"barrierAfterQueueStages:beforeStages:visibilityOptions:",NS.UInteger(max(int)),NS.UInteger(1<<28),NS.UInteger(1))
    if len(initial)>0 {
        send(nil,encoder,"copyFromBuffer:sourceOffset:sourceBytesPerRow:sourceBytesPerImage:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:options:",readback.destination,NS.UInteger(0),NS.UInteger(readback.row_pitch),NS.UInteger(layout.bytes_per_image),MTL.Size{NS.Integer(region.width),NS.Integer(region.height),NS.Integer(region.depth)},texture.object,NS.UInteger(region.layer),NS.UInteger(region.mip),MTL.Origin{NS.Integer(region.x),NS.Integer(region.y),NS.Integer(region.z)},image_copy_options(texture.desc.format,region.aspect))
    } else {
    send(nil,encoder,"copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toBuffer:destinationOffset:destinationBytesPerRow:destinationBytesPerImage:options:",texture.object,NS.UInteger(region.layer),NS.UInteger(region.mip),MTL.Origin{NS.Integer(region.x),NS.Integer(region.y),NS.Integer(region.z)},MTL.Size{NS.Integer(region.width),NS.Integer(region.height),NS.Integer(region.depth)},readback.destination,NS.UInteger(0),NS.UInteger(readback.row_pitch),NS.UInteger(layout.bytes_per_image),image_copy_options(texture.desc.format,region.aspect))
    }
    send(nil,encoder,"endEncoding"); send(nil,readback.command,"endCommandBuffer")
    readback.options=new_object("MTL4CommitOptions")
    if readback.options==nil { return nil,.Allocation_Failed }
    readback.completion=new(Completion,r.allocator); sync.wait_group_add(&readback.completion.wait_group,1)
    readback.block=NS.Block.createLocalWithParam(readback.completion,feedback)
    if readback.block==nil { return nil,.Allocation_Failed }
    send(nil,readback.options,"addFeedbackHandler:",readback.block)
    commands:=[1]^NS.Object{readback.command}
    texture.pending+=1; if texture.heap!=nil { texture.heap.pending+=1 }
    send(nil,r.queue,"commit:count:options:",raw_data(commands[:]),NS.UInteger(1),readback.options)
    readback.submitted=true; success=true
    return readback,.None
}

/// Polls without waiting and transfers completed tightly packed bytes exactly once.
poll_texture_readback :: proc(r:^Renderer,ticket:gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error) {
    entry,ok:=gfx.storage_get(&r.readbacks,ticket); if !ok { return {},false,.Invalid_Resource }
    readback:=entry^
    err:=retire_texture_transfer(r,readback)
    if err==.Busy { return {},false,.None }
    if err!=.None { destroy_readback(r,ticket); return {},true,err }
    size:=int(readback.layout.required_bytes)
    bytes:=make([]byte,size,r.allocator)
    copy(bytes,readback.destination->contents()[:size])
    result:=gfx.Readback_Data{source=readback.source,region=readback.region,row_pitch=readback.row_pitch,image_pitch=readback.layout.bytes_per_image,bytes=bytes,allocator=r.allocator}
    destroy_readback(r,ticket)
    return result,true,.None
}

/// Cancels CPU delivery while joining any accepted native copy before releasing its resources.
destroy_readback :: proc(r:^Renderer,ticket:gfx.Readback_Ticket)->gfx.Gpu_Error {
    readback,ok:=gfx.storage_remove(&r.readbacks,ticket); if !ok { return .Invalid_Resource }
    return release_texture_transfer(r,readback)
}

@(private="package")
retire_uploads :: proc(r:^Renderer,join:bool)->gfx.Gpu_Error {
    outcome:=gfx.Gpu_Error.None
    i:=0
    for i<len(r.uploads) {
        upload:=r.uploads[i]
        if join && !upload.retired { sync.wait_group_wait(&upload.completion.wait_group) }
        err:=retire_texture_transfer(r,upload)
        if err==.Busy { i+=1; continue }
        if err!=.None { outcome=err }
        release_texture_transfer(r,upload)
        unordered_remove(&r.uploads,i)
    }
    return outcome
}

/// Uploads tightly packed bytes through an independently retained accepted GPU copy.
upload_texture :: proc(r:^Renderer,handle:gfx.Texture_Handle,region:gfx.Image_Region,bytes:[]byte)->gfx.Gpu_Error {
    if r.device==nil || r.failed { return .Native_Failure }
    if r.content_epoch==max(u64) { return .Native_Failure }
    err:=retire_uploads(r,false); if err!=.None { return err }
    entry,ok:=gfx.storage_get(&r.textures,handle); if !ok { return .Invalid_Resource }
    texture:=entry^
    if !(.Transfer_Destination in texture.desc.usage) || !gfx.image_region_valid(region,texture.desc) { return .Invalid_Range }
    layout,valid:=gfx.image_region_layout(region,texture.desc)
    if !valid || layout.required_bytes!=u64(len(bytes)) { return .Invalid_Range }
    transfer,copy_error:=enqueue_texture_transfer(r,texture,region,bytes)
    if copy_error!=.None { return copy_error }
    mark_texture_written(r,texture)
    width,height,depth:=gfx.texture_mip_volume(texture.desc,region.mip)
    if region.x==0 && region.y==0 && region.width==width && region.height==height && region.z==0 && region.depth==depth { set_content(texture.initialized,texture.desc,gfx.image_region_range(region),true) }
    append(&r.uploads,transfer)
    return .None
}

/// Creates and initializes one single-mip, single-layer color image through the native queue.
create_texture_with_data :: proc(r:^Renderer,desc:gfx.Texture_Desc,bytes:[]byte)->(gfx.Texture_Handle,gfx.Gpu_Error) {
    if desc.mip_levels!=1 || desc.layers!=1 || gfx.texture_aspects(desc.format)!={.Color} { return {},.Invalid_Range }
    handle,err:=create_texture(r,desc); if err!=.None { return {},err }
    err=upload_texture(r,handle,{0,0,0,0,desc.width,desc.height,.Color,0,desc.depth,0,0},bytes)
    if err!=.None { destroy_texture(r,handle); return {},err }
    return handle,.None
}

/// Removes one graph's published sources; already queued tickets retain independent native owners.
release_graph_exports :: proc(r:^Renderer,g:^gfx.Graph)->gfx.Gpu_Error {
    i:=0
    for i<len(r.exports) {
        if r.exports[i].source.resource.owner!=g { i+=1; continue }
        release_texture(r,r.exports[i].texture)
        unordered_remove(&r.exports,i)
    }
    return .None
}
