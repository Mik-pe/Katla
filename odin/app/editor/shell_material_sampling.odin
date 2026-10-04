//! Continuous sampling fields retain one first-before and last-after material command.
package editor_app

import app ".."
import agent "../../agent"
import ui "../../ui"

@(private="package")
shell_material_sampling_number :: proc(shell:^Shell,event:ui.Number_Action) {
    if event.payload>=6 || !shell.inspector.has_entity { return }
    owner:=shell.state.owner
    role:=agent.Material_Texture_Role(shell_material_role(shell))
    if event.started {
        if error:=shell_finish_gestures(shell);error!=.None { shell.state.last_error=error;return }
        ids:=material_targets(shell);defer delete(ids)
        shell.state.last_error=app.material_sampling_gesture_begin(owner,&shell.sampling_gesture,ids[:],role)
        if shell.state.last_error!=.None { return }
        shell.sampling_node=event.node.key
    }
    if !shell.sampling_gesture.scene.active || shell.sampling_node!=event.node.key { return }
    info,error:=app.material_inspector_read(owner,shell.inspector.entity)
    if error!=.None { shell.state.last_error=error;return };defer app.material_inspector_destroy(&info,shell.allocator)
    current:=app.material_sampling_roles(info.sampling)[int(role)]
    patch:agent.Material_Sampling_Patch
    if event.payload<2 { patch.fields={.Offset};patch.offset=current.uv.offset;patch.offset[event.payload]=event.value }
    else if event.payload==2 { patch.fields={.Rotation};patch.rotation=event.value }
    else if event.payload<5 { patch.fields={.Scale};patch.scale=current.uv.scale;patch.scale[event.payload-3]=event.value }
    else {
        patch.fields={.Anisotropy};patch.anisotropy=u8(clamp(event.value,1,16))
        if patch.anisotropy>1 {
            patch.fields|={.Minification,.Magnification};patch.magnification=.Linear
            patch.minification=.Linear
            if current.sampler.mip_filter==.Nearest { patch.minification=.Linear_Mipmap_Nearest }
            else if current.sampler.mip_filter==.Linear { patch.minification=.Linear_Mipmap_Linear }
        }
    }
    shell.state.last_error=app.material_sampling_gesture_preview(owner,&shell.sampling_gesture,patch)
    if shell.state.last_error==.None && event.finished {
        shell.state.last_error=app.material_sampling_gesture_finish(owner,&shell.sampling_gesture)
        if shell.state.last_error==.None { shell.sampling_node=0 }
    }
}
