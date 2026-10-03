// Borrowed effect callbacks and preallocated auxiliary mixing buffers.
package katla_audio_dsp

import "core:mem"

Effect_Proc :: #type proc(state: rawptr,block: []f32,channels: int) -> DSP_Error
/// Non-owning callback/state pair. State must stay valid and stationary during processing.
Effect :: struct { state: rawptr, process: Effect_Proc }
/// Fixed capacity; adding effects allocates nothing. Effects execute in insertion order.
Effect_Chain :: struct { effects: [16]Effect, count: int }
/// Add a non-owning effect; setup belongs outside the audio callback.
chain_add :: proc(chain: ^Effect_Chain,effect: Effect) -> DSP_Error {
    if effect.process==nil || effect.state==nil { return .Invalid_Parameter }
    if chain.count==len(chain.effects) { return .Capacity_Exceeded }
    chain.effects[chain.count]=effect; chain.count+=1
    return .None
}
/// Reject malformed frames before processing. A failing callback leaves earlier effects applied.
chain_process :: proc(chain: ^Effect_Chain,block: []f32,channels: int) -> DSP_Error {
    if !valid_block(block,channels) { return .Invalid_Block }
    for i in 0..<chain.count {
        effect := chain.effects[i]
        err := effect.process(effect.state,block,channels)
        if err != .None { return err }
    }
    return .None
}
@(private)
biquad_callback :: proc(state: rawptr,block: []f32,channels: int) -> DSP_Error { return biquad_process(cast(^Biquad)state,block,channels) }
@(private)
reverb_callback :: proc(state: rawptr,block: []f32,channels: int) -> DSP_Error { return reverb_process(cast(^Reverb)state,block,channels) }
@(private)
zone_callback :: proc(state: rawptr,block: []f32,channels: int) -> DSP_Error { return zone_reverb_process(cast(^Zone_Reverb)state,block,channels) }
/// Borrow a stable filter for an effect chain.
effect_biquad :: proc(f: ^Biquad) -> Effect { return {f,biquad_callback} }
/// Borrow a stable initialized reverb for an effect chain.
effect_reverb :: proc(r: ^Reverb) -> Effect { return {r,reverb_callback} }
/// Borrow a stable zone reverb for an effect chain.
effect_zone_reverb :: proc(z: ^Zone_Reverb) -> Effect { return {z,zone_callback} }

/// Owns scratch storage, borrows effects, and is manipulated only on the processing thread.
Aux_Bus :: struct {
    id: u64,
    send_level,return_level: f32,
    chain: Effect_Chain,
    buffer: []f32,
    active_samples: int,
    allocator: mem.Allocator,
    initialized,prepared: bool,
}
/// Allocate scratch capacity outside processing; levels must be finite and nonnegative.
aux_bus_init :: proc(bus: ^Aux_Bus,capacity: int,send_level,return_level: f32,allocator := context.allocator) -> DSP_Error {
    if bus.initialized { return .Already_Initialized }
    if capacity<0 || !finite(send_level) || !finite(return_level) || send_level<0 || return_level<0 { return .Invalid_Parameter }
    bus^ = Aux_Bus{buffer=make([]f32,capacity,allocator),send_level=send_level,return_level=return_level,allocator=allocator,initialized=true}
    return .None
}
/// Free scratch memory with the originating allocator. Borrowed effects are not destroyed.
aux_bus_destroy :: proc(bus: ^Aux_Bus) { if bus.initialized { delete(bus.buffer,bus.allocator) }; bus^={} }
/// Clear active samples without resizing or allocating. Oversized blocks leave state unchanged.
aux_bus_prepare :: proc(bus: ^Aux_Bus,count: int) -> DSP_Error {
    if !bus.initialized { return .Not_Initialized }
    if count<0 { return .Invalid_Parameter }
    if count>len(bus.buffer) { return .Capacity_Exceeded }
    bus.active_samples=count; bus.prepared=true
    for &sample in bus.buffer[:count] { sample=0 }
    return .None
}
/// Add one matching-size voice block using its effective send level (default bus.send_level).
aux_bus_accumulate :: proc(bus: ^Aux_Bus,voice: []f32,send_level: f32) -> DSP_Error {
    if !bus.initialized || !bus.prepared { return .Not_Initialized }
    if len(voice)!=bus.active_samples { return .Invalid_Block }
    if !finite(send_level) || send_level<0 { return .Invalid_Parameter }
    for sample,i in voice { bus.buffer[i]+=sample*send_level }
    return .None
}
/// Process the active scratch block through the bus's borrowed effect chain.
aux_bus_process :: proc(bus: ^Aux_Bus,channels: int) -> DSP_Error {
    if !bus.initialized || !bus.prepared { return .Not_Initialized }
    return chain_process(&bus.chain,bus.buffer[:bus.active_samples],channels)
}
/// Add processed scratch samples to a matching-size output block, scaled by return level.
aux_bus_mix_into :: proc(bus: ^Aux_Bus,output: []f32) -> DSP_Error {
    if !bus.initialized || !bus.prepared { return .Not_Initialized }
    if len(output)!=bus.active_samples { return .Invalid_Block }
    for &sample,i in output { sample+=bus.buffer[i]*bus.return_level }
    return .None
}
