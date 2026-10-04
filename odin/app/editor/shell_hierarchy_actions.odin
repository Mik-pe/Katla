//! Hierarchy mutations use one canonical atomic scene action across the complete selection.
package editor_app
import ecs "../../ecs"
import editor "../../editor"
import ui "../../ui"

@(private="package")
selected_ids :: proc(shell:^Shell)->[]ecs.Entity_Id { ids:=make([]ecs.Entity_Id,len(shell.state.selection.entries),shell.allocator); for selected,i in shell.state.selection.entries { ids[i]=selected.entity }; return ids }
@(private="package")
shell_selection_command :: proc(shell:^Shell,kind:editor.Scene_Op_Kind) {
    if !shell.state.selection.has_primary { return }
    if error:=shell_finish_gestures(shell); error!=.None { shell.state.last_error=error; return }
    ids:=selected_ids(shell); defer delete(ids,shell.allocator)
    execute(shell.state,{kind=kind,entity=shell.state.selection.primary},ids)
}
@(private="package")
shell_create :: proc(shell:^Shell,index:int) {
    shapes:=[6]string{"cube","sphere","plane","cylinder","cone","torus"}; if index<0 || index>=len(shapes) { return }
    position:=[3]f32{}; if shell.viewports.has_active { position=shell.viewports.slots[shell.viewports.active].camera.target }
    execute_batch(shell.state,{{kind=.Spawn,shape=shapes[index],scale={1,1,1},position=position}})
}
@(private="package")
shell_hierarchy_pointer :: proc(shell:^Shell,event:ui.Pointer_Action) {
    if Action(event.action)!=.Select { return }
    if event.pressed { shell.hierarchy_drag_start=event.position; shell.hierarchy_drag_entity=event.payload; shell.hierarchy_drag_started=true; shell.hierarchy_drag_active=false; return }
    if !shell.hierarchy_drag_started { return }
    moved:=event.position-shell.hierarchy_drag_start
    if !shell.hierarchy_drag_active && moved[0]*moved[0]+moved[1]*moved[1]>16 {
        if !selection_contains(shell.state,ecs.Entity_Id(shell.hierarchy_drag_entity)) { selection_set(shell.state,ecs.Entity_Id(shell.hierarchy_drag_entity)) }
        shell.hierarchy_drag_active=true
    }
    if !event.released { return }
    shell.hierarchy_drag_started=false
    if !shell.hierarchy_drag_active { return }; shell.hierarchy_drag_active=false
    target:ecs.Entity_Id; has_target:=false; inside_panel:=false
    bounds,exists:=ui.bounds(shell.ctx,key(10,"panel")); if exists { inside_panel=inside(bounds,event.position) }
    if !inside_panel { return }
    for row in shell.state.rows { rect,valid:=ui.bounds(shell.ctx,key(10,"entity",u64(row.entity))); if valid && inside(rect,event.position) { target=row.entity; has_target=true; break } }
    ids:=selected_ids(shell); defer delete(ids,shell.allocator)
    if len(ids)>0 { execute(shell.state,{kind=.Set_Parent,entity=ids[0],parent=target,has_parent=has_target},ids) }
}
