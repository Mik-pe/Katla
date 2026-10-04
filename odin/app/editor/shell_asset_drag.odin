//! Asset drag captures owned rooted paths and admits the whole dropped batch atomically.
package editor_app
import assets "../assets"
import editor "../../editor"
import ui "../../ui"
import "core:encoding/json"
import "core:strings"

@(private="package")
shell_asset_pointer :: proc(shell:^Shell,event:ui.Pointer_Action) {
    if Action(event.action)!=.Asset_Select || shell.browser==nil { return }
    if event.pressed { shell.asset_drag_start=event.position; shell.asset_drag_started=true; shell.asset_drag_active=false; return }
    if !shell.asset_drag_started { return }
    moved:=event.position-shell.asset_drag_start
    if !shell.asset_drag_active && moved[0]*moved[0]+moved[1]*moved[1]>16 {
        if event.payload==0 || event.payload>u64(len(shell.browser.entries)) { return }
        entry:=shell.browser.entries[event.payload-1]; selected:=false
        for path in shell.browser.selected_paths { if path==entry.path { selected=true; break } }
        if !selected { assets.select(shell.browser,entry.path) }
        assets.drag_batch_destroy(&shell.asset_drag); shell.asset_drag=assets.drag_batch(shell.browser); shell.asset_drag_active=true
    }
    if !event.released { return }; shell.asset_drag_started=false
    if !shell.asset_drag_active { return }; shell.asset_drag_active=false
    defer assets.drag_batch_destroy(&shell.asset_drag)
    if shell_material_drop(shell,event.position) { return }
    viewport:=-1
    for slot,index in shell.viewports.slots[:viewport_count(shell.viewports.layout)] { if inside(slot.bounds,event.position) { viewport=index; break } }
    if viewport<0 { return }
    operations:=make([dynamic]editor.Scene_Op,shell.allocator)
    owned:=make([dynamic][]byte,shell.allocator); paths:=make([dynamic]string,shell.allocator)
    defer { delete(operations); for bytes in owned { delete(bytes,shell.allocator) }; delete(owned); for path in paths { delete(path,shell.allocator) }; delete(paths) }
    for item in shell.asset_drag.items {
        if item.kind!=.Model && item.kind!=.Prefab { message(shell,"Drop model and prefab assets into the viewport"); return }
        position:=shell.viewports.slots[viewport].camera.target
        if strings.has_suffix(item.path,".katprefab") {
            path,valid:=assets.project_path(shell.browser,item.path); if !valid { message(shell,"Prefab assets must resolve inside the project"); return }; append(&paths,path)
            data,error:=json.marshal(struct{action,path:string,position:[3]f32}{"instantiate",path,position},allocator=shell.allocator); if error!=nil { return }; append(&owned,data)
            append(&operations,editor.Scene_Op{kind=.Application,tool_name="prefab",value=data})
        } else { append(&operations,editor.Scene_Op{kind=.Spawn_Model,path=item.path,project_asset=item.root==.Project,scale={1,1,1},position=position}) }
    }
    if len(operations)>0 { execute_batch(shell.state,operations[:]) }
}
