// Offline DSP consumer paired with the Rust audio crate. Opens no output device.
package main

import "core:fmt"
import m "core:math"
import dsp "../audio/dsp"

input :: proc(block,channels: int) -> [128]f32 {
    result: [128]f32
    for i in 0..<64*channels {
        frame,ch := block*64+i/channels,i%channels
        if frame==0 { result[i]=1 if ch==0 else 0.5 }
        else if frame<512 { result[i]=m.sin(f32(frame)*0.071)*(0.2 if ch==0 else -0.1) }
    }
    return result
}
emit :: proc(name: string,rate: u32,channels,block: int,values: []f32) {
    fmt.printf("%s_%d_%d_%d",name,rate,channels,block)
    for value in values { fmt.printf(" %.9e",value) }
    fmt.println()
}
main :: proc() {
    for rate in ([3]u32{44100,48000,96000}) {
        for channels in 1..=2 {
            for kind in ([2]dsp.Filter_Kind{.Low_Pass,.High_Pass}) {
                filter: dsp.Biquad; assert(dsp.biquad_init(&filter,kind,800,f32(rate))==.None)
                for block in 0..<32 {
                    if block==8 { assert(dsp.biquad_set_cutoff(&filter,3200)==.None) }
                    signal:=input(block,channels)
                    assert(dsp.biquad_process(&filter,signal[:64*channels],channels)==.None)
                    emit("low" if kind==.Low_Pass else "high",rate,channels,block,signal[:64*channels])
                }
            }
            reverb: dsp.Reverb; assert(dsp.reverb_init(&reverb,rate)==.None)
            for block in 0..<32 {
                if block==16 { dsp.reverb_set_wet(&reverb,0.6); dsp.reverb_set_decay(&reverb,0.65); dsp.reverb_set_dampening(&reverb,0.3) }
                signal:=input(block,channels); assert(dsp.reverb_process(&reverb,signal[:64*channels],channels)==.None)
                emit("reverb",rate,channels,block,signal[:64*channels])
            }
            dsp.reverb_destroy(&reverb)
            chain: dsp.Effect_Chain
            filter: dsp.Biquad; dsp.biquad_init(&filter,.Low_Pass,1000,f32(rate))
            dsp.reverb_init(&reverb,rate)
            dsp.chain_add(&chain,dsp.effect_biquad(&filter)); dsp.chain_add(&chain,dsp.effect_reverb(&reverb))
            for block in 0..<32 {
                signal:=input(block,channels); assert(dsp.chain_process(&chain,signal[:64*channels],channels)==.None)
                emit("chain",rate,channels,block,signal[:64*channels])
            }
            dsp.reverb_destroy(&reverb)
            bus: dsp.Aux_Bus; dsp.aux_bus_init(&bus,128,0.4,0.7)
            filter={}; dsp.biquad_init(&filter,.High_Pass,300,f32(rate)); dsp.reverb_init(&reverb,rate)
            dsp.chain_add(&bus.chain,dsp.effect_biquad(&filter)); dsp.chain_add(&bus.chain,dsp.effect_reverb(&reverb))
            for block in 0..<32 {
                signal:=input(block,channels)
                dsp.aux_bus_prepare(&bus,64*channels)
                dsp.aux_bus_accumulate(&bus,signal[:64*channels],bus.send_level)
                second:=signal*(-0.3); dsp.aux_bus_accumulate(&bus,second[:64*channels],0.2)
                assert(dsp.aux_bus_process(&bus,channels)==.None)
                assert(dsp.aux_bus_mix_into(&bus,signal[:64*channels])==.None)
                emit("aux",rate,channels,block,signal[:64*channels])
            }
            dsp.aux_bus_destroy(&bus); dsp.reverb_destroy(&reverb)
        }
    }
    for i in 0..<41 {
        l,r:=dsp.compute_pan_gains(f32(i)/20-1)
        values:=[3]f32{l,r,dsp.db_to_linear(f32(i)-20)}
        emit("gain",0,0,i,values[:])
    }
}
