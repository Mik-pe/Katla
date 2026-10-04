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
    has_tint:bool `inspect:"skip"`,has_factors:bool `inspect:"skip"`,
    metallic,roughness,ao:f32 `inspect:"skip"`,
    surface:Material_Surface `inspect:"skip"`,has_surface:bool `inspect:"skip"`,
    sampling:Material_Sampling `inspect:"skip"`,has_sampling:bool `inspect:"skip"`,
}
/// Returns authoring values without exposing internal linear color or native resources.
material_values :: proc(surface:Surface_Material)->agent.Material_Values {
    color:=km.Color{1,1,1,1}
    if surface.has_tint { color=km.color_to_srgb(surface.linear_color) }
    properties:=material_surface_effective(surface,material_surface_default())
    return {base_color={color.r,color.g,color.b,color.a},metallic=surface.metallic,roughness=surface.roughness,ao=surface.ao,emissive_factor=properties.emissive_factor,normal_scale=properties.normal_scale,occlusion_strength=properties.occlusion_strength,alpha_mode=agent.Material_Alpha_Mode(properties.alpha_mode),alpha_cutoff=properties.alpha_cutoff,double_sided=properties.double_sided}
}
@(private="package")
Material_Edit :: struct { entity:ecs.Entity_Id, before,after:Surface_Material }
@(private="package")
Material_Command :: struct { edits:[]Material_Edit }
@(private="package")
Material_Output :: struct { entity_id:string, values:Material_Values_Output }
@(private="package")
Preset_Output :: struct { preset,label:string, values:Material_Values_Output }
@(private="package")
Material_Values_Output :: struct { base_color:[4]f32,metallic,roughness,ao:f32,emissive_factor:[3]f32,normal_scale,occlusion_strength:f32,alpha_mode:string,alpha_cutoff:f32,double_sided:bool }
@(private="package")
material_values_output :: proc(value:agent.Material_Values)->Material_Values_Output { return {value.base_color,value.metallic,value.roughness,value.ao,value.emissive_factor,value.normal_scale,value.occlusion_strength,agent.material_alpha_mode_name(value.alpha_mode),value.alpha_cutoff,value.double_sided} }
@(private="package")
target_surface :: proc(w:^ecs.World,id:ecs.Entity_Id)->(Surface_Material,editor.Scene_Error) {
    if !ecs.entity_exists(w,id) { return {},.Entity_Not_Found }
    _,protected:=ecs.get_component(w,id,Editor_Hidden); if protected { return {},.Protected_Entity }
    surface,present:=ecs.get_component(w,id,Surface_Material); if !present { return {},.Component_Not_Found }
    return surface,.None
}
@(private="package")
material_command_apply :: proc(state:rawptr,w:^ecs.World,reg:^editor.Component_Registry,redo:bool,remaps:^[dynamic]editor.Entity_Remap)->editor.Scene_Error {
    command:=cast(^Material_Command)state
    for edit in command.edits { _,err:=target_surface(w,edit.entity); if err!=.None { return err } }
    entry:=reg.entries["SurfaceMaterial"]; if entry==nil { return .Component_Not_Found }
    proposal:=Scene_Action_Command{rows=make([dynamic]Scene_Action_Row,w.allocator),allocator=w.allocator}
    defer { for row in proposal.rows { editor.entity_components_destroy(row.before,w.allocator); editor.entity_components_destroy(row.after,w.allocator) }; delete(proposal.rows) }
    for edit in command.edits {
        before:=new(Surface_Material,w.allocator); before^=edit.before
        after:=new(Surface_Material,w.allocator); after^=edit.after
        row:=Scene_Action_Row{entity=edit.entity,before_exists=true,after_exists=true,before=make([dynamic]editor.Component_Snapshot,w.allocator),after=make([dynamic]editor.Component_Snapshot,w.allocator)}
        append(&row.before,editor.Component_Snapshot{entry.name,entry,before}); append(&row.after,editor.Component_Snapshot{entry.name,entry,after}); append(&proposal.rows,row)
    }
    return scene_action_command_apply(&proposal,w,reg,redo,remaps)
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
    case agent.Material_Set_Sampling: return material_sampling_execute(app,op)
    case agent.Material_Set_Texture: return material_texture_execute(app,op)
    case agent.Material_Presets:
        presets:[6]Preset_Output
        labels:=[6]string{"Plaster","Oak","Concrete","Ceramic","Brushed metal","Fabric"}
        for preset in agent.Material_Preset { presets[int(preset)]={agent.material_preset_name(preset),labels[int(preset)],material_values_output(agent.material_preset_values(preset))} }
        result.data,_=json.marshal(struct { presets:[]Preset_Output, color_space,base_color_space,emissive_color_space,scope:string,capabilities:Material_Capabilities }{presets[:],"srgb","srgb","linear","Per-object factors; textures and mesh geometry are preserved.",material_capabilities()},allocator=w.allocator)
        return result,{}
    case agent.Material_Inspect:
        _,err:=target_surface(w,op.entity); if err!=.None { result.error=err; return result,{} }
        id:=fmt.aprintf("%d",u64(op.entity)); defer delete(id,w.allocator)
        values,values_error:=material_entity_values(app,op.entity); if values_error!=.None { result.error=values_error; return result,{} }
        if !agent.material_values_valid(values) { result.error=.Invalid_Operation; return result,{} }
        sampling,sampling_error:=material_effective_sampling(app,op.entity); if sampling_error!=.None { result.error=sampling_error; return result,{} }
        encoded_sampling:=material_sampling_inspect(sampling)
        provenance:=material_provenance(app,op.entity); defer json.destroy_value(provenance)
        uv_sets:=[2]bool{material_target_uv(app,op.entity,.Albedo,0),material_target_uv(app,op.entity,.Albedo,1)}
        result.data,_=json.marshal(struct { entity_id:string, values:Material_Values_Output, provenance:json.Value,sampling:struct{albedo,normal,metallic_roughness,occlusion,emission:Material_Role_Inspect},uv_sets:[2]bool,color_space,base_color_space,emissive_color_space,rotation_unit:string,capabilities:Material_Capabilities }{id,material_values_output(values),provenance,encoded_sampling,uv_sets,"srgb","srgb","linear","radians",material_capabilities()},allocator=w.allocator)
        return result,{}
    case agent.Material_Set:
        if app.mode!=.Editing { result.error=.Editing_Required; return result,{} }
        if len(op.entities)<1 || len(op.entities)>256 || (!op.has_preset && op.fields=={}) { result.error=.Invalid_Operation; return result,{} }
        allowed_fields:=bit_set[agent.Material_Field]{.Base_Color,.Metallic,.Roughness,.AO,.Emissive_Factor,.Normal_Scale,.Occlusion_Strength,.Alpha_Mode,.Alpha_Cutoff,.Double_Sided}
        if op.fields & ~allowed_fields!={} || (op.has_preset && (int(op.preset)<0 || int(op.preset)>int(agent.Material_Preset.Fabric))) { result.error=.Invalid_Operation; return result,{} }
        command:=new(Material_Command,w.allocator); command.edits=make([]Material_Edit,len(op.entities),w.allocator)
        owned:=false; defer { if !owned { material_command_destroy(command,w.allocator) } }
        outputs:=make([]Material_Output,len(op.entities),w.allocator)
        defer { for output in outputs { delete(output.entity_id,w.allocator) }; delete(outputs,w.allocator) }
        for id,i in op.entities {
            for previous in op.entities[:i] { if previous==id { result.error=.Invalid_Operation; return result,{} } }
            before,err:=target_surface(w,id); if err!=.None { result.error=err; return result,{} }
            values,values_error:=material_entity_values(app,id); if values_error!=.None { result.error=values_error; return result,{} }
            if op.has_preset { values=agent.material_preset_values(op.preset) }
            if .Base_Color in op.fields { values.base_color=op.values.base_color }
            if .Metallic in op.fields { values.metallic=op.values.metallic }
            if .Roughness in op.fields { values.roughness=op.values.roughness }
            if .AO in op.fields { values.ao=op.values.ao }
            if .Emissive_Factor in op.fields { values.emissive_factor=op.values.emissive_factor }
            if .Normal_Scale in op.fields { values.normal_scale=op.values.normal_scale }
            if .Occlusion_Strength in op.fields { values.occlusion_strength=op.values.occlusion_strength }
            if .Alpha_Mode in op.fields { values.alpha_mode=op.values.alpha_mode }
            if .Alpha_Cutoff in op.fields { values.alpha_cutoff=op.values.alpha_cutoff }
            if .Double_Sided in op.fields { values.double_sided=op.values.double_sided }
            if !agent.material_values_valid(values) { result.error=.Invalid_Operation; return result,{} }
            after:=before
            if op.has_preset || op.fields & (bit_set[agent.Material_Field]{.Base_Color,.Metallic,.Roughness,.AO})!={} {
                after.metallic=values.metallic; after.roughness=values.roughness; after.ao=values.ao
                if _,imported:=ecs.get_component(w,id,Scene_Model); imported {
                    after.has_factors=true
                    if !before.has_factors { c:=values.base_color; after.linear_color=km.color_to_linear({c[0],c[1],c[2],c[3]}); after.has_tint=true }
                }
            }
            if op.has_preset || op.fields & (bit_set[agent.Material_Field]{.Emissive_Factor,.Normal_Scale,.Occlusion_Strength,.Alpha_Mode,.Alpha_Cutoff,.Double_Sided})!={} {
                after.surface={values.emissive_factor,values.normal_scale,values.occlusion_strength,Material_Alpha_Mode(values.alpha_mode),values.alpha_cutoff,values.double_sided}; after.has_surface=true
            }
            if op.has_preset || .Base_Color in op.fields {
                c:=values.base_color; after.linear_color=km.color_to_linear({c[0],c[1],c[2],c[3]}); after.has_tint=true
            }
            command.edits[i]={id,before,after}
            outputs[i]={fmt.aprintf("%d",u64(id)),material_values_output(values)}
        }
        marshal_error:json.Marshal_Error
        result.data,marshal_error=json.marshal(struct { materials:[]Material_Output, color_space,base_color_space,emissive_color_space:string,capabilities:Material_Capabilities }{outputs,"srgb","srgb","linear",material_capabilities()},allocator=w.allocator)
        if marshal_error!=nil { result.error=.Decode_Failed; return result,{} }
        group:=editor.undo_group_create(command,{material_command_apply,material_command_destroy,material_command_remap},op.entities,w.allocator); owned=true
        err:=material_command_apply(command,w,&app.registry,true,&group.remaps)
        if err!=.None { editor.undo_group_destroy(&group); delete(result.data,w.allocator); result.data=nil; result.error=err; return result,{} }
        append(&result.entities,..op.entities)
        return result,group
    }
    result.error=.Invalid_Operation; return result,{}
}
