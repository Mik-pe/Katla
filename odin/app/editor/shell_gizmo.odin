//! Native viewport capture manipulates the same triangles drawn by the accepted camera frame.
package editor_app
import app ".."
import render "../render"
import ui "../../ui"
import km "../../math"

@(private="package")
shell_gizmo_basis :: proc(shell:^Shell)->km.Mat4 {
    if shell.gizmo.gesture.active { return shell.gizmo.basis }
    if shell.gizmo_local && shell.state.selection.has_primary {
        world,error:=app.scene_world_matrix(shell.state.owner,shell.state.selection.primary)
        if error==.None { transform,valid:=km.mat4_decompose_approx(world); if valid { return km.quat_to_mat4(transform.rotation) } }
    }; return km.identity(km.Mat4)
}
@(private="package")
shell_gizmo_click :: proc(shell:^Shell,event:ui.Click_Action)->bool {
    #partial switch Action(event.action) {
    case .Gizmo_Mode: if event.payload<=2 { shell.gizmo_mode=render.Overlay_Mode(event.payload) }
    case .Gizmo_Space: shell.gizmo_local=!shell.gizmo_local
    case .Gizmo_Snap: if shell.preferences!=nil { shell.preferences.editor.snap_to_grid=!shell.preferences.editor.snap_to_grid }
    case: return false
    }; return true
}
@(private="package")
shell_gizmo_key :: proc(shell:^Shell,key:ui.Key) {
    if shell.gizmo.gesture.active { return }
    if key==.W { shell.gizmo_mode=.Translate } else if key==.E { shell.gizmo_mode=.Rotate } else if key==.R { shell.gizmo_mode=.Scale }
}
@(private="package")
shell_gizmo_pointer :: proc(shell:^Shell,event:ui.Pointer_Action,index:int)->bool {
    if shell.state.owner.mode!=.Editing || shell.gizmo_meshes==nil { return false }
    slot:=&shell.viewports.slots[index]; frame:=shell.gizmo_frames[index]
    if event.pressed {
        if event.button!=.Left || shell.navigation_orbit || shell.navigation_pan { return false }
        hit:=gizmo_hit(&shell.gizmo_meshes[index],frame.view_projection,slot.bounds,event.position)
        if !hit.hit || hit.handle==.None { return false }
        if error:=shell_finish_gestures(shell); error!=.None { shell.state.last_error=error; return true }
        origin,direction,valid:=gizmo_ray(frame.view_projection,slot.bounds,event.position); if !valid { return true }
        ids:=selected_ids(shell); defer delete(ids,shell.allocator)
        pivot:km.Vec3
        if shell.state.selection.has_primary { world,error:=app.scene_world_matrix(shell.state.owner,shell.state.selection.primary); if error!=.None { shell.state.last_error=error; return true }; pivot=km.xyz(world[3]) }
        snap:Gizmo_Snap
        if shell.preferences!=nil && shell.preferences.editor.snap_to_grid { snap={translation=shell.preferences.editor.grid_size,rotation=15,scale=.1} }
        shell.state.last_error=gizmo_begin(&shell.gizmo,shell.state.owner,ids,hit,origin,direction,pivot,shell_gizmo_basis(shell),shell.gizmo_mode,snap)
        return true
    }
    if !shell.gizmo.gesture.active { return false }
    if event.delta!={} || event.released {
        origin,direction,valid:=gizmo_ray(frame.view_projection,slot.bounds,event.position)
        if valid { shell.state.last_error=gizmo_move(&shell.gizmo,origin,direction) }
    }
    if event.released { shell.state.last_error=gizmo_finish(&shell.gizmo); if shell.state.last_error==.None { shell.state.revision+=1 } }
    return true
}
@(private="package")
shell_gizmo_hover :: proc(shell:^Shell,position:ui.Vec2) {
    if shell.gizmo.gesture.active || shell.gizmo_meshes==nil { return }
    shell.gizmo_hover=.None
    for slot,index in shell.viewports.slots[:viewport_count(shell.viewports.layout)] { if inside(slot.bounds,position) { hit:=gizmo_hit(&shell.gizmo_meshes[index],shell.gizmo_frames[index].view_projection,slot.bounds,position); if hit.hit { shell.gizmo_hover=hit.handle }; break } }
}
