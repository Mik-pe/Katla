//! Hardware callback acceptance: real output, pause, failed replacement and same-device reopen.
package main
import audio "../audio"
import "core:fmt"
import "core:os"
import "core:time"
import "core:strings"
main :: proc() {
    devices:[32]audio.Device_Info;count,enumeration:=audio.devices(devices[:])
    if enumeration!=.None||count==0 {fmt.eprintln("FAIL no actual output devices",enumeration);os.exit(1)}
    for &device,index in devices[:min(count,len(devices))] {fmt.println("device",index,strings.trim_right(string(device.name[:]),"\x00"),"default",device.is_default)}
    bytes,error:=os.read_entire_file("odin/audio/testdata/tone.wav",context.allocator)
    if error!=nil {fmt.eprintln(error);os.exit(1)};defer delete(bytes)
    clip,decode:=audio.clip_decode(bytes);if decode!=.None {fmt.eprintln(decode);os.exit(1)};defer audio.clip_release(clip)
    engine,opened:=audio.engine_create();if opened!=.None {fmt.eprintln(opened);os.exit(1)};defer audio.engine_destroy(engine)
    desc:=audio.DEFAULT_PLAY;desc.looping=true;desc.volume=.15
    voice,played:=audio.engine_play(engine,clip,desc)
    if played!=.None||audio.engine_resume(engine)!=.None {fmt.eprintln("FAIL play/resume");os.exit(1)}
    time.sleep(350*time.Millisecond);before:=audio.engine_device_info(engine)
    if before.callbacks==0||before.nonzero==0 {fmt.eprintln("FAIL no real hardware nonzero callbacks",before);os.exit(1)}
    if audio.engine_pause(engine)!=.None {os.exit(1)};paused:=audio.engine_device_info(engine);time.sleep(80*time.Millisecond)
    after_pause:=audio.engine_device_info(engine)
    if after_pause.callbacks!=paused.callbacks {fmt.eprintln("FAIL callbacks continued after pause");os.exit(1)}
    if audio.engine_resume(engine)!=.None {os.exit(1)}
    position,_:=audio.voice_position(engine,voice)
    if audio.engine_reopen_device(engine,99999)!=.Device_Not_Found||audio.voice_state(engine,voice)!=.Playing {fmt.eprintln("FAIL rollback");os.exit(1)}
    if audio.engine_reopen_device(engine)!=.None {fmt.eprintln("FAIL replacement");os.exit(1)}
    time.sleep(350*time.Millisecond);after:=audio.engine_device_info(engine);position_after,_:=audio.voice_position(engine,voice)
    if after.callbacks==0||after.nonzero==0||audio.voice_state(engine,voice)!=.Playing||position_after==position {fmt.eprintln("FAIL replacement callback/voice",after);os.exit(1)}
    when ODIN_OS==.Darwin {if len(os.args)>1&&os.args[1]=="--switch-default" {if !route_acceptance(engine) {os.exit(1)}}}
    audio.voice_stop(engine,voice);time.sleep(80*time.Millisecond)
    if audio.voice_state(engine,voice)!=.Stopped {fmt.eprintln("FAIL fade stop");os.exit(1)}
    if !codec_output_acceptance(engine) {os.exit(1)}
    fmt.println("PASS actual native output",strings.trim_right(string(after.name[:]),"\x00"),"backend",after.backend,"before callbacks",before.callbacks,"after callbacks",after.callbacks,"nonzero samples",after.nonzero,"pause/failure rollback/reopen preserved voice")
}

// Each codec has an independent nonzero native callback interval after the previous voice has stopped.
codec_output_acceptance :: proc(engine:^audio.Engine)->bool {
    for name in ([]string{"tone.wav","tone.ogg","tone.mp3","tone.flac"}) {
        path:=strings.concatenate({"odin/audio/testdata/",name});defer delete(path)
        bytes,read_error:=os.read_entire_file(path,context.allocator);if read_error!=nil {fmt.eprintln(read_error);return false};defer delete(bytes)
        desc:=audio.DEFAULT_PLAY;desc.looping=true;desc.volume=.1
        for streaming in ([]bool{false,true}) {
            handle:audio.Voice_Handle;error:audio.Error
            if streaming {handle,error=audio.engine_play_stream(engine,bytes,desc)}
            else {
                clip,decode_error:=audio.clip_decode(bytes);if decode_error!=.None {fmt.eprintln(decode_error);return false}
                handle,error=audio.engine_play(engine,clip,desc)
                audio.clip_release(clip)
            }
            if error!=.None {fmt.eprintln("FAIL native codec voice",name,streaming,error);return false}
            before:=audio.engine_device_info(engine);time.sleep(120*time.Millisecond);after:=audio.engine_device_info(engine)
            if after.callbacks<=before.callbacks||after.nonzero<=before.nonzero||audio.voice_state(engine,handle)!=.Playing {fmt.eprintln("FAIL native codec callbacks",name,streaming);return false}
            audio.voice_stop(engine,handle);time.sleep(40*time.Millisecond);audio.engine_collect(engine)
            if audio.voice_state(engine,handle)!=.Stopped {fmt.eprintln("FAIL native codec stop",name,streaming);return false}
            fmt.println("PASS actual codec output",name,"streaming",streaming,"callbacks",after.callbacks-before.callbacks,"nonzero samples",after.nonzero-before.nonzero)
        }
    }
    return true
}
