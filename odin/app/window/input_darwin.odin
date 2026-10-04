#+build darwin, arm64
//! A real first-responder NSView implements text composition instead of synthesizing key text.
package window

import NS "core:sys/darwin/Foundation"
import ui "../../ui"
import "base:runtime"
import "core:strings"

Native_Input :: struct {
    window:^Window,
    input:Input_State,
    marked:string,
    marked_selection:NS.Range,
    ime:ui.IME_Request,
    callback_context:runtime.Context,
    close_requested:bool,
}
@(private="package")
native_input_owner :: proc "contextless" (receiver:NS.id)->^Native_Input { owner:^Native_Input; NS.object_getInstanceVariable(receiver,"owner",&owner); return owner }
@(private="package")
input_yes :: proc "c" (_:NS.id,_:NS.SEL)->NS.BOOL { return true }
@(private="package")
input_should_close :: proc "c" (receiver:NS.id,_:NS.SEL,_:NS.id)->NS.BOOL {
    owner:=native_input_owner(receiver)
    if owner!=nil { owner.close_requested=true }
    return false
}
@(private="package")
input_accept_mouse :: proc "c" (_:NS.id,_:NS.SEL,_:NS.id)->NS.BOOL { return true }
@(private="package")
input_pointer_event :: proc "c" (_:NS.id,_:NS.SEL,_:NS.id) {}
@(private="package")
input_key_down :: proc "c" (receiver:NS.id,_:NS.SEL,event:NS.id) {
    owner:=native_input_owner(receiver); if owner==nil { return }; context=owner.callback_context
    if !owner.ime.active { return }
    events:=send(^NS.Array,cast(^NS.Object)NS.objc_lookUpClass("NSArray"),"arrayWithObject:",event)
    send(nil,cast(^NS.Object)receiver,"interpretKeyEvents:",events)
}
@(private="package")
input_command :: proc "c" (_:NS.id,_:NS.SEL,_:NS.SEL) {}
@(private="package")
input_string :: proc(value:NS.id)->^NS.String {
    if bool(send(NS.BOOL,cast(^NS.Object)value,"isKindOfClass:",NS.objc_lookUpClass("NSAttributedString"))) { return send(^NS.String,cast(^NS.Object)value,"string") }
    return cast(^NS.String)value
}
@(private="package")
input_insert :: proc "c" (receiver:NS.id,_:NS.SEL,value:NS.id,_:NS.Range) {
    owner:=native_input_owner(receiver); if owner==nil { return }; context=owner.callback_context
    text:=input_string(value); if text==nil { return }
    input_text(&owner.input,string(text->UTF8String()))
    if owner.marked!="" { input_preedit(&owner.input,"",0,0) }
    delete(owner.marked,owner.input.allocator); owner.marked=""; owner.marked_selection={}
}
@(private="package")
input_marked :: proc "c" (receiver:NS.id,_:NS.SEL,value:NS.id,selection:NS.Range,_:NS.Range) {
    owner:=native_input_owner(receiver); if owner==nil { return }; context=owner.callback_context
    text:=input_string(value); if text==nil { return }
    next:=strings.clone(string(text->UTF8String()),owner.input.allocator)
    delete(owner.marked,owner.input.allocator); owner.marked=next; owner.marked_selection=selection
    length:=send(NS.UInteger,text,"length")
    start:=min(selection.location,length); end:=min(start+selection.length,length)
    prefix:=send(^NS.String,text,"substringToIndex:",start); through:=send(^NS.String,text,"substringToIndex:",end)
    input_preedit(&owner.input,next,len(string(prefix->UTF8String())),len(string(through->UTF8String())))
}
@(private="package")
input_unmark :: proc "c" (receiver:NS.id,_:NS.SEL) {
    owner:=native_input_owner(receiver); if owner==nil { return }; context=owner.callback_context
    delete(owner.marked,owner.input.allocator); owner.marked=""; owner.marked_selection={}; input_preedit(&owner.input,"",0,0)
}
@(private="package")
input_has_marked :: proc "c" (receiver:NS.id,_:NS.SEL)->NS.BOOL { owner:=native_input_owner(receiver); return owner!=nil && len(owner.marked)>0 }
@(private="package")
input_marked_range :: proc "c" (receiver:NS.id,_:NS.SEL)->NS.Range {
    owner:=native_input_owner(receiver); if owner==nil || owner.marked=="" { return {max(NS.UInteger),0} }; context=owner.callback_context
    text:=NS.String.alloc()->initWithOdinString(owner.marked); defer text->release(); return {0,send(NS.UInteger,text,"length")}
}
@(private="package")
input_selected_range :: proc "c" (receiver:NS.id,_:NS.SEL)->NS.Range { owner:=native_input_owner(receiver); if owner==nil { return {max(NS.UInteger),0} }; return owner.marked_selection }
@(private="package")
input_attributes :: proc "c" (_:NS.id,_:NS.SEL)->NS.id { return cast(NS.id)send(^NS.Array,cast(^NS.Object)NS.objc_lookUpClass("NSArray"),"array") }
@(private="package")
input_substring :: proc "c" (_:NS.id,_:NS.SEL,_:NS.Range,actual:^NS.Range)->NS.id { if actual!=nil { actual^={max(NS.UInteger),0} }; return nil }
@(private="package")
input_character_index :: proc "c" (_:NS.id,_:NS.SEL,_:NS.Point)->NS.UInteger { return max(NS.UInteger) }
@(private="package")
input_first_rect :: proc "c" (receiver:NS.id,_:NS.SEL,range:NS.Range,actual:^NS.Range)->NS.Rect {
    owner:=native_input_owner(receiver); if owner==nil { return {} }
    if actual!=nil { actual^=range }
    rect:=owner.ime.cursor
    window_rect:=send(NS.Rect,owner.window.view,"convertRect:toView:",NS.Rect{{NS.Float(rect.x),NS.Float(rect.y)},{NS.Float(max(1,rect.width)),NS.Float(max(1,rect.height))}},cast(^NS.View)nil)
    return send(NS.Rect,owner.window.native,"convertRectToScreen:",window_rect)
}
/// Installs the first-responder view before a graphics surface is attached to it.
native_input_init :: proc(owner:^Native_Input,window:^Window,allocator:=context.allocator)->Window_Error {
    if window.native==nil { return .Native_Failure }
    owner^={window=window,callback_context=context}; input_init(&owner.input,allocator)
    cls:=NS.objc_lookUpClass("KatlaOdinEditorView")
    if cls==nil {
        cls=NS.objc_allocateClassPair(NS.objc_lookUpClass("NSView"),"KatlaOdinEditorView",0); if cls==nil { input_destroy(&owner.input); return .Native_Failure }
        valid:=bool(NS.class_addIvar(cls,"owner",size_of(rawptr),3,"^v"))
        text_client:=NS.objc_getProtocol("NSTextInputClient")
        valid=text_client!=nil && bool(NS.class_addProtocol(cls,text_client)) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("windowShouldClose:"),cast(NS.IMP)input_should_close,"B@:@")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("acceptsFirstResponder"),cast(NS.IMP)input_yes,"B@:")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("isFlipped"),cast(NS.IMP)input_yes,"B@:")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("acceptsFirstMouse:"),cast(NS.IMP)input_accept_mouse,"B@:@")) && valid
        for selector in ([10]cstring{"mouseDown:","mouseUp:","mouseDragged:","rightMouseDown:","rightMouseUp:","rightMouseDragged:","otherMouseDown:","otherMouseUp:","otherMouseDragged:","mouseMoved:"}) {
            valid=bool(NS.class_addMethod(cls,NS.sel_registerName(selector),cast(NS.IMP)input_pointer_event,"v@:@")) && valid
        }
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("keyDown:"),cast(NS.IMP)input_key_down,"v@:@")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("doCommandBySelector:"),cast(NS.IMP)input_command,"v@::")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("insertText:replacementRange:"),cast(NS.IMP)input_insert,"v@:@{_NSRange=QQ}")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("setMarkedText:selectedRange:replacementRange:"),cast(NS.IMP)input_marked,"v@:@{_NSRange=QQ}{_NSRange=QQ}")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("unmarkText"),cast(NS.IMP)input_unmark,"v@:")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("hasMarkedText"),cast(NS.IMP)input_has_marked,"B@:")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("markedRange"),cast(NS.IMP)input_marked_range,"{_NSRange=QQ}@:")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("selectedRange"),cast(NS.IMP)input_selected_range,"{_NSRange=QQ}@:")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("validAttributesForMarkedText"),cast(NS.IMP)input_attributes,"@@:")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("attributedSubstringForProposedRange:actualRange:"),cast(NS.IMP)input_substring,"@@:{_NSRange=QQ}^{_NSRange=QQ}")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("characterIndexForPoint:"),cast(NS.IMP)input_character_index,"Q@:{CGPoint=dd}")) && valid
        valid=bool(NS.class_addMethod(cls,NS.sel_registerName("firstRectForCharacterRange:actualRange:"),cast(NS.IMP)input_first_rect,"{CGRect={CGPoint=dd}{CGSize=dd}}@:{_NSRange=QQ}^{_NSRange=QQ}")) && valid
        if !valid { NS.objc_disposeClassPair(cls); input_destroy(&owner.input); return .Native_Failure }
        NS.objc_registerClassPair(cls)
    }
    view:=send(^NS.View,cast(^NS.Object)cls,"alloc")
    view=send(^NS.View,view,"initWithFrame:",window.view->bounds())
    if view==nil { input_destroy(&owner.input); return .Native_Failure }
    NS.object_setInstanceVariable(cast(NS.id)view,"owner",owner)
    send(nil,window.native,"setContentView:",view); view->release(); window.view=view
    send(nil,window.native,"setDelegate:",view)
    send(nil,window.native,"setAcceptsMouseMovedEvents:",NS.BOOL(true)); send(NS.BOOL,window.native,"makeFirstResponder:",view)
    return .None
}
@(private="package")
native_modifiers :: proc(flags:NS.EventModifierFlags)->ui.Modifiers {
    result:ui.Modifiers
    if .Shift in flags { result|={.Shift} }; if .Control in flags { result|={.Control} }
    if .Option in flags { result|={.Alt} }; if .Command in flags { result|={.Super} }
    return result
}
@(private="package")
native_key :: proc(code:u16)->ui.Key {
    #partial switch NS.kVK(code) {
    case .ANSI_B: return .B
    case .ANSI_D: return .D
    case .ANSI_G: return .G
    case .ANSI_H: return .H
    case .ANSI_I: return .I
    case .ANSI_J: return .J
    case .ANSI_K: return .K
    case .ANSI_L: return .L
    case .ANSI_M: return .M
    case .ANSI_P: return .P
    case .ANSI_Q: return .Q
    case .ANSI_T: return .T
    case .ANSI_U: return .U
    case .ANSI_A: return .A
    case .ANSI_C: return .C
    case .ANSI_V: return .V
    case .ANSI_X: return .X
    case .ANSI_Y: return .Y
    case .ANSI_Z: return .Z
    case .ANSI_S: return .S
    case .ANSI_O: return .O
    case .ANSI_N: return .N
    case .ANSI_F: return .F
    case .ANSI_W: return .W
    case .ANSI_E: return .E
    case .ANSI_R: return .R
    case .ANSI_Comma: return .Comma
    case .ANSI_0: return .Num0
    case .ANSI_1: return .Num1
    case .ANSI_2: return .Num2
    case .ANSI_3: return .Num3
    case .ANSI_4: return .Num4
    case .ANSI_5: return .Num5
    case .ANSI_6: return .Num6
    case .ANSI_7: return .Num7
    case .ANSI_8: return .Num8
    case .ANSI_9: return .Num9
    case .ANSI_Period: return .Period
    case .ANSI_Slash: return .Slash
    case .ANSI_Backslash: return .Backslash
    case .ANSI_Minus: return .Minus
    case .ANSI_Equal: return .Equal
    case .ANSI_LeftBracket: return .Left_Bracket
    case .ANSI_RightBracket: return .Right_Bracket
    case .ANSI_Quote: return .Quote
    case .ANSI_Semicolon: return .Semicolon
    case .ANSI_Grave: return .Backtick

    case .Tab: return .Tab
    case .Return,.ANSI_KeypadEnter: return .Enter
    case .Escape: return .Escape
    case .Delete: return .Backspace
    case .ForwardDelete: return .Delete
    case .LeftArrow: return .Left
    case .RightArrow: return .Right
    case .UpArrow: return .Up
    case .DownArrow: return .Down
    case .Home: return .Home
    case .End: return .End
    case .PageUp: return .Page_Up
    case .PageDown: return .Page_Down
    case .Space: return .Space
    }
    return .None
}
@(private="package")
native_input_event :: proc(state:rawptr,event:^NS.Event) {
    owner:=cast(^Native_Input)state
    if event->window()!=owner.window.native { return }
    input:=&owner.input; input.modifiers=native_modifiers(event->modifierFlags())
    kind:=event->type()
    #partial switch kind {
    case .LeftMouseDown,.LeftMouseUp,.RightMouseDown,.RightMouseUp,.OtherMouseDown,.OtherMouseUp,.MouseMoved,.LeftMouseDragged,.RightMouseDragged,.OtherMouseDragged,.ScrollWheel:
        point:=send(NS.Point,owner.window.view,"convertPoint:fromView:",event->locationInWindow(),cast(^NS.View)nil)
        position:=ui.Vec2{f32(point.x),f32(point.y)}
        if kind==.MouseMoved || kind==.LeftMouseDragged || kind==.RightMouseDragged || kind==.OtherMouseDragged {
            input.delta+=position-input.pointer; append(&input.events,ui.Pointer_Move{position})
        } else if kind==.ScrollWheel {
            scale:=f32(0.1) if bool(send(NS.BOOL,event,"hasPreciseScrollingDeltas")) else f32(1)
            delta:=ui.Vec2{f32(event->deltaX())*scale,f32(event->deltaY())*scale}; input.wheel+=delta[1]; append(&input.events,ui.Scroll{position,delta})
        } else {
            button:=ui.Pointer_Button.Left; if kind==.RightMouseDown || kind==.RightMouseUp { button=.Right }
            if kind==.OtherMouseDown || kind==.OtherMouseUp { if event->buttonNumber()!=2 { return }; button=.Middle }
            down:=kind==.LeftMouseDown || kind==.RightMouseDown || kind==.OtherMouseDown
            if down { input.buttons|={button}; append(&input.events,ui.Pointer_Down{position,button,input.modifiers,u32(max(1,event->clickCount()))}) }
            else { input.buttons-={button}; append(&input.events,ui.Pointer_Up{position,button,input.modifiers}) }
        }
        input.pointer=position
    case .KeyDown,.KeyUp:
        code:=send(u16,event,"keyCode"); key:=native_key(code); if key!=.None { if kind==.KeyDown { input.keys|={key} } else { input.keys-={key} } }
        if kind==.KeyDown { append(&input.events,ui.Key_Down{native_key(code),input.modifiers,bool(send(NS.BOOL,event,"isARepeat"))}) }
        else { append(&input.events,ui.Key_Up{native_key(code),input.modifiers}) }
    case .FlagsChanged:
    case: return
    }
}
/// Pumps AppKit, preserving pointer capture beyond the original widget bounds.
native_input_poll :: proc(owner:^Native_Input,time:f64)->(State,ui.Input) {
    input_begin(&owner.input)
    focused:=bool(send(NS.BOOL,owner.window.native,"isKeyWindow")) && bool(send(NS.BOOL,owner.window.application,"isActive"))
    input_focus(&owner.input,focused)
    state:=window_poll(owner.window,owner,native_input_event)
    focused=bool(send(NS.BOOL,owner.window.native,"isKeyWindow")) && bool(send(NS.BOOL,owner.window.application,"isActive"))
    input_focus(&owner.input,focused)
    scale:=f32(send(NS.Float,owner.window.native,"backingScaleFactor"))
    return state,ui.Input{owner.input.events[:],time,scale}
}
/// Updates the native candidate-window position after the UI has resolved its caret geometry.
native_input_ime :: proc(owner:^Native_Input,request:ui.IME_Request) {
    was_active:=owner.ime.active; owner.ime=request
    if request.active { send(NS.BOOL,owner.window.native,"makeFirstResponder:",owner.window.view) }
    input_context:=send(^NS.Object,owner.window.view,"inputContext")
    if input_context==nil { return }
    if was_active && !request.active { send(nil,input_context,"discardMarkedText") }
    if request.active { send(nil,input_context,"invalidateCharacterCoordinates") }
}
native_input_destroy :: proc(owner:^Native_Input) {
    if owner.window!=nil && owner.window.native!=nil { send(nil,owner.window.native,"setDelegate:",cast(rawptr)nil) }
    if owner.window!=nil && owner.window.view!=nil { NS.object_setInstanceVariable(cast(NS.id)owner.window.view,"owner",cast(rawptr)nil) }
    delete(owner.marked,owner.input.allocator); input_destroy(&owner.input); owner^={}
}
