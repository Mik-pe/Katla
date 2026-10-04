#+build darwin, arm64
//! Explicit native window ownership acceptance without installing scene or graphics services.
package main

import window "../../app/window"
import "core:fmt"
import "core:time"

main :: proc() {
    native:window.Window
    assert(window.window_create(&native,"Katla Odin native window",480,320)==.None)
    defer window.window_destroy(&native)
    state:=window.window_poll(&native)
    assert(state.visible && !state.closed && state.width>=480 && state.height>=320 && window.window_view(&native)!=nil)
    first_width,first_height:=state.width,state.height
    assert(window.window_resize(&native,600,400)==.None)
    for _ in 0..<10 { time.sleep(10*time.Millisecond); state=window.window_poll(&native) }
    assert(state.visible && state.width>first_width && state.height>first_height)
    fmt.printf("Native NSWindow/NSView live: %dx%d -> %dx%d backing pixels\n",first_width,first_height,state.width,state.height)
}
