//! Validated per-object factors use the editor's single owned command history.
package app

import ecs "../ecs"
import editor "../editor"
import agent "../agent"
import km "../math"
import "core:encoding/json"
import "core:mem"
import "core:fmt"

/// Numeric surface factors are independent of mesh, texture and GPU resource handles.
Surface_Material :: struct {
    linear_color:km.Color `inspect:"skip"`,
    has_tint:bool `inspect:"skip"`,
    metallic,roughness,ao:f32 `inspect:"skip"`,
}
/// Returns authoring values without exposing internal linear color or native resources.
material_values :: proc(surface:Surface_Material)->agent.Material_Values {
    color:=km.Color{1,1,1,1}
    if surface.has_tint { color=km.color_to_srgb(surface.linear_color) }
    return {{color.r,color.g,color.b,color.a},surface.metallic,surface.roughness,surface.ao}
}
@(private="package")
Material_Edit :: struct { entity:ecs.Entity_Id, before,after:Surface_Material }
@(private="package")
Material_Command :: struct { edits:[]Material_Edit }
@(private="package")
Material_Output :: struct { entity_id:string, values:agent.Material_Values }
@(private="package")
Preset_Output :: struct { preset,label:string, values:agent.Material_Values }
@(private="package")
target_surface :: proc(w:^ecs.World,id:ecs.Entity_Id)->(Surface_Material,editor.Scene_Error) {
    if !ecs.entity_exists(w,id) { return {},.Entity_Not_Found }
    _,protected:=ecs.get_component(w,id,Editor_Hidden); if protected { return {},.Protected_Entity }
    surface,present:=ecs.get_component(w,id,Surface_Material); if !present { return {},.Component_Not_Found }
    return surface,.None
}
@(private="package")
material_command_apply :: proc(state:rawptr,w:^ecs.World,_:^editor.Component_Registry,redo:bool,_:^[dynamic]editor.Entity_Remap)->editor.Scene_Error {
    command:=cast(^Material_Command)state
    for edit in command.edits { _,err:=target_surface(w,edit.entity); if err!=.None { return err } }
    for edit in command.edits {
        surface:=ecs.get_component_mut(w,edit.entity,Surface_Material)
        surface^=edit.after if redo else edit.before
    }
    return .None
}
@(private="package")
material_command_destroy :: proc(state:rawptr,allocator:mem.Allocator) {
    command:=cast(^Material_Command)state; delete(command.edits,allocator); free(command,allocator)
}
@(private="package")
material_command_remap :: proc(state:rawptr,remap:editor.Entity_Remap) {
    command:=cast(^Material_Command)state
    for &edit in command.edits { if edit.entity==remap.before { edit.entity=remap.after } }
}
/// Preflights every target and factor, serializes the response, then commits one undoable batch.
material_execute :: proc(app:^Authoring,operation:agent.Material_Op)->(editor.Tool_Result,editor.Undo_Group) {
    w:=&app.world; context.allocator=w.allocator
    result:=error_result(w,.None)
    switch op in operation {
    case agent.Material_Presets:
        presets:[6]Preset_Output
        labels:=[6]string{"Plaster","Oak","Concrete","Ceramic","Brushed metal","Fabric"}
        for preset in agent.Material_Preset { presets[int(preset)]={agent.material_preset_name(preset),labels[int(preset)],agent.material_preset_values(preset)} }
        result.data,_=json.marshal(struct { presets:[]Preset_Output, color_space,scope:string }{presets[:],"srgb","Per-object multipliers; textures and mesh geometry are preserved."},allocator=w.allocator)
        return result,{}
    case agent.Material_Inspect:
        surface,err:=target_surface(w,op.entity); if err!=.None { result.error=err; return result,{} }
        id:=fmt.aprintf("%d",u64(op.entity)); defer delete(id,w.allocator)
        values:=material_values(surface)
        if !agent.material_values_valid(values) { result.error=.Invalid_Operation; return result,{} }
        result.data,_=json.marshal(struct { entity_id:string, values:agent.Material_Values, color_space:string }{id,values,"srgb"},allocator=w.allocator)
        return result,{}
    case agent.Material_Set:
        if app.mode!=.Editing { result.error=.Editing_Required; return result,{} }
        if len(op.entities)<1 || len(op.entities)>256 || (!op.has_preset && op.fields=={}) { result.error=.Invalid_Operation; return result,{} }
        allowed_fields:=bit_set[agent.Material_Field]{.Base_Color,.Metallic,.Roughness,.AO}
        if op.fields & ~allowed_fields!={} || (op.has_preset && (int(op.preset)<0 || int(op.preset)>int(agent.Material_Preset.Fabric))) { result.error=.Invalid_Operation; return result,{} }
        command:=new(Material_Command,w.allocator); command.edits=make([]Material_Edit,len(op.entities),w.allocator)
        owned:=false; defer { if !owned { material_command_destroy(command,w.allocator) } }
        outputs:=make([]Material_Output,len(op.entities),w.allocator)
        defer { for output in outputs { delete(output.entity_id,w.allocator) }; delete(outputs,w.allocator) }
        for id,i in op.entities {
            for previous in op.entities[:i] { if previous==id { result.error=.Invalid_Operation; return result,{} } }
            before,err:=target_surface(w,id); if err!=.None { result.error=err; return result,{} }
            values:=material_values(before)
            if op.has_preset { values=agent.material_preset_values(op.preset) }
            if .Base_Color in op.fields { values.base_color=op.values.base_color }
            if .Metallic in op.fields { values.metallic=op.values.metallic }
            if .Roughness in op.fields { values.roughness=op.values.roughness }
            if .AO in op.fields { values.ao=op.values.ao }
            if !agent.material_values_valid(values) { result.error=.Invalid_Operation; return result,{} }
            after:=before
            after.metallic=values.metallic; after.roughness=values.roughness; after.ao=values.ao
            if op.has_preset || .Base_Color in op.fields {
                c:=values.base_color; after.linear_color=km.color_to_linear({c[0],c[1],c[2],c[3]}); after.has_tint=true
            }
            command.edits[i]={id,before,after}
            outputs[i]={fmt.aprintf("%d",u64(id)),material_values(after)}
        }
        marshal_error:json.Marshal_Error
        result.data,marshal_error=json.marshal(struct { materials:[]Material_Output, color_space:string }{outputs,"srgb"},allocator=w.allocator)
        if marshal_error!=nil { result.error=.Decode_Failed; return result,{} }
        group:=editor.undo_group_create(command,{material_command_apply,material_command_destroy,material_command_remap},op.entities,w.allocator); owned=true
        err:=material_command_apply(command,w,&app.registry,true,&group.remaps)
        if err!=.None { editor.undo_group_destroy(&group); delete(result.data,w.allocator); result.data=nil; result.error=err; return result,{} }
        append(&result.entities,..op.entities)
        return result,group
    }
    result.error=.Invalid_Operation; return result,{}
}
