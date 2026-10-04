#+build darwin, arm64
//! Native controls emit user intent; authored components and history stay in the application.
package window

import NS "core:sys/darwin/Foundation"
import "base:runtime"
import "core:fmt"

/// Named scalar controls, followed by ordinary shared-history actions.
Control_Field :: enum { Red, Green, Blue, Alpha, Metallic, Roughness, Occlusion, Undo, Redo, Preset }
/// Pointer previews are completed once native tracking releases the pointer.
Control_Phase :: enum { Preview, Finish, Cancel, Activate }
Control_Event :: struct { field:Control_Field, phase:Control_Phase, value:f32 }
/// The panel owns its native widgets and action target; callback state must remain stationary.
Controls :: struct {
    window:Window,
    target:^NS.Object,
    fields:[Control_Field]^NS.Object,
    values:[7]^NS.Object,
    status:^NS.Object,
    callback:proc(rawptr,Control_Event),
    state:rawptr,
    callback_context:runtime.Context,
    tracking:bool,
    last_field:Control_Field,
}

@(private="package")
control_action :: proc "c" (receiver:NS.id,selector:NS.SEL,sender:NS.id) {
    owner:^Controls
    NS.object_getInstanceVariable(receiver,"owner",&owner)
    if owner==nil || owner.callback==nil { return }
    context=owner.callback_context
    field:=Control_Field(send(NS.Integer,cast(^NS.Object)sender,"tag"))
    if field==.Undo || field==.Redo || field==.Preset {
        value:=f32(send(NS.Integer,cast(^NS.Object)sender,"indexOfSelectedItem")) if field==.Preset else f32(0)
        owner.callback(owner.state,{field,.Activate,value}); return
    }
    owner.tracking=true; owner.last_field=field
    value:=f32(send(f64,cast(^NS.Object)sender,"doubleValue"))
    control_value_label(owner,field,value)
    owner.callback(owner.state,{field,.Preview,value})
    current:=send(^NS.Event,owner.window.application,"currentEvent")
    if current==nil || current->type()==.KeyDown || current->type()==.KeyUp {
        owner.tracking=false; owner.callback(owner.state,{field,.Finish,value})
    }
}

@(private="package")
control_target :: proc(owner:^Controls)->^NS.Object {
    cls:=NS.objc_lookUpClass("KatlaOdinMaterialControls")
    if cls==nil {
        cls=NS.objc_allocateClassPair(NS.objc_lookUpClass("NSObject"),"KatlaOdinMaterialControls",0)
        if cls==nil { return nil }
        if !bool(NS.class_addIvar(cls,"owner",size_of(rawptr),3,"^v")) || !bool(NS.class_addMethod(cls,NS.sel_registerName("changed:"),cast(NS.IMP)control_action,"v@:@")) { NS.objc_disposeClassPair(cls); return nil }
        NS.objc_registerClassPair(cls)
    }
    target:=send(^NS.Object,cast(^NS.Object)cls,"alloc")
    if target==nil { return nil }
    target=send(^NS.Object,target,"init")
    if target!=nil { NS.object_setInstanceVariable(cast(NS.id)target,"owner",owner) }
    return target
}

@(private="package")
control_string :: proc(object:^NS.Object, $selector:string,value:string) {
    text:=NS.String.alloc()->initWithOdinString(value)
    if text!=nil { send(nil,object,selector,text); text->release() }
}
@(private="package")
control_label :: proc(owner:^Controls,text:string,rect:NS.Rect,size:f64)->^NS.Object {
    cls:=NS.objc_lookUpClass("NSTextField")
    if cls==nil { return nil }
    field:=send(^NS.Object,cast(^NS.Object)cls,"alloc")
    if field==nil { return nil }
    field=send(^NS.Object,field,"initWithFrame:",rect)
    if field==nil { return nil }
    control_string(field,"setStringValue:",text)
    send(nil,field,"setEditable:",NS.BOOL(false)); send(nil,field,"setSelectable:",NS.BOOL(false)); send(nil,field,"setBezeled:",NS.BOOL(false)); send(nil,field,"setDrawsBackground:",NS.BOOL(false))
    font:=send(^NS.Object,cast(^NS.Object)NS.objc_lookUpClass("NSFont"),"systemFontOfSize:",NS.Float(size)); send(nil,field,"setFont:",font)
    send(nil,owner.window.view,"addSubview:",field); field->release()
    return field
}
@(private="package")
control_value_label :: proc(owner:^Controls,field:Control_Field,value:f32) {
    if int(field)>=len(owner.values) { return }
    text:=fmt.aprintf("%.2f",value); defer delete(text)
    control_string(owner.values[field],"setStringValue:",text)
}
/// Creates accessible macOS sliders and history buttons with no application component dependency.
controls_create :: proc(owner:^Controls,state:rawptr,callback:proc(rawptr,Control_Event))->Window_Error {
    if callback==nil || state==nil { return .Native_Failure }
    owner.callback=callback; owner.state=state; owner.callback_context=context
    success:=false; defer { if !success { controls_destroy(owner) } }
    error:=window_create(&owner.window,"Material · Selected sphere",304,480); if error!=.None { return error }
    send(nil,owner.window.native,"setContentMinSize:",NS.Size{304,480})
    owner.target=control_target(owner); if owner.target==nil { return .Native_Failure }
    name:=NS.String.alloc()->initWithOdinString("NSAppearanceNameDarkAqua"); if name!=nil { defer name->release(); appearance:=send(^NS.Object,cast(^NS.Object)NS.objc_lookUpClass("NSAppearance"),"appearanceNamed:",name); if appearance!=nil { send(nil,owner.window.native,"setAppearance:",appearance) } }
    if control_label(owner,"Material",{{16,437},{272,26}},18)==nil { return .Native_Failure }
    if control_label(owner,"Left sphere · sRGB color",{{16,412},{272,20}},12)==nil { return .Native_Failure }
    names:=[7]string{"Red","Green","Blue","Alpha","Metallic","Roughness","Occlusion"}
    for label,i in names {
        y:=NS.Float(376-i*38)
        if control_label(owner,label,{{16,y+3},{72,20}},12)==nil { return .Native_Failure }
        owner.values[i]=control_label(owner,"0.00",{{252,y+3},{40,20}},12); if owner.values[i]==nil { return .Native_Failure }
        cls:=NS.objc_lookUpClass("NSSlider"); if cls==nil { return .Native_Failure }
        slider:=send(^NS.Object,cast(^NS.Object)cls,"alloc"); if slider==nil { return .Native_Failure }
        slider=send(^NS.Object,slider,"initWithFrame:",NS.Rect{{92,y},{152,24}}); if slider==nil { return .Native_Failure }
        send(nil,slider,"setMinValue:",f64(0)); send(nil,slider,"setMaxValue:",f64(1)); send(nil,slider,"setContinuous:",NS.BOOL(true)); send(nil,slider,"setTag:",NS.Integer(i)); send(nil,slider,"setTarget:",owner.target); send(nil,slider,"setAction:",NS.sel_registerName("changed:")); control_string(slider,"setAccessibilityLabel:",label)
        send(nil,owner.window.view,"addSubview:",slider); slider->release(); owner.fields[Control_Field(i)]=slider
    }
    if control_label(owner,"Preset",{{16,84},{72,20}},12)==nil { return .Native_Failure }
    popup:=send(^NS.Object,cast(^NS.Object)NS.objc_lookUpClass("NSPopUpButton"),"alloc"); if popup==nil { return .Native_Failure }
    popup=send(^NS.Object,popup,"initWithFrame:pullsDown:",NS.Rect{{92,80},{196,28}},NS.BOOL(false)); if popup==nil { return .Native_Failure }
    for name in ([6]string{"Plaster","Oak","Concrete","Ceramic","Brushed metal","Fabric"}) { control_string(popup,"addItemWithTitle:",name) }
    send(nil,popup,"setTag:",NS.Integer(Control_Field.Preset)); send(nil,popup,"setTarget:",owner.target); send(nil,popup,"setAction:",NS.sel_registerName("changed:")); control_string(popup,"setAccessibilityLabel:","Material preset")
    send(nil,owner.window.view,"addSubview:",popup); popup->release(); owner.fields[.Preset]=popup
    for label,i in ([2]string{"Undo","Redo"}) {
        field:=Control_Field(int(Control_Field.Undo)+i)
        button:=send(^NS.Object,cast(^NS.Object)NS.objc_lookUpClass("NSButton"),"alloc"); if button==nil { return .Native_Failure }
        button=send(^NS.Object,button,"initWithFrame:",NS.Rect{{NS.Float(16+i*136),40},{128,30}}); if button==nil { return .Native_Failure }
        control_string(button,"setTitle:",label); control_string(button,"setAccessibilityLabel:",label)
        send(nil,button,"setBezelStyle:",NS.UInteger(1)); send(nil,button,"setTag:",NS.Integer(field)); send(nil,button,"setTarget:",owner.target); send(nil,button,"setAction:",NS.sel_registerName("changed:"))
        send(nil,owner.window.view,"addSubview:",button); button->release(); owner.fields[field]=button
    }
    owner.status=control_label(owner,"One undo step per drag",{{16,10},{272,20}},11); if owner.status==nil { return .Native_Failure }
    success=true; return .None
}
/// Synchronizes exact scalar values and disabled history actions after accepted edits.
controls_set :: proc(owner:^Controls,values:[7]f32,can_undo,can_redo:bool,status:string) {
    for value,i in values { send(nil,owner.fields[Control_Field(i)],"setDoubleValue:",f64(value)); control_value_label(owner,Control_Field(i),value) }
    send(nil,owner.fields[.Undo],"setEnabled:",NS.BOOL(can_undo)); send(nil,owner.fields[.Redo],"setEnabled:",NS.BOOL(can_redo)); control_string(owner.status,"setStringValue:",status)
}
/// Empty selections keep history available while disabling material edits.
controls_selection :: proc(owner:^Controls,available:bool) {
    for field in Control_Field { if field!=.Undo && field!=.Redo { send(nil,owner.fields[field],"setEnabled:",NS.BOOL(available)) } }
}
@(private="package")
controls_event :: proc(data:rawptr,event:^NS.Event) {
    owner:=cast(^Controls)data
    if owner.tracking && (event->type()==.LeftMouseDown || event->type()==.LeftMouseUp) { owner.tracking=false; owner.callback(owner.state,{owner.last_field,.Finish,0}) }
    if event->type()==.KeyDown && event->keyCode()==53 { if owner.tracking { owner.tracking=false }; owner.callback(owner.state,{owner.last_field,.Cancel,0}) }
}
/// Native sliders retain pointer capture outside their rows and finish after release.
controls_poll :: proc(owner:^Controls)->State { return window_poll(&owner.window,owner,controls_event) }
/// Releases callbacks before destroying their native widget tree.
controls_destroy :: proc(owner:^Controls) {
    window_destroy(&owner.window)
    if owner.target!=nil { NS.object_setInstanceVariable(cast(NS.id)owner.target,"owner",nil); owner.target->release() }
    owner^={}
}
