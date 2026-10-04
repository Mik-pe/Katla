#+build linux, windows
//! Real SDL text and composition events remain independent of physical key edges.
package window

import wn "../../deps/window_native"
import ui "../../ui"
import "core:math"

Native_Input :: struct { window:^Window,input:Input_State,ime:ui.IME_Request,close_requested:bool }
/// Installs a per-window input owner; SDL queues preserve events for other native windows.
native_input_init :: proc(owner:^Native_Input,window:^Window,allocator:=context.allocator)->Window_Error {
    if window==nil || window.native==nil { return .Native_Failure }
    owner^={window=window};input_init(&owner.input,allocator);return .None
}
@(private="package")
native_coordinate_scale :: proc(window:^Window)->f32 {
    value:wn.State;if window.api.state(window.native,&value)==0 || value.density<=0 || value.scale<=0 { window.error=.Native_Failure;return 1 };return value.scale/value.density
}
@(private="package")
native_input_event :: proc(state:rawptr,event:^wn.Event) {
    owner:=cast(^Native_Input)state;input:=&owner.input
    scale:=native_coordinate_scale(owner.window);position:=ui.Vec2{event.x/scale,event.y/scale}
    #partial switch event.kind {
    case .Close:owner.close_requested=true
    case .Focus:input_focus(input,event.code!=0)
    case .Key_Down,.Key_Up:
        key:=portable_key(event.code);input.modifiers=portable_modifiers(event.modifiers)
        if event.kind==.Key_Down { if key!=.None { input.keys|={key} };append(&input.events,ui.Key_Down{key,input.modifiers,event.repeat!=0}) }
        else { input.keys-={key};append(&input.events,ui.Key_Up{key,input.modifiers}) }
    case .Motion:
        input.delta+=ui.Vec2{event.dx/scale,event.dy/scale};input.pointer=position;append(&input.events,ui.Pointer_Move{position})
    case .Button_Down,.Button_Up:
        button:ui.Pointer_Button
        switch event.button {
        case 1:button=.Left
        case 2:button=.Middle
        case 3:button=.Right
        case:return
        }
        input.pointer=position;input.modifiers=portable_modifiers(event.modifiers)
        if event.kind==.Button_Down { input.buttons|={button};append(&input.events,ui.Pointer_Down{position,button,input.modifiers,max(1,event.clicks)}) }
        else { input.buttons-={button};append(&input.events,ui.Pointer_Up{position,button,input.modifiers}) }
    case .Wheel:
        input.pointer=position;delta:=ui.Vec2{event.dx,event.dy};input.wheel+=delta[1];append(&input.events,ui.Scroll{position,delta})
    case .Text:if event.text!=nil { input_text(input,string(event.text)) }
    case .Editing:
        text:=string(event.text) if event.text!=nil else ""
        start:=portable_text_byte(text,int(event.start)) if event.start>=0 else len(text)
        end:=portable_text_byte(text,int(max(0,event.start))+int(max(0,event.length))) if event.start>=0 else start
        input_preedit(input,text,start,end)
    }
}
/// Reports native physical drawable extent and input in UI logical coordinates.
native_input_poll :: proc(owner:^Native_Input,time:f64)->(State,ui.Input) {
    input_begin(&owner.input)
    state:=window_poll(owner.window,owner,native_input_event)
    value:wn.State;if owner.window.api.state(owner.window.native,&value)==0 { owner.window.error=.Native_Failure;return {closed=true},{} }
    input_focus(&owner.input,value.focused!=0)
    return state,ui.Input{owner.input.events[:],time,max(f32(0.01),value.scale)}
}
/// Native candidate placement receives SDL window coordinates rather than GPU pixels.
native_input_ime :: proc(owner:^Native_Input,request:ui.IME_Request) {
    owner.ime=request;scale:=native_coordinate_scale(owner.window);rect:=request.cursor
    values:=[4]f32{rect.x*scale,rect.y*scale,max(1,rect.width*scale),max(1,rect.height*scale)}
    for value in values { if math.is_nan(value) || math.is_inf(value) || value<f32(min(i32)) || value>=f32(max(i32)) { owner.window.error=.Native_Failure;return } }
    if owner.window.api.ime(owner.window.native,u32(request.active),i32(values[0]),i32(values[1]),i32(values[2]),i32(values[3]))==0 { owner.window.error=.Native_Failure }
}
native_input_destroy :: proc(owner:^Native_Input) { if owner.window!=nil && owner.window.native!=nil { owner.window.api.ime(owner.window.native,0,0,0,1,1) };input_destroy(&owner.input);owner^={} }
