#+build linux, windows
//! SDL owns portable native windows; the GPU receives only explicit borrowed surface handles.
package window

import wn "../../deps/window_native"
import gfx "../../gfx"
import ui "../../ui"
import "core:strings"
import "core:log"

Window_Error :: enum { None,Invalid_Size,Native_Failure }
State :: struct { width,height:u32,visible,closed:bool }
Window :: struct { native:rawptr,api:wn.API,closed:bool,error:Window_Error }
@(private="package")
settings:struct { library,loader:string }
/// Configures explicit dependency paths before creating any native windows; borrowed strings outlive them.
window_configure :: proc(library,loader:string) { settings.library=library;settings.loader=loader }
window_create :: proc(owner:^Window,title:string,width,height:u32)->Window_Error {
    if owner.native!=nil { return .Native_Failure }
    if width==0 || height==0 || width>16384 || height>16384 { return .Invalid_Size }
    if settings.library=="" { return .Native_Failure }
    api,error:=wn.load(settings.library); if error!=.None { log.error("Cannot load native window dependency",settings.library,error);return .Native_Failure }
    name:=strings.clone_to_cstring(title);defer delete(name)
    loader:=strings.clone_to_cstring(settings.loader);defer delete(loader)
    handle:=api.create(name,width,height,loader)
    if handle==nil { log.error("Cannot create native window",string(api.error()));wn.unload(&api);return .Native_Failure }
    owner^={native=handle,api=api};return .None
}
window_state :: proc(owner:^Window)->State {
    if owner.native==nil { return {closed=true} }
    value:wn.State
    if owner.api.state(owner.native,&value)==0 { owner.error=.Native_Failure;return {closed=true} }
    return {value.width,value.height,value.visible!=0,owner.closed}
}
/// Native SDL events are dispatched by native_input_poll; auxiliary consumers can borrow each flattened event.
window_poll :: proc(owner:^Window,state:rawptr=nil,handler:proc(rawptr,^wn.Event)=nil)->State {
    if owner.native==nil { return {closed=true} }
    event:wn.Event
    for {
        result:=owner.api.poll(owner.native,&event); if result<0 { owner.error=.Native_Failure;owner.closed=true;break };if result==0 { break }
        if event.kind==.Close && handler==nil { owner.closed=true }
        if handler!=nil { handler(state,&event) }
    }
    return window_state(owner)
}
window_resize :: proc(owner:^Window,width,height:u32)->Window_Error {
    if width==0 || height==0 || width>16384 || height>16384 { return .Invalid_Size }
    if owner.native==nil || owner.api.resize(owner.native,width,height)==0 { return .Native_Failure };return .None
}
/// The explicit type prevents interpreting a Wayland wl_surface as an Xlib Window.
window_surface :: proc(owner:^Window)->gfx.Surface_Desc {
    value:wn.Surface;state:=window_state(owner)
    if owner.native==nil || owner.api.surface(owner.native,&value)==0 { owner.error=.Native_Failure;return {} }
    kind:gfx.Surface_Kind
    switch value.kind {
    case 2:kind=.Xlib
    case 3:kind=.Wayland
    case 4:kind=.Win32
    case:owner.error=.Native_Failure;return {}
    }
    return {view=value.view,display=value.display,width=state.width,height=state.height,kind=kind}
}
window_view :: proc(owner:^Window)->rawptr { return window_surface(owner).view }
window_destroy :: proc(owner:^Window) { if owner.native!=nil { owner.api.destroy(owner.native) };wn.unload(&owner.api);owner^={} }
/// Portable native frames have no autorelease-pool owner.
frame_begin :: proc()->rawptr { return nil }
frame_end :: proc(_:rawptr) {}
@(private="package")
clipboard_read :: proc(state:rawptr)->string {
    owner:=cast(^Window)state;if owner==nil || owner.native==nil { return "" }
    text:=owner.api.clipboard_get(owner.native);if text==nil { owner.error=.Native_Failure;return "" };return string(text)
}
@(private="package")
clipboard_write :: proc(state:rawptr,text:string) {
    owner:=cast(^Window)state;if owner==nil || owner.native==nil { return }
    value:=strings.clone_to_cstring(text);defer delete(value)
    if owner.api.clipboard_set(owner.native,value)==0 { owner.error=.Native_Failure }
}
/// Returned text is borrowed until the next clipboard read; retained UI copies it while applying a paste.
clipboard_provider :: proc(owner:^Window)->ui.Clipboard_Provider { return {owner,clipboard_read,clipboard_write} }
