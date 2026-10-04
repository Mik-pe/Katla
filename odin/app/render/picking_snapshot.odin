//! Paired image copies publish only with matching committed submission and immutable entity map.
package render

import gfx "../../gfx"

/// Queues both sources before another submission may overwrite either image.
picking_queue :: proc(renderer:^$R,ops:Picking_Ops(R),submission:gfx.Submission,color,id:gfx.Image_Id,metadata:Picking_Metadata,entries:[]Picking_Entry,allocator:=context.allocator)->(Picking_Capture,gfx.Gpu_Error) {
    if ops.source==nil || ops.queue==nil || ops.poll==nil || ops.destroy==nil || submission.owner==nil { return {},.Unsupported }
    for entry,i in entries {
        if entry.encoded==0 { return {},.Invalid_Resource }
        for earlier in entries[:i] { if entry.encoded==earlier.encoded { return {},.Invalid_Resource } }
    }
    color_source,error:=ops.source(renderer,submission,color); if error!=.None { return {},error }
    id_source:gfx.Texture_Source
    id_source,error=ops.source(renderer,submission,id); if error!=.None { return {},error }
    if color_source.submission!=submission || id_source.submission!=submission || color_source.desc.width!=id_source.desc.width || color_source.desc.height!=id_source.desc.height || metadata.width!=id_source.desc.width || metadata.height!=id_source.desc.height || id_source.desc.format!=.R32_Uint || (color_source.desc.format!=.RGBA8_Unorm && color_source.desc.format!=.BGRA8_Unorm) || color_source.desc.depth!=1 || id_source.desc.depth!=1 { return {},.Invalid_Resource }
    capture:=Picking_Capture{metadata=metadata,submission=submission,color_source=color_source,id_source=id_source,entries=make([]Picking_Entry,len(entries),allocator),allocator=allocator}
    copy(capture.entries,entries)
    region:=gfx.Image_Region{width=metadata.width,height=metadata.height,aspect=.Color,depth=1}
    capture.color_ticket,error=ops.queue(renderer,color_source,region)
    if error!=.None { delete(capture.entries,allocator); return {},error }
    capture.id_ticket,error=ops.queue(renderer,id_source,region)
    if error!=.None {
        cleanup:=ops.destroy(renderer,capture.color_ticket)
        if cleanup!=.None { return capture,cleanup }
        delete(capture.entries,allocator); return {},error
    }
    return capture,.None
}
/// Transfers the paired snapshot exactly once; partial completion stays capture-owned.
picking_poll :: proc(renderer:^$R,ops:Picking_Ops(R),capture:^Picking_Capture)->(Picking_Snapshot,bool,gfx.Gpu_Error) {
    if capture==nil || capture.submission.owner==nil || ops.poll==nil { return {},false,.Invalid_Resource }
    if capture.color_ticket.owner!=nil {
        data,complete,error:=ops.poll(renderer,capture.color_ticket)
        if error!=.None { return {},false,error }
        if complete { capture.color=data; capture.color_ticket={} }
    }
    if capture.id_ticket.owner!=nil {
        data,complete,error:=ops.poll(renderer,capture.id_ticket)
        if error!=.None { return {},false,error }
        if complete { capture.id_pixels=data; capture.id_ticket={} }
    }
    if capture.color_ticket.owner!=nil || capture.id_ticket.owner!=nil { return {},false,.None }
    if capture.color.source!=capture.color_source || capture.id_pixels.source!=capture.id_source || !picking_pixels_valid(capture.color,capture.metadata) || !picking_pixels_valid(capture.id_pixels,capture.metadata) { return {},false,.Invalid_Resource }
    snapshot:=Picking_Snapshot{capture.metadata,capture.submission,capture.color_source,capture.id_source,capture.color,capture.id_pixels,capture.entries,capture.allocator}
    capture^={}; return snapshot,true,.None
}
@(private="package")
picking_pixels_valid :: proc(data:gfx.Readback_Data,metadata:Picking_Metadata)->bool {
    if metadata.width==0 || metadata.height==0 || data.row_pitch<u64(metadata.width)*4 { return false }
    row_bytes:=u64(metadata.width)*4
    if u64(len(data.bytes))<row_bytes { return false }
    return metadata.height==1 || data.row_pitch<=(u64(len(data.bytes))-row_bytes)/u64(metadata.height-1)
}
/// Returns the foremost committed object-ID sample at native image-row coordinates.
picking_sample :: proc(snapshot:^Picking_Snapshot,x,y:i32)->Picking_Sample {
    if snapshot==nil || x<0 || y<0 || u32(x)>=snapshot.metadata.width || u32(y)>=snapshot.metadata.height || !picking_pixels_valid(snapshot.id_pixels,snapshot.metadata) { return {} }
    offset:=int(u64(y)*snapshot.id_pixels.row_pitch+u64(x)*4)
    bytes:=snapshot.id_pixels.bytes[offset:offset+4]
    encoded:=u32(bytes[0])|u32(bytes[1])<<8|u32(bytes[2])<<16|u32(bytes[3])<<24
    for entry in snapshot.entries { if entry.encoded==encoded { return {encoded,entry.entity,true} } }
    return {encoded=encoded}
}
/// Releases pending native copies and any already completed half without abandoning GPU owners.
picking_capture_destroy :: proc(renderer:^$R,ops:Picking_Ops(R),capture:^Picking_Capture)->gfx.Gpu_Error {
    if capture==nil { return .Invalid_Resource }
    if capture.color_ticket.owner!=nil { error:=ops.destroy(renderer,capture.color_ticket); if error!=.None { return error }; capture.color_ticket={} }
    if capture.id_ticket.owner!=nil { error:=ops.destroy(renderer,capture.id_ticket); if error!=.None { return error }; capture.id_ticket={} }
    gfx.readback_data_destroy(&capture.color); gfx.readback_data_destroy(&capture.id_pixels)
    delete(capture.entries,capture.allocator); capture^={}; return .None
}
/// Retained CPU pixels and mapping outlive renderer resize and newer snapshots.
picking_snapshot_destroy :: proc(snapshot:^Picking_Snapshot) {
    if snapshot==nil { return }
    gfx.readback_data_destroy(&snapshot.color); gfx.readback_data_destroy(&snapshot.id_pixels)
    delete(snapshot.entries,snapshot.allocator); snapshot^={}
}
