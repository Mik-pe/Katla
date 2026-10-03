package katla_audio_dsp

import "core:testing"
import "core:mem"
import "core:sync"
import "core:thread"
import m "core:math"

@(private)
energy :: proc(samples: []f32) -> f32 { sum: f32; for v in samples { sum+=v*v }; return sum }
@(private)
expect_samples :: proc(t: ^testing.T,a,b: []f32) { testing.expect(t,len(a)==len(b)); for v,i in a { testing.expect(t,abs(v-b[i])<2e-6) } }

@(test)
test_biquad_response_and_stereo_isolation :: proc(t: ^testing.T) {
    low,high: Biquad
    testing.expect(t,biquad_init(&low,.Low_Pass,500,44100)==.None)
    testing.expect(t,biquad_init(&high,.High_Pass,500,44100)==.None)
    lp,hp: [1024]f32
    for i in 0..<len(lp) { lp[i]=1; hp[i]=1 }
    testing.expect(t,biquad_process(&low,lp[:],1)==.None && biquad_process(&high,hp[:],1)==.None)
    testing.expect(t,abs(lp[1023]-1)<1e-4 && abs(hp[1023])<1e-4)
    biquad_clear(&low)
    stereo: [1024]f32; stereo[0]=1
    testing.expect(t,biquad_process(&low,stereo[:],2)==.None)
    for i in 0..<512 { testing.expect(t,stereo[i*2+1]==0) }
}
@(test)
test_biquad_block_continuity_and_retune :: proc(t: ^testing.T) {
    a,b: Biquad
    biquad_init(&a,.Low_Pass,800,48000); biquad_init(&b,.Low_Pass,800,48000)
    original: [512]f32
    for &v,i in original { v=m.sin(f32(i)*0.3) }
    whole,chunked := original,original
    biquad_process(&a,whole[:],2)
    biquad_process(&b,chunked[:126],2); biquad_process(&b,chunked[126:300],2); biquad_process(&b,chunked[300:],2)
    expect_samples(t,whole[:],chunked[:])
    testing.expect(t,biquad_set_cutoff(&a,3000)==.None && a.cutoff==3000)
    before := a
    block := [3]f32{1,2,3}
    testing.expect(t,biquad_process(&a,block[:],2)==.Invalid_Block && block==[3]f32{1,2,3})
    testing.expect(t,biquad_set_cutoff(&a,m.nan_f32())==.Invalid_Parameter && a.cutoff==before.cutoff)
    testing.expect(t,biquad_init(&a,.High_Pass,500,44100)==.Already_Initialized)
    unset: Biquad
    testing.expect(t,biquad_init(&unset,.Low_Pass,30000,44100)==.Invalid_Parameter)
    testing.expect(t,biquad_process(&unset,block[:],1)==.Not_Initialized)
}
@(test)
test_reverb_tail_reset_and_stereo_isolation :: proc(t: ^testing.T) {
    r: Reverb; testing.expect(t,reverb_init(&r,44100)==.None); defer reverb_destroy(&r)
    reverb_set_wet(&r,1)
    signal: [8192]f32; signal[0]=1
    testing.expect(t,reverb_process(&r,signal[:],2)==.None)
    testing.expect(t,energy(signal[2000:])>0.1)
    for i in 0..<4096 { testing.expect(t,signal[i*2+1]==0) }
    reverb_clear(&r)
    silent: [4096]f32; reverb_process(&r,silent[:],2)
    testing.expect(t,energy(silent[:])==0)
    reverb_set_wet(&r,0)
    dry := [4]f32{1,-1,0.5,-0.5}; expected := dry
    reverb_process(&r,dry[:],2); expect_samples(t,dry[:],expected[:])
    testing.expect(t,reverb_set_decay(&r,2)==.None && r.combs[0][0].feedback==0.99)
    testing.expect(t,reverb_set_dampening(&r,-1)==.None && r.combs[1][2].dampening==0)
    testing.expect(t,reverb_set_wet(&r,m.INF_F32)==.Invalid_Parameter && r.wet==0)
}
@(test)
test_reverb_block_continuity_and_low_rate :: proc(t: ^testing.T) {
    a,b: Reverb; reverb_init(&a,48000); reverb_init(&b,48000)
    defer reverb_destroy(&a); defer reverb_destroy(&b)
    source: [8192]f32; source[0]=1; source[1]=0.5
    whole,chunked := source,source
    reverb_process(&a,whole[:],2)
    for i in 0..<32 { reverb_process(&b,chunked[i*256:(i+1)*256],2) }
    expect_samples(t,whole[:],chunked[:])
    tiny: Reverb; testing.expect(t,reverb_init(&tiny,1)==.None)
    impulse := [8]f32{1,0,0,0,0,0,0,0}; reverb_process(&tiny,impulse[:],1)
    for v in impulse { testing.expect(t,finite(v)) }
    reverb_destroy(&tiny); reverb_destroy(&tiny)
    testing.expect(t,reverb_init(&tiny,0)==.Invalid_Parameter)
}
@(private)
add_callback :: proc(state: rawptr,block: []f32,_: int) -> DSP_Error { n:=cast(^f32)state; for &v in block { v+=n^ }; return .None }
@(private)
scale_callback :: proc(state: rawptr,block: []f32,_: int) -> DSP_Error { n:=cast(^f32)state; for &v in block { v*=n^ }; return .None }
@(private)
failure_callback :: proc(_: rawptr,_: []f32,_: int) -> DSP_Error { return .Invalid_Parameter }
@(test)
test_effect_order_capacity_and_errors :: proc(t: ^testing.T) {
    chain: Effect_Chain; add: f32=1; scale: f32=2
    chain_add(&chain,Effect{&add,add_callback}); chain_add(&chain,Effect{&scale,scale_callback})
    block := [2]f32{3,4}
    testing.expect(t,chain_process(&chain,block[:],1)==.None && block==[2]f32{8,10})
    failed: Effect_Chain
    chain_add(&failed,Effect{&add,add_callback}); chain_add(&failed,Effect{&add,failure_callback})
    partial := [2]f32{1,2}
    testing.expect(t,chain_process(&failed,partial[:],1)==.Invalid_Parameter && partial==[2]f32{2,3})
    for _ in 2..<16 { testing.expect(t,chain_add(&chain,Effect{&add,add_callback})==.None) }
    testing.expect(t,chain_add(&chain,Effect{&add,add_callback})==.Capacity_Exceeded)
    testing.expect(t,chain_add(&chain,Effect{})==.Invalid_Parameter)
    before:=block; testing.expect(t,chain_process(&chain,block[:],3)==.Invalid_Block && before==block)
}
@(test)
test_aux_bus_sum_return_and_capacity :: proc(t: ^testing.T) {
    bus: Aux_Bus; testing.expect(t,aux_bus_init(&bus,8,0.5,0.25)==.None); defer aux_bus_destroy(&bus)
    voice := [4]f32{1,2,3,4}; output: [4]f32
    testing.expect(t,aux_bus_accumulate(&bus,voice[:],0.5)==.Not_Initialized)
    aux_bus_prepare(&bus,4); aux_bus_accumulate(&bus,voice[:],bus.send_level); aux_bus_accumulate(&bus,voice[:],0.25)
    testing.expect(t,aux_bus_process(&bus,2)==.None && aux_bus_mix_into(&bus,output[:])==.None)
    for v,i in output { testing.expect(t,abs(v-voice[i]*0.75*0.25)<1e-6) }
    testing.expect(t,aux_bus_prepare(&bus,9)==.Capacity_Exceeded && bus.active_samples==4)
    testing.expect(t,aux_bus_mix_into(&bus,output[:2])==.Invalid_Block)
    aux_bus_prepare(&bus,4); testing.expect(t,energy(bus.buffer[:4])==0)
    aux_bus_prepare(&bus,0); testing.expect(t,aux_bus_process(&bus,2)==.None)
}
@(test)
test_processing_does_not_allocate :: proc(t: ^testing.T) {
    tracker: mem.Tracking_Allocator
    mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    saved:=context.allocator; context.allocator=mem.tracking_allocator(&tracker)
    r: Reverb; reverb_init(&r,44100)
    bus: Aux_Bus; aux_bus_init(&bus,256,0.5,0.5)
    f: Biquad; biquad_init(&f,.Low_Pass,500,44100)
    chain_add(&bus.chain,effect_biquad(&f)); chain_add(&bus.chain,effect_reverb(&r))
    allocations := tracker.total_allocation_count
    signal,output: [256]f32; signal[0]=1
    for _ in 0..<64 {
        aux_bus_prepare(&bus,256); aux_bus_accumulate(&bus,signal[:],0.5)
        aux_bus_process(&bus,2); aux_bus_mix_into(&bus,output[:])
    }
    allocated := tracker.total_allocation_count-allocations
    context.allocator=saved
    reverb_destroy(&r); aux_bus_destroy(&bus)
    testing.expect(t,allocated==0 && len(tracker.allocation_map)==0 && tracker.current_memory_allocated==0)
}
@(test)
test_zone_silence_smoothing_and_ownership :: proc(t: ^testing.T) {
    targets: Zone_Targets; zone_targets_set(&targets,0,0,0.2)
    z: Zone_Reverb; testing.expect(t,zone_reverb_init(&z,44100,&targets)==.None); defer zone_reverb_destroy(&z)
    block := [4]f32{1,1,1,1}; zone_reverb_process(&z,block[:],2)
    testing.expect(t,energy(block[:])==0)
    zone_targets_set(&targets,0.8,0.5,0.3)
    block={1,1,1,1}; zone_reverb_process(&z,block[:],2)
    testing.expect(t,abs(z.wet-0.04)<1e-6 && abs(block[0]-0.96)<1e-6)
    before:=z.wet
    testing.expect(t,zone_reverb_process(&z,block[:3],2)==.Invalid_Block && z.wet==before)
    testing.expect(t,zone_targets_set(&targets,m.nan_f32(),1,0)==.Invalid_Parameter)
    chain: Effect_Chain; chain_add(&chain,effect_zone_reverb(&z)); testing.expect(t,chain_process(&chain,block[:],2)==.None)
}
@(private)
Zone_Thread_State :: struct { targets: ^Zone_Targets, started,done: u32 }
@(private)
test_zone_producer :: proc(worker: ^thread.Thread) {
    s:=cast(^Zone_Thread_State)worker.data
    for sync.atomic_load(&s.started)==0 { sync.cpu_relax() }
    for i in 0..<20000 { zone_targets_set(s.targets,f32(i%99)/100,f32(i%100)/100,f32(i%100)/100) }
    sync.atomic_store(&s.done,1)
}
@(test)
test_zone_atomic_publication_on_real_thread :: proc(t: ^testing.T) {
    targets: Zone_Targets; zone_targets_set(&targets,0.8,0.4,0.2)
    state:=Zone_Thread_State{targets=&targets}
    z: Zone_Reverb; zone_reverb_init(&z,44100,&targets); defer zone_reverb_destroy(&z)
    producer:=thread.create(test_zone_producer); producer.data=&state; thread.start(producer)
    sync.atomic_store(&state.started,1)
    block: [256]f32
    for _ in 0..<128 { block[0]=1; testing.expect(t,zone_reverb_process(&z,block[:],2)==.None); for sample in block { testing.expect(t,finite(sample)) } }
    thread.join(producer); thread.destroy(producer)
    testing.expect(t,sync.atomic_load(&state.done)==1)
}
@(test)
test_gain_helpers :: proc(t: ^testing.T) {
    for i in 0..<41 {
        pan:=f32(i)/20-1
        l,r:=compute_pan_gains(pan)
        testing.expect(t,abs(l*l+r*r-1)<1e-6)
        db:=f32(i)-20
        testing.expect(t,abs(linear_to_db(db_to_linear(db))-db)<1e-4)
    }
    testing.expect(t,m.is_inf(linear_to_db(0),-1))
}
