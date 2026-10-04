//! Source attachment and live VM variables retain real instance generations across inspector refreshes.
package editor_app
import app ".."
import ecs "../../ecs"
import script "../../script"
import editor "../../editor"
import ui "../../ui"
import "core:encoding/json"
import "core:fmt"
import "core:strconv"

Script_Panel :: struct {variables:[]script.Variable,handle:script.Handle,entity:ecs.Entity_Id,has_entity:bool,next_poll:f64,error:editor.Scene_Error}
@(private="package")
script_panel_destroy :: proc(shell:^Shell) { script.variables_destroy(shell.script_panel.variables,shell.allocator); shell.script_panel={} }
/// Polls only the selected actual VM once per second; unchanged source is never compiled by the inspector each frame.
shell_script_poll :: proc(shell:^Shell,now:f64) {
    panel:=&shell.script_panel
    if !shell.state.selection.has_primary { if panel.has_entity { script_panel_destroy(shell) }; return }
    entity:=shell.state.selection.primary
    if _,present:=ecs.get_component(&shell.state.owner.world,entity,app.Script_Component); !present { if panel.has_entity { script_panel_destroy(shell) }; return }
    if panel.has_entity && panel.entity==entity && now<panel.next_poll { return }
    script.variables_destroy(panel.variables,shell.allocator); panel.variables=nil
    panel.entity=entity; panel.has_entity=true; panel.next_poll=now+1
    panel.variables,panel.handle,panel.error=app.script_inspect(shell.state.owner,entity)
}
@(private="package")
shell_script :: proc(shell:^Shell)->ui.Descriptor {
    source,present:=ecs.get_component(&shell.state.owner.world,shell.inspector.entity,app.Script_Component); if !present { return {} }
    children:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(children)
    append(&children,text(130,"Script source"))
    identity:=key(130,"path",u64(shell.inspector.entity)); state:=ui.state(shell.ctx,identity,0,source.path)
    if shell.ctx.focused.key!=identity { ui.state_set(shell.ctx,state,source.path) }
    append(&children,ui.Descriptor{key=identity,kind=.Text_Input,text="Path",placeholder="Resource-relative or absolute .luau path",state=state,action=u64(Action.Script_Path),disabled=shell.state.owner.mode!=.Editing,layout={height=ui.pixels(30),width=ui.percent(1)}},ui.Descriptor{key=key(130,"tools"),kind=.Row,layout={gap={6,0}},children=nodes(shell,{button("Edit source",.Script_Open,source.path==""),button("Reload source",.Script_Reload,source.path=="")})})
    status:=fmt.aprintf("%v · %s · %d consecutive errors",source.root,"disabled" if source.disabled else "active",source.consecutive_errors,allocator=shell.allocator); append(&shell.texts,status); append(&children,text(130,status))
    for error,index in source.last_errors { item:=text(131,error); item.key=key(131,"error",u64(index)); append(&children,item) }
    if shell.script_panel.has_entity && shell.script_panel.entity==shell.inspector.entity {
        if shell.script_panel.error!=.None { error:=fmt.aprintf("VM inspection: %v",shell.script_panel.error,allocator=shell.allocator); append(&shell.texts,error); append(&children,text(130,error)) }
        append(&children,text(130,"Live VM variables"))
        for variable,index in shell.script_panel.variables {
            control_key:=key(132,variable.name,u64(shell.inspector.entity)); control:=ui.Descriptor{key=control_key,text=variable.name,payload=u64(index+1),action=u64(Action.Script_Variable),layout={height=ui.pixels(30),width=ui.percent(1)}}
            switch value in variable.value {
            case f64:
                if f64(f32(value))!=value || value< -100000 || value>100000 { encoded:=fmt.aprintf("%.17g",value,allocator=shell.allocator); append(&shell.texts,encoded); control.kind=.Text_Input; control.state=ui.state(shell.ctx,control_key,0,encoded); if shell.ctx.focused.key!=control_key { ui.state_set(shell.ctx,control.state,encoded) }; append(&children,control); continue }
                control.kind=.Numeric_Input; control.minimum=-100000; control.maximum=100000; control.step=.01; control.state=ui.state(shell.ctx,control_key,0,f32(value)); if shell.ctx.focused.key!=control_key { ui.state_set(shell.ctx,control.state,f32(value)) }
            case bool:control.kind=.Checkbox; control.state=ui.state(shell.ctx,control_key,0,value); if shell.ctx.focused.key!=control_key { ui.state_set(shell.ctx,control.state,value) }
            case string:control.kind=.Text_Input; control.state=ui.state(shell.ctx,control_key,0,value); if shell.ctx.focused.key!=control_key { ui.state_set(shell.ctx,control.state,value) }
            }
            append(&children,control)
        }
    }
    return {key=key(130,"section"),kind=.Column,layout={gap={0,6},width=ui.percent(1)},children=nodes(shell,children[:])}
}
@(private="package")
shell_script_variable :: proc(shell:^Shell,payload:u64,value:script.Scalar) {
    panel:=&shell.script_panel; if payload==0 || payload>u64(len(panel.variables)) || !shell.state.selection.has_primary || panel.entity!=shell.state.selection.primary { shell.state.last_error=.Invalid_Operation; return }
    shell.state.last_error=app.script_set_variable(shell.state.owner,panel.handle,panel.variables[payload-1].name,value)
    panel.next_poll=0
}
@(private="package")
shell_script_click :: proc(shell:^Shell,event:ui.Click_Action)->bool {
    action:=Action(event.action); if action!=.Script_Open && action!=.Script_Reload { return false }
    if !shell.state.selection.has_primary { return true }; entity:=shell.state.selection.primary
    source,present:=ecs.get_component(&shell.state.owner.world,entity,app.Script_Component); if !present { return true }
    if action==.Script_Reload { shell.state.last_error=app.script_reload(shell.state.owner,entity); shell.script_panel.next_poll=0 }
    else { shell.state.last_error=code_document_open(&shell.code,source.root,source.path); if shell.state.last_error==.None { ui.dock_open(&shell.dock,ui.Tab_Id(Panel.Code)) } }
    return true
}
@(private="package")
shell_script_text :: proc(shell:^Shell,event:ui.Text_Action,value:string)->bool {
    action:=Action(event.action); if action!=.Script_Path && action!=.Script_Variable { return false }; if !event.submitted { return true }
    if action==.Script_Variable {
        scalar:script.Scalar=value
        if event.payload>0 && event.payload<=u64(len(shell.script_panel.variables)) { if _,numeric:=shell.script_panel.variables[event.payload-1].value.(f64); numeric { number,valid:=strconv.parse_f64(value); if !valid { shell.state.last_error=.Invalid_Field_Value; return true }; scalar=number } }
        shell_script_variable(shell,event.payload,scalar); return true
    }
    if !shell.state.selection.has_primary { return true }
    identity:=fmt.aprintf("%d",u64(shell.state.selection.primary),allocator=shell.allocator); defer delete(identity,shell.allocator)
    bytes,error:=json.marshal(struct {action,entity_id,path:string}{"set_script",identity,value},allocator=shell.allocator); if error==nil { execute(shell.state,{kind=.Application,tool_name="behavior",value=bytes}); delete(bytes,shell.allocator); shell.script_panel.next_poll=0 }
    return true
}
