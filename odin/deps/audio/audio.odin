//! Versioned opaque device/decoder ABI to repository-pinned miniaudio and stb_vorbis.
package audio_native
import "core:c"
@(private)
LIB :: #config(AUDIO_LIBRARY,"../../../target/odin-audio/libaudio_native.a")
when ODIN_OS == .Darwin {
    foreign import lib {LIB,"system:CoreFoundation.framework","system:CoreAudio.framework","system:AudioToolbox.framework"}
} else when ODIN_OS == .Linux {
    foreign import lib {LIB,"system:dl","system:pthread","system:m"}
} else when ODIN_OS == .Windows {
    foreign import lib {LIB,"system:ole32.lib","system:uuid.lib","system:winmm.lib"}
}
Decoder :: distinct rawptr
Device :: distinct rawptr
Metadata :: struct {format,channels,sample_rate,reserved:u32,frames:u64}
Device_Info :: struct {name:[256]u8,sample_rate,channels,backend,is_default:u32,callbacks,frames,nonzero:u64}
Render :: #type proc "c" (user:rawptr,output:[^]f32,frames:u32)
@(default_calling_convention="c", link_prefix="ka_")
foreign lib {
    abi_version :: proc "c" ()->u32 ---
    decoder_open :: proc "c" (bytes:rawptr,length:uintptr,format:u32,output:^Decoder,info:^Metadata)->c.int ---
    decoder_close :: proc "c" (decoder:Decoder) ---
    decoder_read :: proc "c" (decoder:Decoder,output:[^]f32,frames:u64,read:^u64)->c.int ---
    decoder_seek :: proc "c" (decoder:Decoder,frame:u64)->c.int ---
    device_open :: proc "c" (render:Render,user:rawptr,sample_rate:u32,index:c.int,output:^Device)->c.int ---
    device_start :: proc "c" (device:Device)->c.int ---
    device_stop :: proc "c" (device:Device)->c.int ---
    device_close :: proc "c" (device:Device) ---
    device_changed :: proc "c" (device:Device)->c.int ---
    device_snapshot :: proc "c" (device:Device,output:^Device_Info) ---
    devices :: proc "c" (output:[^]Device_Info,capacity:u32,count:^u32)->c.int ---
}
