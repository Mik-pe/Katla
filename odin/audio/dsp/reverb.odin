// Schroeder reverb using the Katla reference delay lines, allocated at initialization.
package katla_audio_dsp

import "core:mem"

@(private)
Comb :: struct { buffer: []f32, index: int, feedback,store,dampening: f32 }
@(private)
Allpass :: struct { buffer: []f32, index: int }
/// Owns delay storage. Keep one owner; shallow copies must not be destroyed or processed.
Reverb :: struct {
    combs: [2][4]Comb,
    allpasses: [2][2]Allpass,
    wet: f32,
    allocator: mem.Allocator,
    initialized: bool,
}
/// Positive sample rate; low-rate delay lines are clamped to at least one sample.
reverb_init :: proc(r: ^Reverb,sample_rate: u32,allocator := context.allocator) -> DSP_Error {
    if r.initialized { return .Already_Initialized }
    if sample_rate==0 { return .Invalid_Parameter }
    r^ = Reverb{wet=0.3,allocator=allocator,initialized=true}
    comb_delays := [2][4]int{{1557,1617,1491,1422},{1666,1730,1595,1522}}
    allpass_delays := [2][2]int{{225,556},{239,589}}
    scale := f32(sample_rate)/44100
    for ch in 0..<2 {
        for i in 0..<4 {
            delay := max(1,int(f32(comb_delays[ch][i])*scale))
            r.combs[ch][i] = Comb{buffer=make([]f32,delay,allocator),feedback=0.84,dampening=0.2}
        }
        for i in 0..<2 { r.allpasses[ch][i].buffer = make([]f32,max(1,int(f32(allpass_delays[ch][i])*scale)),allocator) }
    }
    return .None
}
/// Free owned buffers with their originating allocator. Idempotent for a zero/destroyed value.
reverb_destroy :: proc(r: ^Reverb) {
    if !r.initialized { return }
    for ch in 0..<2 {
        for i in 0..<4 { delete(r.combs[ch][i].buffer,r.allocator) }
        for i in 0..<2 { delete(r.allpasses[ch][i].buffer,r.allocator) }
    }
    r^ = {}
}
/// Reset the delay tail without changing parameters or allocating.
reverb_clear :: proc(r: ^Reverb) {
    for ch in 0..<2 {
        for i in 0..<4 {
            c := &r.combs[ch][i]
            for &sample in c.buffer { sample=0 }
            c.index=0; c.store=0
        }
        for i in 0..<2 {
            a := &r.allpasses[ch][i]
            for &sample in a.buffer { sample=0 }
            a.index=0
        }
    }
}
/// Clamp finite wet mix to [0,1]; reject NaN/infinity without mutation.
reverb_set_wet :: proc(r: ^Reverb,value: f32) -> DSP_Error {
    if !r.initialized { return .Not_Initialized }
    if !finite(value) { return .Invalid_Parameter }
    r.wet=clamp(value,0,1); return .None
}
/// Clamp finite comb feedback to [0,0.99].
reverb_set_decay :: proc(r: ^Reverb,value: f32) -> DSP_Error {
    if !r.initialized { return .Not_Initialized }
    if !finite(value) { return .Invalid_Parameter }
    for ch in 0..<2 { for i in 0..<4 { r.combs[ch][i].feedback=clamp(value,0,0.99) } }
    return .None
}
/// Clamp finite low-pass dampening to [0,1].
reverb_set_dampening :: proc(r: ^Reverb,value: f32) -> DSP_Error {
    if !r.initialized { return .Not_Initialized }
    if !finite(value) { return .Invalid_Parameter }
    for ch in 0..<2 { for i in 0..<4 { r.combs[ch][i].dampening=clamp(value,0,1) } }
    return .None
}
@(private)
reverb_sample :: #force_inline proc(r: ^Reverb,ch: int,input: f32) -> f32 {
    wet: f32
    for i in 0..<4 {
        c := &r.combs[ch][i]
        output := c.buffer[c.index]
        c.store = output*(1-c.dampening)+c.store*c.dampening
        c.buffer[c.index] = input+c.store*c.feedback
        c.index = (c.index+1)%len(c.buffer)
        wet += output
    }
    for i in 0..<2 {
        a := &r.allpasses[ch][i]
        buffered := a.buffer[a.index]
        output := -wet+buffered
        a.buffer[a.index] = wet+buffered*0.5
        a.index = (a.index+1)%len(a.buffer)
        wet=output
    }
    return input*(1-r.wet)+wet*r.wet
}
/// Mono uses left delay lines; stereo has independent left/right histories.
reverb_process :: proc(r: ^Reverb,block: []f32,channels: int) -> DSP_Error {
    if !r.initialized { return .Not_Initialized }
    if !valid_block(block,channels) { return .Invalid_Block }
    for sample,i in block { block[i]=reverb_sample(r,i%channels,sample) }
    return .None
}
