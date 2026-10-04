//! Explicit acquisition and accepted-submission ownership for backend frame slots.
package gfx

import "core:mem"

/// Failures preserve existing slot state.
Frame_Error :: enum { None, Invalid_Token, Busy, Invalid_State, Exhausted }
/// CPU ownership stages; native completion is reported separately.
Frame_State :: enum { Idle, Acquired, Recorded, Submitted }
/// A token belongs to one acquisition of one stationary frame owner.
Frame_Token :: struct { owner:rawptr, slot:int, generation:u64 }
@(private="package")
Frame_Slot :: struct { state:Frame_State, generation,submission:u64 }
/// Owns CPU slot state; the backend reports exact native completion before reuse.
Frames :: struct { slots:[]Frame_Slot, next_submission:u64, allocator:mem.Allocator }
/// Allocates the requested number of slots without native surfaces or scene resources.
frames_init :: proc(f:^Frames,count:int,allocator:=context.allocator) {
    assert(count>0); f.slots=make([]Frame_Slot,count,allocator); f.allocator=allocator
}
/// Drain native work and abort acquisitions before releasing the owner.
frames_destroy :: proc(f:^Frames) {
    for slot in f.slots { assert(slot.state==.Idle,"frame still owned") }
    delete(f.slots,f.allocator); f^={}
}
@(private="package")
frame_slot :: #force_inline proc(f:^Frames,token:Frame_Token)->(^Frame_Slot,bool) {
    if token.owner!=f || token.slot<0 || token.slot>=len(f.slots) { return nil,false }
    slot:=&f.slots[token.slot]
    return slot,slot.generation==token.generation && slot.state!=.Idle
}
/// Acquires only an idle slot; busy native work must be completed explicitly.
frame_acquire :: proc(f:^Frames,index:int)->(Frame_Token,Frame_Error) {
    if index<0 || index>=len(f.slots) { return {},.Invalid_Token }
    slot:=&f.slots[index]
    if slot.state!=.Idle { return {},.Busy }
    if slot.generation==max(u64) { return {},.Exhausted }
    slot.generation+=1; slot.state=.Acquired
    return {f,index,slot.generation},.None
}
/// Marks encoding complete only for the active acquisition.
frame_recorded :: proc(f:^Frames,token:Frame_Token)->Frame_Error {
    slot,ok:=frame_slot(f,token); if !ok { return .Invalid_Token }
    if slot.state!=.Acquired { return .Invalid_State }
    slot.state=.Recorded; return .None
}
/// Publishes an accepted native submission; rejection must instead abort the token.
frame_submitted :: proc(f:^Frames,token:Frame_Token)->(u64,Frame_Error) {
    slot,ok:=frame_slot(f,token); if !ok { return 0,.Invalid_Token }
    if slot.state!=.Recorded { return 0,.Invalid_State }
    if f.next_submission==max(u64) { return 0,.Exhausted }
    f.next_submission+=1; slot.submission=f.next_submission; slot.state=.Submitted
    return slot.submission,.None
}
/// Retires only the exact accepted submission after native completion is observed.
frame_completed :: proc(f:^Frames,token:Frame_Token,submission:u64)->Frame_Error {
    slot,ok:=frame_slot(f,token); if !ok { return .Invalid_Token }
    if slot.state!=.Submitted || submission!=slot.submission { return .Invalid_State }
    slot.state=.Idle; return .None
}
/// Discards an acquisition or rejected recording without inventing a submission.
frame_abort :: proc(f:^Frames,token:Frame_Token)->Frame_Error {
    slot,ok:=frame_slot(f,token); if !ok { return .Invalid_Token }
    if slot.state==.Submitted { return .Invalid_State }
    slot.state=.Idle; return .None
}
/// Checks active CPU write ownership without changing acquisition state.
frame_is_acquired :: proc(f:^Frames,token:Frame_Token)->bool {
    slot,ok:=frame_slot(f,token)
    return ok && slot.state==.Acquired
}
