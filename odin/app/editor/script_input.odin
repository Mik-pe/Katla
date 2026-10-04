//! Configured physical gameplay bindings read only the focused viewport's owned native snapshot.
package editor_app
import app ".."
import window "../window"
import ui "../../ui"

/// Supplies the exposed Rust default action names without deriving text from physical key events.
shell_script_input :: proc(shell:^Shell,input:^window.Input_State) {
    focused:=input.focused && shell.state.owner.mode==.Playing && shell.viewports.has_active && shell.ctx.focused.key==key(20,"image",u64(shell.viewports.active)) && shell.ctx.modal.key==0 && shell.ctx.popup.key==0
    actions:=make([dynamic]string,shell.allocator); keys:=make([dynamic]string,shell.allocator); defer delete(actions); defer delete(keys)
    bindings:=[11]struct{key:ui.Key,name,physical:string}{{.Space,"jump","space"},{.W,"move_forward","w"},{.S,"move_backward","s"},{.A,"move_left","a"},{.D,"move_right","d"},{.E,"move_up","e"},{.Q,"move_down","q"},{.I,"inventory","i"},{.P,"pause","p"},{.Escape,"exit","escape"},{.L,"look_enable","l"}}
    for binding in bindings { if binding.key in input.keys { append(&actions,binding.name); append(&keys,binding.physical) } }
    if .Shift in input.modifiers { append(&actions,"sprint"); append(&keys,"left_shift") }
    if .Control in input.modifiers { append(&actions,"slow"); append(&keys,"left_control") }
    if .Right in input.buttons { append(&actions,"pan_enable" if .Control in input.modifiers else "look_enable") }
    if .Left in input.buttons { append(&actions,"interact") }
    buttons:bit_set[app.Script_Mouse_Button]
    if .Left in input.buttons { buttons|={.Left} }; if .Right in input.buttons { buttons|={.Right} }; if .Middle in input.buttons { buttons|={.Middle} }
    error:=app.script_input_set(shell.state.owner,{actions=actions[:],keys=keys[:],mouse_delta=input.delta,mouse_wheel=input.wheel,buttons=buttons,focused=focused})
    if error!=.None { shell.state.last_error=error }
}
