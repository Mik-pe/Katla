// Atomically published zone parameters, with audio-thread-owned reverb history.
package katla_audio_dsp

import "core:sync"

/// Shared controls. Use zone_targets_set; these bit fields must only be accessed atomically.
Zone_Targets :: struct { decay_bits,wet_bits,dampening_bits: u32 }
/// Targets are borrowed and must outlive processing; owned inner delay buffers require destruction.
Zone_Reverb :: struct { inner: Reverb, targets: ^Zone_Targets, decay,wet,dampening: f32 }

/// Publish finite clamped target parameters; fields are independent atomic controls.
zone_targets_set :: proc(targets: ^Zone_Targets,decay,wet,dampening: f32) -> DSP_Error {
    if targets==nil || !finite(decay) || !finite(wet) || !finite(dampening) { return .Invalid_Parameter }
    sync.atomic_store_explicit(&targets.decay_bits,transmute(u32)clamp(decay,0,0.99),.Relaxed)
    sync.atomic_store_explicit(&targets.wet_bits,transmute(u32)clamp(wet,0,1),.Relaxed)
    sync.atomic_store_explicit(&targets.dampening_bits,transmute(u32)clamp(dampening,0,1),.Relaxed)
    return .None
}
/// Initializes delay storage with wet output initially disabled.
zone_reverb_init :: proc(z: ^Zone_Reverb,sample_rate: u32,targets: ^Zone_Targets) -> DSP_Error {
    if targets==nil { return .Invalid_Parameter }
    err := reverb_init(&z.inner,sample_rate)
    if err != .None { return err }
    z.targets=targets; z.decay=0; z.wet=0; z.dampening=0.2
    reverb_set_wet(&z.inner,0)
    return .None
}
/// Destroy owned delay lines; shared targets remain caller-owned.
zone_reverb_destroy :: proc(z: ^Zone_Reverb) { reverb_destroy(&z.inner); z^={} }
/// Smooth once per process block by 0.08, matching Rust. Inactive zones output silence.
zone_reverb_process :: proc(z: ^Zone_Reverb,block: []f32,channels: int) -> DSP_Error {
    if !z.inner.initialized || z.targets==nil { return .Not_Initialized }
    if !valid_block(block,channels) { return .Invalid_Block }
    if len(block)==0 { return .None }
    decay := transmute(f32)sync.atomic_load_explicit(&z.targets.decay_bits,.Relaxed)
    wet := transmute(f32)sync.atomic_load_explicit(&z.targets.wet_bits,.Relaxed)
    dampening := transmute(f32)sync.atomic_load_explicit(&z.targets.dampening_bits,.Relaxed)
    z.decay += (decay-z.decay)*0.08
    z.wet += (wet-z.wet)*0.08
    z.dampening += (dampening-z.dampening)*0.08
    if z.wet < 0.001 { for &sample in block { sample=0 }; return .None }
    reverb_set_decay(&z.inner,z.decay)
    reverb_set_wet(&z.inner,z.wet)
    reverb_set_dampening(&z.inner,z.dampening)
    return reverb_process(&z.inner,block,channels)
}
