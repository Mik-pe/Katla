//! GPU resource identity is independent of backend addressing and scene policy.
package gfx


/// Distinct resource classes prevent binding one kind of handle as another.
Buffer_Kind :: struct {}
/// Marks texture identities separately from buffers and pipelines.
Texture_Kind :: struct {}
/// Marks compiled pipeline identities.
Pipeline_Kind :: struct {}
/// A handle is valid only in its stationary originating storage and generation.
Handle :: struct($Kind:typeid) { owner:rawptr, index,generation:u32 }
/// A typed buffer identity, never a native address.
Buffer_Handle :: Handle(Buffer_Kind)
/// A typed texture identity, never a shader descriptor slot.
Texture_Handle :: Handle(Texture_Kind)
/// A typed compiled pipeline identity.
Pipeline_Handle :: Handle(Pipeline_Kind)
@(private="package")
Resource_Slot :: struct($T:typeid) { value:T, generation:u32, occupied,retired:bool }
/// Owns slot memory; resource values are transferred on insertion/removal.
Resource_Storage :: struct($T,$Kind:typeid) { slots:[dynamic]Resource_Slot(T), free_slots:[dynamic]u32, count:int }
/// Initializes a stationary owner; destroy/remove native resource values before teardown.
storage_init :: proc(s:^Resource_Storage($T,$Kind),allocator:=context.allocator) {
    s.slots=make([dynamic]Resource_Slot(T),allocator); s.free_slots=make([dynamic]u32,allocator)
}
/// Releases empty storage; live resource teardown remains its backend's responsibility.
storage_destroy :: proc(s:^Resource_Storage($T,$Kind)) {
    assert(s.count==0,"remove owned resources before destroying storage")
    delete(s.slots); delete(s.free_slots); s^={}
}
/// Transfers a value into storage and returns its typed, owner-bound identity.
storage_insert :: proc(s:^Resource_Storage($T,$Kind),value:T)->Handle(Kind) {
    index:u32
    if len(s.free_slots)>0 { index=pop(&s.free_slots); s.slots[index].value=value; s.slots[index].occupied=true }
    else {
        assert(u64(len(s.slots))<=u64(max(u32)))
        index=u32(len(s.slots)); append(&s.slots,Resource_Slot(T){value=value,occupied=true})
    }
    s.count+=1
    return {s,index,s.slots[index].generation}
}
/// Resolves a live value without interpreting a native descriptor or GPU address.
storage_get :: #force_inline proc(s:^Resource_Storage($T,$Kind),h:Handle(Kind))->(^T,bool) {
    if h.owner!=s || u64(h.index)>=u64(len(s.slots)) { return nil,false }
    slot:=&s.slots[h.index]
    if !slot.occupied || slot.generation!=h.generation { return nil,false }
    return &slot.value,true
}
/// Transfers the value out; generation exhaustion permanently retires the slot.
storage_remove :: proc(s:^Resource_Storage($T,$Kind),h:Handle(Kind))->(T,bool) {
    value,ok:=storage_get(s,h); if !ok { return {},false }
    result:=value^; value^={}
    slot:=&s.slots[h.index]; slot.occupied=false; s.count-=1
    if slot.generation==max(u32) { slot.retired=true } else { slot.generation+=1; append(&s.free_slots,h.index) }
    return result,true
}
/// Access flags describe actual buffer uses, independent of native enum values.
Buffer_Usage :: enum { Storage, Uniform, Vertex, Index, Indirect, Transfer_Source, Transfer_Destination, Readback }
/// A set of declared uses that a buffer allocation permits.
Buffer_Usages :: bit_set[Buffer_Usage]
/// Generic byte capacity and permitted uses supplied by the caller.
Buffer_Desc :: struct { size:u64, usage:Buffer_Usages }
/// A nonempty half-open range; validation uses subtraction to avoid overflow.
Buffer_Range :: struct { offset,size:u64 }
/// Checks a range without adding potentially overflowing offsets.
range_valid :: #force_inline proc(range:Buffer_Range,capacity:u64)->bool {
    return range.size>0 && range.offset<=capacity && range.size<=capacity-range.offset
}
/// Detects overlap with subtraction, including near the integer limit.
range_overlaps :: #force_inline proc(a,b:Buffer_Range)->bool {
    if a.size==0 || b.size==0 { return false }
    if a.offset<=b.offset { return b.offset-a.offset<a.size }
    return a.offset-b.offset<b.size
}
