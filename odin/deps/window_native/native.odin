//! Source-pinned SDL3 window/input C ABI; the application owns all policy and resources.
package window_native

import "core:dynlib"

State :: struct { width,height:u32,scale,density:f32,visible,focused,closed:u32 }
Event_Kind :: enum u32 { None,Close,Focus,Key_Down,Key_Up,Motion,Button_Down,Button_Up,Wheel,Text,Editing }
Event :: struct { kind:Event_Kind,code,modifiers,button,clicks,repeat:u32,x,y,dx,dy:f32,start,length:i32,text:cstring }
Surface :: struct { kind:u32,view,display:rawptr }
Error :: enum { None,Library,ABI }
API :: struct {
    library:dynlib.Library,
    abi:proc "c" ()->u32,
    create:proc "c" (cstring,u32,u32,cstring)->rawptr,
    destroy:proc "c" (rawptr),
    state:proc "c" (rawptr,^State)->i32,
    poll:proc "c" (rawptr,^Event)->i32,
    resize:proc "c" (rawptr,u32,u32)->i32,
    surface:proc "c" (rawptr,^Surface)->i32,
    ime:proc "c" (rawptr,u32,i32,i32,i32,i32)->i32,
    clipboard_get:proc "c" (rawptr)->cstring,
    clipboard_set:proc "c" (rawptr,cstring)->i32,
    error:proc "c" ()->cstring,
}
/// Resolves every mandatory function before a native owner can be created.
load :: proc(path:string)->(API,Error) {
    library,ok:=dynlib.load_library(path); if !ok { return {},.Library }
    api:=API{library=library}
    api.abi=cast(type_of(api.abi))dynlib.symbol_address(library,"katla_window_abi")
    api.create=cast(type_of(api.create))dynlib.symbol_address(library,"katla_window_create")
    api.destroy=cast(type_of(api.destroy))dynlib.symbol_address(library,"katla_window_destroy")
    api.state=cast(type_of(api.state))dynlib.symbol_address(library,"katla_window_state")
    api.poll=cast(type_of(api.poll))dynlib.symbol_address(library,"katla_window_poll")
    api.resize=cast(type_of(api.resize))dynlib.symbol_address(library,"katla_window_resize")
    api.surface=cast(type_of(api.surface))dynlib.symbol_address(library,"katla_window_surface")
    api.ime=cast(type_of(api.ime))dynlib.symbol_address(library,"katla_window_ime")
    api.clipboard_get=cast(type_of(api.clipboard_get))dynlib.symbol_address(library,"katla_window_clipboard_get")
    api.clipboard_set=cast(type_of(api.clipboard_set))dynlib.symbol_address(library,"katla_window_clipboard_set")
    api.error=cast(type_of(api.error))dynlib.symbol_address(library,"katla_window_error")
    if api.abi==nil || api.create==nil || api.destroy==nil || api.state==nil || api.poll==nil || api.resize==nil || api.surface==nil || api.ime==nil || api.clipboard_get==nil || api.clipboard_set==nil || api.error==nil || api.abi()!=1 { dynlib.unload_library(library); return {},.ABI }
    return api,.None
}
/// Destroy every native window owner before unloading the dependency.
unload :: proc(api:^API) { if api.library!=nil { dynlib.unload_library(api.library) }; api^={} }
