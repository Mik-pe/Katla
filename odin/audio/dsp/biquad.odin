// Mono/stereo DSP primitives, independent of devices, codecs, ECS and engine math.
package katla_audio_dsp

import m "core:math"

DSP_Error :: enum { None, Invalid_Parameter, Invalid_Block, Not_Initialized, Already_Initialized, Capacity_Exceeded }
Filter_Kind :: enum { Low_Pass, High_Pass }
/// Mutable processing history belongs to a single audio thread.
Biquad :: struct {
    kind: Filter_Kind,
    cutoff,sample_rate,q: f32,
    b0,b1,b2,a1,a2: f32,
    x1,x2,y1,y2: [2]f32,
    initialized: bool,
}
@(private)
finite :: #force_inline proc(v: f32) -> bool { return !m.is_nan(v) && !m.is_inf(v) }
@(private)
valid_block :: #force_inline proc(block: []f32,channels: int) -> bool { return (channels==1 || channels==2) && len(block)%channels==0 }
@(private)
biquad_recalculate :: proc(f: ^Biquad) {
    w0 := 2*f32(m.PI)*f.cutoff/f.sample_rate
    c,s := m.cos(w0),m.sin(w0)
    alpha := s/(2*f.q)
    if f.kind == .Low_Pass { f.b1 = 1-c; f.b0 = f.b1*0.5; f.b2 = f.b0 }
    else { f.b0 = (1+c)*0.5; f.b1 = -(1+c); f.b2 = f.b0 }
    a0 := 1+alpha
    f.b0 /= a0; f.b1 /= a0; f.b2 /= a0
    f.a1 = -2*c/a0; f.a2 = (1-alpha)/a0
}
/// Butterworth-like Q=0.707; cutoff must be strictly below Nyquist and above zero.
biquad_init :: proc(f: ^Biquad,kind: Filter_Kind,cutoff,sample_rate: f32) -> DSP_Error {
    if f.initialized { return .Already_Initialized }
    if !finite(sample_rate) || sample_rate<=0 || !finite(cutoff) || cutoff<=0 || cutoff>=sample_rate*0.5 { return .Invalid_Parameter }
    if kind != .Low_Pass && kind != .High_Pass { return .Invalid_Parameter }
    f^ = Biquad{kind=kind,cutoff=cutoff,sample_rate=sample_rate,q=0.707,initialized=true}
    biquad_recalculate(f)
    return .None
}
/// Retune coefficients without discarding prior sample history.
biquad_set_cutoff :: proc(f: ^Biquad,cutoff: f32) -> DSP_Error {
    if !f.initialized { return .Not_Initialized }
    if !finite(cutoff) || cutoff<=0 || cutoff>=f.sample_rate*0.5 { return .Invalid_Parameter }
    f.cutoff = cutoff; biquad_recalculate(f)
    return .None
}
/// Clear history while retaining coefficients.
biquad_clear :: proc(f: ^Biquad) { f.x1={}; f.x2={}; f.y1={}; f.y2={} }
/// Process complete interleaved mono/stereo frames in place, without allocation.
biquad_process :: proc(f: ^Biquad,block: []f32,channels: int) -> DSP_Error {
    if !f.initialized { return .Not_Initialized }
    if !valid_block(block,channels) { return .Invalid_Block }
    for x,i in block {
        ch := i%channels
        y := f.b0*x+f.b1*f.x1[ch]+f.b2*f.x2[ch]-f.a1*f.y1[ch]-f.a2*f.y2[ch]
        f.x2[ch]=f.x1[ch]; f.x1[ch]=x
        f.y2[ch]=f.y1[ch]; f.y1[ch]=y
        block[i]=y
    }
    return .None
}
/// Equal-power panning, matching the Rust gain helper; pan is expected in [-1,1].
compute_pan_gains :: #force_inline proc(pan: f32) -> (f32,f32) { angle := (pan+1)*0.25*f32(m.PI); return m.cos(angle),m.sin(angle) }
/// Convert decibels to linear amplitude.
db_to_linear :: #force_inline proc(db: f32) -> f32 { return m.pow(f32(10),db/20) }
/// Nonpositive amplitudes produce negative infinity.
linear_to_db :: #force_inline proc(linear: f32) -> f32 { if linear<=0 { return m.NEG_INF_F32 }; return 20*m.log10(linear) }
