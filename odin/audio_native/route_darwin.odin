#+build darwin
package main
import "core:c"
import audio "../audio"
import "core:fmt"
import "core:time"
import "core:strings"
@(private)
LIB :: #config(AUDIO_ROUTE_LIBRARY,"../../target/odin-audio/libaudio_native.a")
foreign import route {LIB,"system:CoreAudio.framework","system:CoreFoundation.framework"}
@(default_calling_convention="c",link_prefix="ka_")
foreign route {route_begin :: proc(original,aggregate:^u32)->c.int ---;route_end :: proc(original,aggregate:u32)->c.int ---}
route_acceptance :: proc(engine:^audio.Engine)->bool {
    original,aggregate:u32;result:=route_begin(&original,&aggregate)
    if result!=0 {fmt.eprintln("FAIL actual aggregate/default switch",result);return false}
    restored:=false
    defer {if !restored {restore:=route_end(original,aggregate);fmt.println("default-output cleanup",restore)}}
    switched:=false
    for _ in 0..<100 {changed,error:=audio.engine_poll_device_change(engine);if error!=.None {fmt.eprintln("FAIL automatic device rebuild",error);return false};if changed {switched=true;break};time.sleep(20*time.Millisecond)}
    time.sleep(200*time.Millisecond);info:=audio.engine_device_info(engine)
    if !switched||!strings.contains(string(info.name[:]),"Katla Odin audio acceptance")||info.callbacks==0||info.nonzero==0 {fmt.eprintln("FAIL actual aggregate callback",switched,info);return false}
    if route_end(original,aggregate)!=0 {fmt.eprintln("FAIL restore default output");return false};restored=true
    back:=false
    for _ in 0..<100 {changed,error:=audio.engine_poll_device_change(engine);if error!=.None {fmt.eprintln("FAIL restored output rebuild",error);return false};if changed {back=true;break};time.sleep(20*time.Millisecond)}
    time.sleep(150*time.Millisecond);info=audio.engine_device_info(engine)
    if !back||strings.contains(string(info.name[:]),"Katla Odin audio acceptance")||info.callbacks==0||info.nonzero==0 {fmt.eprintln("FAIL restored actual device callback",back,info);return false}
    fmt.println("PASS actual default-device hot-swap to aggregate and restored speakers",info.callbacks,"callbacks",info.nonzero,"nonzero samples")
    return true
}
