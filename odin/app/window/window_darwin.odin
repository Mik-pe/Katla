#+build darwin, arm64
//! Native window lifecycle belongs to the application; graphics owns only its attached surface.
package window

import NS "core:sys/darwin/Foundation"
import "base:intrinsics"
import "core:math"

foreign import AppKit "system:AppKit.framework"
foreign AppKit { NSApplicationLoad :: proc "c" ()->NS.BOOL --- }
@(private="package")
send :: intrinsics.objc_send
/// A main-thread NSWindow owner; detach/drain graphics before destroying it.
Window :: struct { application:^NS.Application, native:^NS.Window, view:^NS.View }
/// Native construction failures leave no retained window owner.
Window_Error :: enum { None, Invalid_Size, Native_Failure }
/// Describes the physical drawable extent and current window visibility.
State :: struct { width,height:u32, visible,closed:bool }
/// Creates a real resizable titled window; the application retains its NSView parent.
window_create :: proc(window:^Window,title:string,width,height:u32)->Window_Error {
    if width==0 || height==0 || width>16384 || height>16384 { return .Invalid_Size }
    _=NS.scoped_autoreleasepool()
    if !bool(NSApplicationLoad()) { return .Native_Failure }
    application:=NS.Application.sharedApplication()
    if application==nil { return .Native_Failure }
    if send(NS.Integer,application,"activationPolicy")!=NS.Integer(NS.ActivationPolicy.Regular) && !bool(application->setActivationPolicy(.Regular)) { return .Native_Failure }
    native:=NS.Window.alloc()->initWithContentRect({{0,0},{NS.Float(width),NS.Float(height)}},{.Titled,.Closable,.Resizable,.Miniaturizable},.Buffered,false)
    if native==nil { return .Native_Failure }
    native->setReleasedWhenClosed(false)
    name:=NS.String.alloc()->initWithOdinString(title)
    if name==nil { native->release(); return .Native_Failure }; defer name->release()
    native->setTitle(name)
    native->center()
    view:=native->contentView()
    if view==nil { native->release(); return .Native_Failure }
    window^={application,native,view}
    send(nil,application,"finishLaunching")
    native->makeKeyAndOrderFront(nil)
    application->activateIgnoringOtherApps(true)
    return .None
}
/// Pumps native events without blocking the GPU owner frame loop.
window_poll :: proc(window:^Window,user_data:rawptr=nil,event_handler:proc(rawptr,^NS.Event)=nil)->State {
    if window.native==nil { return {closed=true} }
    _=NS.scoped_autoreleasepool()
    for {
        event:=window.application->nextEventMatchingMask(NS.EventMaskAny,NS.Date.distantPast(),NS.DefaultRunLoopMode,true)
        if event==nil { break }
        window.application->sendEvent(event)
        if event_handler!=nil { event_handler(user_data,event) }
    }
    send(nil,window.application,"updateWindows")
    return window_state(window)
}
/// Reads actual backing pixel dimensions; logical points are never used as drawable extents.
window_state :: proc(window:^Window)->State {
    if window.native==nil { return {closed=true} }
    visible:=bool(send(NS.BOOL,window.native,"isVisible"))
    rect:=window.native->convertRectToBacking(window.view->bounds())
    width,height:=max(f64(0),f64(rect.size.width)),max(f64(0),f64(rect.size.height))
    minimized:=bool(send(NS.BOOL,window.native,"isMiniaturized"))
    return {u32(math.round(width)),u32(math.round(height)),visible,!visible && !minimized}
}
/// Resizes logical content points, then reports the real drawable extent through window_state.
window_resize :: proc(window:^Window,width,height:u32)->Window_Error {
    if window.native==nil { return .Native_Failure }
    if width==0 || height==0 || width>16384 || height>16384 { return .Invalid_Size }
    send(nil,window.native,"setContentSize:",NS.Size{NS.Float(width),NS.Float(height)})
    return .None
}
/// Returns the retained-parent NSView borrowed by native surface initialization.
window_view :: proc(window:^Window)->rawptr { return window.view }
/// Places an auxiliary panel beside the scene while keeping its frame on the visible display.
window_place_beside :: proc(window,scene:^Window) {
    if window.native==nil || scene.native==nil { return }
    frame:=window.native->frame(); peer:=scene.native->frame()
    screen:=scene.native->screen(); if screen==nil { return }
    visible:=send(NS.Rect,screen,"visibleFrame")
    x:=clamp(peer.origin.x+peer.size.width+12,visible.origin.x,visible.origin.x+visible.size.width-frame.size.width)
    y:=clamp(peer.origin.y+peer.size.height-frame.size.height,visible.origin.y,visible.origin.y+visible.size.height-frame.size.height)
    send(nil,window.native,"setFrameOrigin:",NS.Point{x,y})
}
/// Closes/releases the window after its graphics surface has been detached.
window_destroy :: proc(window:^Window) {
    if window.native!=nil { window.native->close(); window.native->release() }
    window^={}
}
