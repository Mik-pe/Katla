//! Native input batches own committed text and retain physical states across owner frames.
package window

import ui "../../ui"
import "core:mem"
import "core:strings"

/// Keyboard codes are native physical positions; application bindings assign gameplay action names.
Input_State :: struct {
    events:[dynamic]ui.Input_Event,
    keys:bit_set[ui.Key],
    buttons:bit_set[ui.Pointer_Button],
    pointer,delta:ui.Vec2,
    wheel:f32,
    modifiers:ui.Modifiers,
    focused:bool,
    allocator:mem.Allocator,
}
input_init :: proc(owner:^Input_State,allocator:=context.allocator) { owner^={events=make([dynamic]ui.Input_Event,allocator),allocator=allocator} }
/// Call after all UI, camera and gameplay consumers have consumed the current frame.
input_begin :: proc(owner:^Input_State) {
    for event in owner.events {
        #partial switch item in event {
        case ui.Text_Commit: delete(item.text,owner.allocator)
        case ui.IME_Preedit: delete(item.text,owner.allocator)
        }
    }
    clear(&owner.events); owner.delta={}; owner.wheel=0
}
input_destroy :: proc(owner:^Input_State) { input_begin(owner); delete(owner.events); owner^={} }
/// Copies transient AppKit strings before the native autorelease pool drains.
input_text :: proc(owner:^Input_State,text:string) { append(&owner.events,ui.Text_Commit{strings.clone(text,owner.allocator)}) }
input_preedit :: proc(owner:^Input_State,text:string,cursor,end:int) { append(&owner.events,ui.IME_Preedit{strings.clone(text,owner.allocator),cursor,end}) }
/// Blur releases every held key/button and explicitly cancels retained UI capture.
input_focus :: proc(owner:^Input_State,focused:bool) {
    if owner.focused==focused { return }
    owner.focused=focused
    if !focused { owner.keys={}; owner.buttons={}; owner.modifiers={}; owner.delta={}; owner.wheel=0 }
    append(&owner.events,ui.Window_Focus{focused})
}
