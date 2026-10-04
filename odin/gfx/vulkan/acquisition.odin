//! Mutable uploads require the exact CPU acquisition before command recording.
package katla_vulkan

import gfx ".."

@(private="package")
valid_acquisition :: proc(r:^Renderer,token:gfx.Frame_Token)->bool {
    if token.owner!=&r.frames || token.slot<0 || token.slot>=len(r.slots) { return false }
    slot:=&r.slots[token.slot]
    return slot.token==token && !slot.accepted && !slot.poisoned && gfx.frame_is_acquired(&r.frames,token)
}
/// Acquires one reusable native frame slot without accepting any GPU work.
acquire :: proc(r:^Renderer)->(gfx.Frame_Token,gfx.Gpu_Error) {
    if r.device==nil || r.failed { return {},.Native_Failure }
    token,err:=gfx.frame_acquire(&r.frames,r.next_slot)
    if err==.Busy { return {},.Busy }
    if err!=.None { return {},.Native_Failure }
    r.slots[token.slot].token=token; r.slots[token.slot].poisoned=false
    return token,.None
}
/// Discards acquired or failed recording state; accepted work retains its own lifecycle.
abort :: proc(r:^Renderer,token:gfx.Frame_Token)->gfx.Gpu_Error {
    if token.owner!=&r.frames || token.slot<0 || token.slot>=len(r.slots) { return .Invalid_Resource }
    slot:=&r.slots[token.slot]
    if slot.token!=token || slot.accepted { return .Invalid_Resource }
    if gfx.frame_abort(&r.frames,token)!=.None { return .Invalid_Resource }
    clear_frame(r,slot,false); slot.poisoned=false
    return .None
}
/// Initializes a fresh immutable allocation before any submission can retain it.
create_buffer_with_data :: proc(r:^Renderer,desc:gfx.Buffer_Desc,data:[]byte)->(gfx.Buffer_Handle,gfx.Gpu_Error) {
    if u64(len(data))!=desc.size { return {},.Invalid_Range }
    handle,err:=create_buffer(r,desc)
    if err!=.None { return {},err }
    entry,_:=gfx.storage_get(&r.buffers,handle)
    if desc.memory==.GPU_Private {
        upload_error:=upload_private_buffer(r,entry^,data)
        if upload_error!=.None { destroy_buffer(r,handle); return {},upload_error }
    } else { copy((cast([^]byte)entry^.mapped)[:len(data)],data); entry^.heap.epoch+=1 }
    return handle,.None
}
