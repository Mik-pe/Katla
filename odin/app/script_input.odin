//! Gameplay input is copied from the focused native viewport before a script tick.
package app
import ecs "../ecs"
import editor "../editor"
import "core:strings"

Script_Mouse_Button :: enum { Left, Right, Middle }
/// Names come from the application's configured action bindings and normalized physical keys.
Script_Input :: struct { actions,keys:[]string,mouse_delta:[2]f32,mouse_wheel:f32,buttons:bit_set[Script_Mouse_Button],focused:bool }
@(private="package")
script_input_destroy :: proc(value:rawptr) {
    input:=cast(^Script_Input)value
    for name in input.actions { delete(name) }; for name in input.keys { delete(name) }; delete(input.actions); delete(input.keys); input^={}
}
/// Replaces an owned frame snapshot only after every finite motion value passes preflight.
script_input_set :: proc(app:^Authoring,input:Script_Input)->editor.Scene_Error {
    for value in input.mouse_delta { if !finite_nonnegative(abs(value)) { return .Invalid_Field_Value } }
    if !finite_nonnegative(abs(input.mouse_wheel)) { return .Invalid_Field_Value }
    if len(input.actions)>256 || len(input.keys)>256 { return .Invalid_Field_Value }
    for names in ([2][]string{input.actions,input.keys}) { for name in names { if len(name)==0 || len(name)>128 || strings.contains(name,"\x00") { return .Invalid_Field_Value } } }
    context.allocator=app.world.allocator; staged:=input
    staged.actions=nil; staged.keys=nil
    if input.focused && app.mode==.Playing {
        staged.actions=make([]string,len(input.actions),app.world.allocator); for name,i in input.actions { staged.actions[i]=strings.clone(name) }
        staged.keys=make([]string,len(input.keys),app.world.allocator); for name,i in input.keys { staged.keys[i]=strings.clone(name) }
    } else { staged.focused=false; staged.mouse_delta={}; staged.mouse_wheel=0; staged.buttons={} }
    ecs.insert_resource(&app.world,staged,ecs.Value_Ops{destroy=script_input_destroy}); return .None
}
/// Motion and scroll are visible to every instance in one tick and consumed together afterward.
script_input_consume_motion :: proc(app:^Authoring) { input:=ecs.get_resource_mut(&app.world,Script_Input); if input!=nil { input.mouse_delta={}; input.mouse_wheel=0 } }
