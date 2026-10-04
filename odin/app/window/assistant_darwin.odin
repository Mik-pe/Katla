#+build darwin, arm64
//! A native assistant panel emits intent and displays borrowed main-thread service snapshots.
package window

import NS "core:sys/darwin/Foundation"
import "base:runtime"

/// Actions are dispatched on the native event thread; prompt text is borrowed during the callback.
Assistant_Action :: enum { Send, Cancel, New_Conversation }
/// Owns native controls only; application state and network jobs remain outside the window package.
Assistant_Panel :: struct {
    window:Window,
    target,input,output,model,status:^NS.Object,
    buttons:[Assistant_Action]^NS.Object,
    callback:proc(rawptr,Assistant_Action,string),
    state:rawptr,
    callback_context:runtime.Context,
}
@(private="package")
assistant_panel_action :: proc "c" (receiver:NS.id,selector:NS.SEL,sender:NS.id) {
    owner:^Assistant_Panel; NS.object_getInstanceVariable(receiver,"owner",&owner)
    if owner==nil || owner.callback==nil { return }
    context=owner.callback_context
    action:=Assistant_Action(send(NS.Integer,cast(^NS.Object)sender,"tag"))
    text:=""
    if action==.Send { value:=send(^NS.String,owner.input,"stringValue"); if value!=nil { text=string(value->UTF8String()) } }
    owner.callback(owner.state,action,text)
}
@(private="package")
assistant_panel_target :: proc(owner:^Assistant_Panel)->^NS.Object {
    cls:=NS.objc_lookUpClass("KatlaOdinAssistantPanel")
    if cls==nil {
        cls=NS.objc_allocateClassPair(NS.objc_lookUpClass("NSObject"),"KatlaOdinAssistantPanel",0)
        if cls==nil { return nil }
        if !bool(NS.class_addIvar(cls,"owner",size_of(rawptr),3,"^v")) || !bool(NS.class_addMethod(cls,NS.sel_registerName("changed:"),cast(NS.IMP)assistant_panel_action,"v@:@")) { NS.objc_disposeClassPair(cls); return nil }
        NS.objc_registerClassPair(cls)
    }
    target:=send(^NS.Object,cast(^NS.Object)cls,"alloc"); if target==nil { return nil }
    target=send(^NS.Object,target,"init"); if target!=nil { NS.object_setInstanceVariable(cast(NS.id)target,"owner",owner) }
    return target
}
@(private="package")
assistant_panel_label :: proc(owner:^Assistant_Panel,text:string,rect:NS.Rect,size:f64,mask:NS.UInteger)->^NS.Object {
    field:=send(^NS.Object,cast(^NS.Object)NS.objc_lookUpClass("NSTextField"),"alloc"); if field==nil { return nil }
    field=send(^NS.Object,field,"initWithFrame:",rect); if field==nil { return nil }
    control_string(field,"setStringValue:",text)
    send(nil,field,"setEditable:",NS.BOOL(false)); send(nil,field,"setSelectable:",NS.BOOL(true)); send(nil,field,"setBezeled:",NS.BOOL(false)); send(nil,field,"setDrawsBackground:",NS.BOOL(false))
    font:=send(^NS.Object,cast(^NS.Object)NS.objc_lookUpClass("NSFont"),"systemFontOfSize:",NS.Float(size)); send(nil,field,"setFont:",font)
    send(nil,field,"setAutoresizingMask:",mask); send(nil,owner.window.view,"addSubview:",field); field->release()
    return field
}
/// Creates a restrained native panel with readable progress, keyboard submission and ordinary actions.
assistant_panel_create :: proc(owner:^Assistant_Panel,state:rawptr,callback:proc(rawptr,Assistant_Action,string))->Window_Error {
    if state==nil || callback==nil { return .Native_Failure }
    owner.state=state; owner.callback=callback; owner.callback_context=context
    success:=false; defer { if !success { assistant_panel_destroy(owner) } }
    error:=window_create(&owner.window,"Assistant",500,560); if error!=.None { return error }
    send(nil,owner.window.native,"setContentMinSize:",NS.Size{500,520})
    appearance_name:=NS.String.alloc()->initWithOdinString("NSAppearanceNameDarkAqua")
    if appearance_name!=nil { defer appearance_name->release(); appearance:=send(^NS.Object,cast(^NS.Object)NS.objc_lookUpClass("NSAppearance"),"appearanceNamed:",appearance_name); if appearance!=nil { send(nil,owner.window.native,"setAppearance:",appearance) } }
    owner.target=assistant_panel_target(owner); if owner.target==nil { return .Native_Failure }
    if assistant_panel_label(owner,"Assistant",{{16,516},{468,28}},18,10)==nil { return .Native_Failure }
    owner.model=assistant_panel_label(owner,"Disabled",{{16,490},{468,20}},12,10); if owner.model==nil { return .Native_Failure }
    owner.status=assistant_panel_label(owner,"Choose an LLM configuration to enable the assistant.",{{16,438},{468,44}},12,10); if owner.status==nil { return .Native_Failure }
    control_string(owner.status,"setAccessibilityLabel:","Assistant status")
    status_cell:=send(^NS.Object,owner.status,"cell"); send(nil,status_cell,"setWraps:",NS.BOOL(true)); send(nil,status_cell,"setScrollable:",NS.BOOL(false)); send(nil,status_cell,"setUsesSingleLineMode:",NS.BOOL(false))
    scroll:=send(^NS.Object,cast(^NS.Object)NS.objc_lookUpClass("NSScrollView"),"alloc"); if scroll==nil { return .Native_Failure }
    scroll=send(^NS.Object,scroll,"initWithFrame:",NS.Rect{{16,154},{468,272}}); if scroll==nil { return .Native_Failure }
    send(nil,scroll,"setHasVerticalScroller:",NS.BOOL(true)); send(nil,scroll,"setAutoresizingMask:",NS.UInteger(18))
    owner.output=send(^NS.Object,cast(^NS.Object)NS.objc_lookUpClass("NSTextView"),"alloc"); if owner.output==nil { scroll->release(); return .Native_Failure }
    owner.output=send(^NS.Object,owner.output,"initWithFrame:",NS.Rect{{0,0},{468,272}}); if owner.output==nil { scroll->release(); return .Native_Failure }
    send(nil,owner.output,"setEditable:",NS.BOOL(false)); send(nil,owner.output,"setSelectable:",NS.BOOL(true)); send(nil,owner.output,"setRichText:",NS.BOOL(false)); send(nil,owner.output,"setTextContainerInset:",NS.Size{8,8}); send(nil,owner.output,"setAutoresizingMask:",NS.UInteger(2))
    font:=send(^NS.Object,cast(^NS.Object)NS.objc_lookUpClass("NSFont"),"systemFontOfSize:",NS.Float(13)); send(nil,owner.output,"setFont:",font); control_string(owner.output,"setAccessibilityLabel:","Assistant response")
    send(nil,owner.output,"setVerticallyResizable:",NS.BOOL(true)); send(nil,owner.output,"setHorizontallyResizable:",NS.BOOL(false)); send(nil,owner.output,"setMaxSize:",NS.Size{10000000,10000000})
    text_container:=send(^NS.Object,owner.output,"textContainer"); send(nil,text_container,"setContainerSize:",NS.Size{468,10000000}); send(nil,text_container,"setWidthTracksTextView:",NS.BOOL(true))
    send(nil,scroll,"setDocumentView:",owner.output); owner.output->release(); send(nil,owner.window.view,"addSubview:",scroll); scroll->release()
    if assistant_panel_label(owner,"Request",{{16,122},{468,20}},12,34)==nil { return .Native_Failure }
    owner.input=send(^NS.Object,cast(^NS.Object)NS.objc_lookUpClass("NSTextField"),"alloc"); if owner.input==nil { return .Native_Failure }
    owner.input=send(^NS.Object,owner.input,"initWithFrame:",NS.Rect{{16,82},{468,30}}); if owner.input==nil { return .Native_Failure }
    control_string(owner.input,"setPlaceholderString:","Describe a change to the scene…"); control_string(owner.input,"setAccessibilityLabel:","Assistant request")
    send(nil,owner.input,"setAutoresizingMask:",NS.UInteger(34)); send(nil,owner.input,"setTarget:",owner.target); send(nil,owner.input,"setTag:",NS.Integer(Assistant_Action.Send)); send(nil,owner.input,"setAction:",NS.sel_registerName("changed:")); send(nil,owner.window.view,"addSubview:",owner.input); owner.input->release()
    for label,i in ([3]string{"Send","Cancel","New conversation"}) {
        button:=send(^NS.Object,cast(^NS.Object)NS.objc_lookUpClass("NSButton"),"alloc"); if button==nil { return .Native_Failure }
        width:=NS.Float(176) if i==2 else NS.Float(128)
        button=send(^NS.Object,button,"initWithFrame:",NS.Rect{{NS.Float(16+i*140),36},{width,30}}); if button==nil { return .Native_Failure }
        control_string(button,"setTitle:",label); control_string(button,"setAccessibilityLabel:",label); send(nil,button,"setBezelStyle:",NS.UInteger(1)); send(nil,button,"setTag:",NS.Integer(i)); send(nil,button,"setTarget:",owner.target); send(nil,button,"setAction:",NS.sel_registerName("changed:")); send(nil,button,"setAutoresizingMask:",NS.UInteger(32))
        send(nil,owner.window.view,"addSubview:",button); button->release(); owner.buttons[Assistant_Action(i)]=button
    }
    if assistant_panel_label(owner,"Accepted scene edits use the editor’s Undo and Redo.",{{16,8},{468,20}},11,34)==nil { return .Native_Failure }
    send(nil,owner.window.native,"makeFirstResponder:",owner.input)
    success=true; return .None
}
/// Applies a main-thread service snapshot and disables unavailable actions.
assistant_panel_set :: proc(owner:^Assistant_Panel,model,status,output:string,can_send,can_cancel,can_reset:bool) {
    control_string(owner.model,"setStringValue:",model); control_string(owner.status,"setStringValue:",status); control_string(owner.output,"setString:",output)
    send(nil,owner.input,"setEnabled:",NS.BOOL(can_send)); send(nil,owner.buttons[.Send],"setEnabled:",NS.BOOL(can_send)); send(nil,owner.buttons[.Cancel],"setEnabled:",NS.BOOL(can_cancel)); send(nil,owner.buttons[.New_Conversation],"setEnabled:",NS.BOOL(can_reset))
}
/// Reads panel visibility when another native panel already pumps application events.
assistant_panel_state :: proc(owner:^Assistant_Panel)->State { return window_state(&owner.window) }
/// Pumps events for consumers which own this panel without another event-polling window.
assistant_panel_poll :: proc(owner:^Assistant_Panel)->State { return window_poll(&owner.window) }
/// Clears callback state before native teardown and releases the action target.
assistant_panel_destroy :: proc(owner:^Assistant_Panel) {
    owner.callback=nil; owner.state=nil
    if owner.target!=nil { NS.object_setInstanceVariable(cast(NS.id)owner.target,"owner",nil) }
    window_destroy(&owner.window)
    if owner.target!=nil { owner.target->release() }
    owner^={}
}
