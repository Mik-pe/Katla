//! Material widgets display sRGB factors and group live previews into the canonical shared history.
package editor_app

import app ".."
import agent "../../agent"
import editor "../../editor"
import ecs "../../ecs"
import ui "../../ui"
import "core:encoding/json"
import "core:fmt"

@(private="package")
MATERIAL_PRESET_OPTIONS := [6]string{"Plaster","Oak","Concrete","Ceramic","Brushed metal","Fabric"}

@(private="package")
material_targets :: proc(shell:^Shell)->[dynamic]ecs.Entity_Id {
    result:=make([dynamic]ecs.Entity_Id,shell.allocator)
    for selected in shell.state.selection.entries { if _,present:=ecs.get_component(&shell.state.owner.world,selected.entity,app.Surface_Material); present { append(&result,selected.entity) } }
    return result
}
@(private="package")
shell_material :: proc(shell:^Shell)->ui.Descriptor {
    source,present:=ecs.get_component(&shell.state.owner.world,shell.inspector.entity,app.Surface_Material)
    if !present { return {} }
    values:=app.material_values(source)
    fields:=[7]f32{values.base_color[0],values.base_color[1],values.base_color[2],values.base_color[3],values.metallic,values.roughness,values.ao}
    names:=[7]string{"Red","Green","Blue","Alpha","Metallic","Roughness","Occlusion"}
    children:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(children)
    append(&children,text(50,"Material · sRGB color"))
    swatch:=ui.Descriptor{key=key(50,"swatch"),kind=.Stack,has_background=true,background=values.base_color,layout={height=ui.pixels(26),width=ui.percent(1)}}; append(&children,swatch)
    for name,index in names {
        field_key:=key(50,name,u64(shell.inspector.entity)); state:=ui.state(shell.ctx,field_key,0,fields[index])
        if !shell.material.active && shell.ctx.captured.key!=field_key { ui.state_set(shell.ctx,state,fields[index]) }
        append(&children,ui.Descriptor{key=field_key,kind=.Slider,text=name,state=state,action=u64(Action.Material),payload=u64(index),minimum=0,maximum=1,step=0.005,disabled=shell.state.owner.mode!=.Editing,layout={height=ui.pixels(30),width=ui.percent(1)}})
    }
    preset_key:=key(50,"preset",u64(shell.inspector.entity))
    append(&children,ui.Descriptor{key=preset_key,kind=.Combo,text="Material preset",options=MATERIAL_PRESET_OPTIONS[:],state=ui.state(shell.ctx,preset_key,0,f32(-1)),action=u64(Action.Material_Preset),disabled=shell.state.owner.mode!=.Editing,layout={height=ui.pixels(30),width=ui.percent(1)}})
    return {key=key(50,"section"),kind=.Column,layout={gap={0,6},width=ui.percent(1)},children=nodes(shell,children[:])}
}
@(private="package")
shell_material_change :: proc(shell:^Shell,event:ui.Number_Action) {
    if event.payload>=7 { return }
    owner:=shell.state.owner
    if event.started && !shell.material.active {
        ids:=material_targets(shell); defer delete(ids)
        error:=app.material_gesture_begin(owner,&shell.material,ids[:]); if error!=.None { shell.state.last_error=error; return }
    }
    if !shell.material.active { return }
    source,present:=ecs.get_component(&owner.world,shell.inspector.entity,app.Surface_Material)
    if !present { shell.state.last_error=.Component_Not_Found; return }
    values:=app.material_values(source); fields:bit_set[agent.Material_Field]
    if event.payload<4 { values.base_color[event.payload]=event.value; fields={.Base_Color} }
    else if event.payload==4 { values.metallic=event.value; fields={.Metallic} }
    else if event.payload==5 { values.roughness=event.value; fields={.Roughness} }
    else { values.ao=event.value; fields={.AO} }
    error:=app.material_gesture_preview(owner,&shell.material,fields,values)
    if error==.None && event.finished { error=app.material_gesture_finish(owner,&shell.material) }
    if error!=.None && event.finished && shell.material.active { cancel_error:=app.material_gesture_cancel(owner,&shell.material); if cancel_error!=.None { error=cancel_error } }
    shell.state.last_error=error
}
@(private="package")
shell_material_preset :: proc(shell:^Shell,index:int) {
    if index<0 || index>=6 || shell.material.active { return }
    ids:=material_targets(shell); defer delete(ids)
    names:=make([]string,len(ids),shell.allocator); defer delete(names,shell.allocator)
    for id,i in ids { names[i]=fmt.aprintf("%d",u64(id),allocator=shell.allocator) }; defer { for name in names { delete(name,shell.allocator) } }
    preset:=agent.material_preset_name(agent.Material_Preset(index))
    bytes,error:=json.marshal(struct{action:string,entity_ids:[]string,preset:string}{"set",names,preset},allocator=shell.allocator)
    if error!=nil { shell.state.last_error=.Decode_Failed; return }; defer delete(bytes,shell.allocator)
    execute(shell.state,{kind=.Application,tool_name="material",value=bytes})
}
/// The application calls this before a document, agent or simulation mutation can invalidate a live gesture.
shell_cancel_gesture :: proc(shell:^Shell)->editor.Scene_Error {
    if shell.gizmo.gesture.active { if error:=gizmo_cancel(&shell.gizmo); error!=.None { return error } }
    if shell.field_gesture.active { if error:=app.scene_gesture_cancel(shell.state.owner,&shell.field_gesture); error!=.None { return error }; shell.field_gesture_node=0 }
    if !shell.material.active { return .None }
    return app.material_gesture_cancel(shell.state.owner,&shell.material)
}
