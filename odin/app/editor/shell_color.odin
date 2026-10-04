//! Color channels retain the typed vector path and one shared captured gesture.
package editor_app
import ui "../../ui"
import "core:encoding/json"

@(private="package")
shell_color :: proc(shell:^Shell,component:string,field:^Inspector_Field,identity:u64,array:json.Array)->ui.Descriptor {
    values:=ui.Color{0,0,0,1}
    for value,i in array {
        #partial switch number in value {
        case json.Float:values[i]=f32(number)
        case json.Integer:values[i]=f32(number)
        }
    }
    labels:=[4]string{"Red","Green","Blue","Alpha"}
    children:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(children)
    append(&children,ui.Descriptor{key=key(identity,"swatch"),kind=.Stack,has_background=true,background=values,layout={height=ui.pixels(22),width=ui.percent(1)}},text(identity,field.label))
    for channel in 0..<len(array) {
        control_key:=key(identity,"channel",u64(channel)); state:=ui.state(shell.ctx,control_key,0,values[channel])
        if shell.ctx.captured.key!=control_key && shell.ctx.focused.key!=control_key { ui.state_set(shell.ctx,state,values[channel]) }
        append(&shell.bindings,Field_Binding{component=component,field=field,color_channel=channel,is_color=true})
        append(&children,ui.Descriptor{key=control_key,kind=.Slider,text=labels[channel],state=state,action=u64(Action.Field),payload=u64(len(shell.bindings)),minimum=field.constraints.min if field.constraints.has_min else 0,maximum=field.constraints.max if field.constraints.has_max else 1,step=.005,disabled=shell.state.owner.mode!=.Editing,layout={height=ui.pixels(30),width=ui.percent(1)}})
    }
    return {key=identity,kind=.Column,layout={gap={0,4},width=ui.percent(1)},children=nodes(shell,children[:])}
}
