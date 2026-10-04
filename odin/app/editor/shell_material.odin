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
MATERIAL_ALPHA_OPTIONS := [3]string{"Opaque","Mask","Blend"}
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
    if shell.material_info_ready { values=shell.material_info.values }
    fields:=[13]f32{values.base_color[0],values.base_color[1],values.base_color[2],values.base_color[3],values.metallic,values.roughness,values.ao,values.emissive_factor[0],values.emissive_factor[1],values.emissive_factor[2],values.normal_scale,values.occlusion_strength,values.alpha_cutoff}
    names:=[13]string{"Red","Green","Blue","Alpha","Metallic","Roughness","Occlusion","Emission R","Emission G","Emission B","Normal scale","AO strength","Alpha cutoff"}
    children:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(children)
    expand_key:=key(50,"expanded"); expanded_state:=ui.state(shell.ctx,expand_key,0,true)
    expanded,_:=ui.state_get(shell.ctx,expanded_state,bool)
    header:=ui.Descriptor{key=expand_key,kind=.Section,text="Surface material",state=expanded_state,expanded=expanded,action=u64(Action.Material_Expand),layout={height=ui.pixels(30),width=ui.percent(1)}}
    if !expanded { return header }
    width:=max(1,shell_panel_width(shell)-24)
    narrow:=width<196
    append(&children,text(50,"Base color · sRGB"))
    swatch:=ui.Descriptor{key=key(50,"swatch"),kind=.Stack,has_background=true,background=values.base_color,layout={height=ui.pixels(26),width=ui.percent(1)}}; append(&children,swatch)
    for name,index in names {
        field_key:=key(50,name,u64(shell.inspector.entity)); state:=ui.state(shell.ctx,field_key,0,fields[index])
        if !shell.material.active && shell.ctx.captured.key!=field_key { ui.state_set(shell.ctx,state,fields[index]) }
        low:f32=0;high:f32=1
        typed:=index>=7 && index<=10
        if index>=7 && index<=9 { high=max(f32) }
        if index==10 { low= -max(f32);high=max(f32) }
        if index==12 { high=max(1,fields[index]) }
        if narrow || typed { label:=text(50,name);label.key=key(key(50,"label"),name,u64(shell.inspector.entity));append(&children,label) }
        append(&children,ui.Descriptor{key=field_key,kind=.Numeric_Input if typed else .Slider,text="" if narrow || typed else name,state=state,action=u64(Action.Material),payload=u64(index),minimum=low,maximum=high,step=0 if typed else .005,disabled=shell.state.owner.mode!=.Editing,layout={height=ui.pixels(30),width=ui.percent(1),no_shrink=true}})
    }
    preset_key:=key(50,"preset",u64(shell.inspector.entity))
    append(&children,ui.Descriptor{key=preset_key,kind=.Combo,text="Material preset",options=MATERIAL_PRESET_OPTIONS[:],state=ui.state(shell.ctx,preset_key,0,f32(-1)),action=u64(Action.Material_Preset),disabled=shell.state.owner.mode!=.Editing,layout={height=ui.pixels(30),width=ui.percent(1)}})
    alpha_key:=key(50,"alpha",u64(shell.inspector.entity));alpha_state:=ui.state(shell.ctx,alpha_key,0,f32(values.alpha_mode));if shell.ctx.popup.key!=alpha_key { ui.state_set(shell.ctx,alpha_state,f32(values.alpha_mode)) }
    append(&children,ui.Descriptor{key=alpha_key,kind=.Combo,text="Alpha mode",state=alpha_state,options=MATERIAL_ALPHA_OPTIONS[:],action=u64(Action.Material_Alpha),disabled=shell.state.owner.mode!=.Editing,layout={height=ui.pixels(30),width=ui.percent(1)}})
    double_key:=key(50,"double",u64(shell.inspector.entity));double_state:=ui.state(shell.ctx,double_key,0,values.double_sided);ui.state_set(shell.ctx,double_state,values.double_sided)
    append(&children,ui.Descriptor{key=double_key,kind=.Checkbox,text="Double sided",state=double_state,action=u64(Action.Material_Double),disabled=shell.state.owner.mode!=.Editing,layout={height=ui.pixels(30),width=ui.percent(1)}})
    detail:=text(50,"Emission uses linear RGB. Values above 1 contribute HDR light. Presets preserve image bindings.");detail.layout.height={};detail.layout.no_shrink=true;append(&children,detail)
    content:=ui.Descriptor{key=key(50,"content"),kind=.Column,layout={gap={0,6},width=ui.percent(1),no_shrink=true},children=nodes(shell,children[:])}
    return {key=key(50,"section"),kind=.Column,layout={gap={0,6},width=ui.percent(1),no_shrink=true},children=nodes(shell,{header,content})}
}
@(private="package")
shell_material_change :: proc(shell:^Shell,event:ui.Number_Action) {
    if event.payload>=13 { return }
    owner:=shell.state.owner
    if event.started && !shell.material.active {
        ids:=material_targets(shell); defer delete(ids)
        error:=app.material_gesture_begin(owner,&shell.material,ids[:]); if error!=.None { shell.state.last_error=error; return }
    }
    if !shell.material.active { return }
    values,read_error:=app.material_entity_values(owner,shell.inspector.entity)
    if read_error!=.None { shell.state.last_error=read_error;return }
    fields:bit_set[agent.Material_Field]
    if event.payload<4 { values.base_color[event.payload]=event.value; fields={.Base_Color} }
    else if event.payload==4 { values.metallic=event.value; fields={.Metallic} }
    else if event.payload==5 { values.roughness=event.value; fields={.Roughness} }
    else if event.payload==6 { values.ao=event.value; fields={.AO} }
    else if event.payload<=9 { values.emissive_factor[event.payload-7]=event.value;fields={.Emissive_Factor} }
    else if event.payload==10 { values.normal_scale=event.value;fields={.Normal_Scale} }
    else if event.payload==11 { values.occlusion_strength=event.value;fields={.Occlusion_Strength} }
    else { values.alpha_cutoff=event.value;fields={.Alpha_Cutoff} }
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
@(private="package")
shell_material_property :: proc(shell:^Shell,field:string,value:json.Value) {
    if error:=shell_finish_gestures(shell); error!=.None { shell.state.last_error=error; return }
    ids:=material_targets(shell); defer delete(ids)
    names:=make(json.Array,len(ids),shell.allocator)
    defer { for name in names { delete(name.(string),shell.allocator) }; delete(names) }
    for id,i in ids { names[i]=fmt.aprintf("%d",u64(id),allocator=shell.allocator) }
    request:=make(json.Object,shell.allocator); defer delete(request)
    request["action"]="set"; request["entity_ids"]=names; request[field]=value
    bytes,error:=json.marshal(request,allocator=shell.allocator)
    if error!=nil { shell.state.last_error=.Decode_Failed; return }; defer delete(bytes,shell.allocator)
    execute(shell.state,{kind=.Application,tool_name="material",value=bytes})
}
/// The application calls this before a document, agent or simulation mutation can invalidate a live gesture.
shell_cancel_gesture :: proc(shell:^Shell)->editor.Scene_Error {
    if shell.gizmo.gesture.active { if error:=gizmo_cancel(&shell.gizmo); error!=.None { return error } }
    if shell.field_gesture.active { if error:=app.scene_gesture_cancel(shell.state.owner,&shell.field_gesture); error!=.None { return error }; shell.field_gesture_node=0 }
    if shell.sampling_gesture.scene.active { if error:=app.material_sampling_gesture_cancel(shell.state.owner,&shell.sampling_gesture); error!=.None { return error };shell.sampling_node=0 }
    if !shell.material.active { return .None }
    return app.material_gesture_cancel(shell.state.owner,&shell.material)
}
