//! Native view intents update persistent cameras; authored transforms remain separate shared-history gestures.
package editor_app
import app ".."
import ui "../../ui"
import km "../../math"

@(private="package")
inside :: proc(bounds:ui.Rect,point:ui.Vec2)->bool { return point[0]>=bounds.x && point[1]>=bounds.y && point[0]<bounds.x+bounds.width && point[1]<bounds.y+bounds.height }
/// Focuses the union of actual posed drawable bounds, falling back to a selected entity's world position.
shell_focus :: proc(shell:^Shell) {
    if !shell.viewports.has_active { shell.viewports.active=0; shell.viewports.has_active=true }
    minimum,maximum:km.Vec3; found:=false
    for selected in shell.state.selection.entries {
        bound,present,error:=app.scene_drawable_bounds(shell.state.owner,selected.entity)
        if error!=.None { shell.state.last_error=error; continue }
        if !present {
            world,world_error:=app.scene_world_matrix(shell.state.owner,selected.entity); if world_error!=.None { continue }
            bound={center={world[3][0],world[3][1],world[3][2]},extent={.1,.1,.1}}
        }
        lower,upper:=bound.center-bound.extent,bound.center+bound.extent
        if !found { minimum=lower; maximum=upper; found=true }
        else { for axis in 0..<3 { minimum[axis]=min(minimum[axis],lower[axis]); maximum[axis]=max(maximum[axis],upper[axis]) } }
    }
    if found { viewport:=&shell.viewports.slots[shell.viewports.active]; camera_focus(&viewport.camera,km.aabb_from_min_max(minimum,maximum),viewport.bounds.width/max(1,viewport.bounds.height)) }
}
/// Supplies raw native scroll/modifiers only after the retained UI has routed modal and captured input.
shell_viewport_input :: proc(shell:^Shell,input:ui.Input,result:ui.Frame_Result) {
    if shell.ctx.modal.key!=0 || shell.ctx.popup.key!=0 { return }
    for event in input.events {
        #partial switch value in event {
        case ui.Pointer_Down:
            shell.navigation_modifiers=value.modifiers
        case ui.Scroll:
            if shell.ctx.hovered.key!=key(20,"image",u64(shell.viewports.active)) && shell.ctx.captured.key!=key(20,"image",u64(shell.viewports.active)) { continue }
            for &slot,index in shell.viewports.slots[:viewport_count(shell.viewports.layout)] {
                if inside(slot.bounds,value.position) { shell.viewports.active=index; shell.viewports.has_active=true; camera_navigate(&slot.camera,{},value.delta[1],false,false,slot.bounds.height,shell.preferences.editor.camera_speed if shell.preferences!=nil else 50); break }
            }
        case ui.Window_Focus: if !value.focused { shell.navigation_orbit=false; shell.navigation_pan=false; if shell.gizmo.gesture.active || shell.field_gesture.active || shell.material.active { shell.state.last_error=shell_cancel_gesture(shell) } }
        case ui.Pointer_Move: shell_gizmo_hover(shell,value.position)
        }
    }
}
@(private="package")
shell_viewport_action :: proc(shell:^Shell,event:ui.Pointer_Action) {
    if Action(event.action)!=.Viewport || event.payload>=u64(viewport_count(shell.viewports.layout)) { return }
    index:=int(event.payload); slot:=&shell.viewports.slots[index]
    if shell_gizmo_pointer(shell,event,index) { return }
    if event.pressed {
        shell.viewports.active=index; shell.viewports.has_active=true
        shell.pick_start=event.position; shell.pick_dragged=false
        shell.navigation_pan=event.button==.Middle || event.button==.Right && (.Control in shell.navigation_modifiers || .Super in shell.navigation_modifiers)
        shell.navigation_orbit=event.button==.Right && !shell.navigation_pan || event.button==.Left && .Alt in shell.navigation_modifiers
    }
    if event.delta!={} { movement:=event.position-shell.pick_start; if movement[0]*movement[0]+movement[1]*movement[1]>16 { shell.pick_dragged=true }; camera_navigate(&slot.camera,event.delta,0,shell.navigation_orbit,shell.navigation_pan,slot.bounds.height,shell.preferences.editor.camera_speed if shell.preferences!=nil else 50) }
    if event.released {
        if event.button==.Left && !shell.navigation_orbit && !shell.navigation_pan && !shell.pick_dragged { shell.pick_requested=true; shell.pick_view=index; shell.pick_modifiers=shell.navigation_modifiers; shell.pick_position={(event.position[0]-slot.bounds.x)/max(1,slot.bounds.width),(event.position[1]-slot.bounds.y)/max(1,slot.bounds.height)} }
        shell.navigation_orbit=false; shell.navigation_pan=false
    }
}
/// Focus animation advances independently for every real camera owner.
shell_camera_tick :: proc(shell:^Shell,delta:f32) { for &slot in shell.viewports.slots { camera_tick(&slot.camera,delta) } }
