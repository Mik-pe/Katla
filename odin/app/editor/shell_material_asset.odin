//! Material capture and apply use complete portable assets and the shared scene history.
package editor_app
import ui "../../ui"
import "core:encoding/json"
import "core:fmt"

@(private="package")
shell_material_asset_controls :: proc(shell:^Shell)->ui.Descriptor {
    path_key:=key(120,"asset-path")
    path:=ui.Descriptor{key=path_key,kind=.Text_Input,placeholder="resources/materials/surface.katmat",state=ui.state(shell.ctx,path_key,0,"resources/materials/surface.katmat"),action=u64(Action.Material_Asset_Path),layout={height=ui.pixels(30),width=ui.percent(1),no_shrink=true}}
    save:=button("Save material",.Material_Save,shell.state.owner.mode!=.Editing);apply:=button("Apply material",.Material_Apply,shell.state.owner.mode!=.Editing)
    width:=max(1,shell_panel_width(shell)-24)
    buttons:=ui.Descriptor{key=key(120,"asset-buttons"),kind=.Row,layout={gap={6,0},width=ui.percent(1),no_shrink=true},children=nodes(shell,{save,apply})}
    if width<240 { buttons.kind=.Column;buttons.layout.gap={0,6};for &child in buttons.children { child.layout.width=ui.percent(1) } }
    hint:=text(120,"Save captures the current surface, image choices and sampling. Apply replaces them together as one Undo step.");hint.layout.height={};hint.layout.no_shrink=true
    return {key=key(120,"asset"),kind=.Column,layout={width=ui.percent(1),gap={0,6},no_shrink=true},children=nodes(shell,{text(120,"Reusable material"),path,buttons,hint})}
}
@(private="package")
shell_material_asset_apply :: proc(shell:^Shell,path:string,capture:bool=false) {
    if error:=shell_finish_gestures(shell);error!=.None { shell.state.last_error=error;return }
    request:=make(json.Object,shell.allocator);defer delete(request)
    request["action"]="capture" if capture else "apply";request["path"]=path
    targets:=make(json.Array,0,shell.allocator);defer { for target in targets { delete(target.(string),shell.allocator) };delete(targets) }
    selected:=""
    if capture { if !shell.state.selection.has_primary { return };selected=fmt.aprintf("%d",u64(shell.state.selection.primary),allocator=shell.allocator);request["entity_id"]=selected }
    else { ids:=material_targets(shell);defer delete(ids);for id in ids { append(&targets,fmt.aprintf("%d",u64(id),allocator=shell.allocator)) };request["entity_ids"]=targets }
    defer delete(selected,shell.allocator)
    bytes,error:=json.marshal(request,allocator=shell.allocator);if error!=nil { return };defer delete(bytes,shell.allocator)
    result:=execute(shell.state,{kind=.Application,tool_name="material_asset",value=bytes})
    message(shell,"Material saved" if result==.None && capture else "Material applied" if result==.None else "Material operation failed; the current surface is preserved")
}
@(private="package")
shell_material_asset_click :: proc(shell:^Shell,event:ui.Click_Action)->bool {
    action:=Action(event.action);if action!=.Material_Save && action!=.Material_Apply { return false }
    path,valid:=ui.state_get(shell.ctx,ui.state(shell.ctx,key(120,"asset-path"),0,"resources/materials/surface.katmat"),string)
    if valid { shell_material_asset_apply(shell,path,action==.Material_Save) };return true
}
