//! Reusable surfaces publish confined files and independent prepared live copies.
package app

import asset "../agent/assets"
import agent "../agent"
import ecs "../ecs"
import editor "../editor"
import resources "../resources"
import ron "../encoding/ron"
import km "../math"
import "core:encoding/json"
import "core:strings"
import "core:fmt"

/// Captures effective imported sources explicitly so reusable copies never depend on target meshes.
material_asset_capture :: proc(owner:^Authoring,id:ecs.Entity_Id)->(Material_Asset,editor.Scene_Error) {
    surface,error:=target_surface(&owner.world,id); if error!=.None { return {},error }
    result:=Material_Asset{version=1,name=strings.clone("Material",owner.world.allocator),material=surface}
    accepted:=false; defer { if !accepted { material_asset_destroy(&result,owner.world.allocator) } }
    if name,present:=ecs.get_component(&owner.world,id,Scene_Name); present && len(strings.trim_space(name.name))>0 { delete(result.name,owner.world.allocator); result.name=strings.clone(name.name,owner.world.allocator) }
    model,imported,import_error:=material_imported_material(owner,id); if import_error!=.None { return {},import_error }
    values,values_error:=material_entity_values(owner,id); if values_error!=.None { return {},values_error }
    result.material.has_factors=true; result.material.has_tint=true; c:=values.base_color; result.material.linear_color=km.color_to_linear({c[0],c[1],c[2],c[3]})
    result.material.metallic=values.metallic; result.material.roughness=values.roughness; result.material.ao=values.ao
    result.material.surface={values.emissive_factor,values.normal_scale,values.occlusion_strength,Material_Alpha_Mode(values.alpha_mode),values.alpha_cutoff,values.double_sided}; result.material.has_surface=true
    policies,policy_error:=material_effective_sampling(owner,id); if policy_error!=.None { return {},policy_error }; result.material.sampling=policies; result.material.has_sampling=true
    choices,present:=ecs.get_component(&owner.world,id,Texture_Assignments); if present { result.textures=texture_assignments_clone(choices,owner.world.allocator) }
    views:=material_imported_views(imported)
    for source,index in texture_assignments_roles(&result.textures) {
        if source.kind!=.Inherit { continue }; source.kind=.Neutral
        if model==nil || views[index].texture<0 { continue }
        texture:=views[index].texture; if int(texture)>=len(model.model.textures) { return {},.Invalid_Operation }
        image_index:=model.model.textures[texture].image; if image_index<0 || int(image_index)>=len(model.model.images) { return {},.Invalid_Operation }
        source^={kind=.GltfImage,root=model.source.root,path=strings.clone(model.source.path,owner.world.allocator),image_index=u32(image_index)}
    }
    accepted=true; return result,.None
}
/// Executes all six portable material actions through the application's atomic file/history paths.
material_asset_execute :: proc(owner:^Authoring,request:asset.Material_Asset_Request)->(editor.Tool_Result,editor.Undo_Group) {
    allocator:=owner.world.allocator; context.allocator=allocator
    result:=error_result(&owner.world,.None)
    if (request.action==.Capture || request.action==.Apply) && owner.mode!=.Editing { result.error=.Editing_Required; return result,{} }
    if request.action==.Describe {
        example:=Material_Asset{version=1,name="Plaster",material={has_factors=true,has_tint=true,linear_color=km.color_to_linear({.88,.85,.79,1}),roughness=.9,ao=1,surface=material_surface_default(),has_surface=true,sampling=material_sampling_default(),has_sampling=true}}
        for source in texture_assignments_roles(&example.textures) { source.kind=.Neutral }
        document,valid:=material_asset_document_encode(owner,example,""); if !valid { result.error=.Decode_Failed; return result,{} }; defer json.destroy_value(document)
        result.data,_=json.marshal(struct {example:json.Value,format,paths,workflow,units:string,limits:struct{targets,file_bytes:u32}}{document,".katmat RON, version 1","Project-relative tool paths; Resource, Scene and intentional File image origins","Capture or read, validate/write, apply independent copies, inspect and undo/redo","base_color sRGB; emissive_factor linear HDR; rotation radians; normal_scale finite; occlusion_strength 0..1",{256,64*1024*1024}},allocator=allocator); return result,{}
    }
    if !resources.valid_relative_path(request.path) || !strings.has_suffix(request.path,".katmat") { result.error=.Invalid_Operation; return result,{} }
    if request.action==.Write || request.action==.Capture || request.action==.Apply { if owner.mode!=.Editing { result.error=.Editing_Required; return result,{} } }
    scope,scope_error:=asset_path_scope(owner,.Project,request.path); if scope_error!=.None { result.error=.Invalid_Operation; return result,{} }; defer asset_path_scope_destroy(&scope)
    document:=request.document; owned_document:=false; defer { if owned_document { json.destroy_value(document) } }
    if request.action==.Read || request.action==.Apply {
        bytes,read_error:=resources.read_text(&scope.root,scope.path); if read_error!=.None { result.error=.Invalid_Operation; return result,{} }; defer delete(bytes,allocator)
        parsed,parse_error:=ron.parse(string(bytes),allocator); if parse_error.kind!=.None { result.error=.Decode_Failed; return result,{} }; document=parsed; owned_document=true
    }
    definition:Material_Asset; definition_error:editor.Scene_Error
    if request.action==.Capture { definition,definition_error=material_asset_capture(owner,request.entity) }
    else { definition,definition_error=material_asset_document_decode(owner,document,request.path) }
    if definition_error!=.None { result.error=definition_error; return result,{} }; defer material_asset_destroy(&definition,allocator)
    canonical,canonical_ok:=material_asset_document_encode(owner,definition,request.path); if !canonical_ok { result.error=.Decode_Failed; return result,{} }; defer json.destroy_value(canonical)
    if request.action==.Read { result.data,_=json.marshal(canonical,allocator=allocator); return result,{} }
    if request.action==.Apply { return material_asset_apply(owner,request,definition,canonical) }
    if error:=material_asset_validate_images(owner,&definition); error!=.None { result.error=error; return result,{} }
    if request.action==.Validate { result.data,_=json.marshal(struct {path:string,valid:bool,name:string}{request.path,true,definition.name},allocator=allocator); return result,{} }
    capture_id:=fmt.aprintf("%d",u64(request.entity)); defer delete(capture_id,allocator)
    response:[]byte; response_error:json.Marshal_Error
    if request.action==.Capture { response,response_error=json.marshal(struct {path:string,saved:bool,document:json.Value,entity_id:string}{request.path,true,canonical,capture_id},allocator=allocator) }
    else { response,response_error=json.marshal(struct {path:string,saved:bool,name:string}{request.path,true,definition.name},allocator=allocator) }
    if response_error!=nil { result.error=.Decode_Failed; return result,{} }
    bytes,write_error:=ron.write(canonical,allocator); if write_error.kind!=.None { delete(response,allocator); result.error=.Decode_Failed; return result,{} }; defer delete(bytes,allocator)
    if parent_error:=resources.make_parents(&scope.root,scope.path); parent_error!=.None { delete(response,allocator); result.error=.Invalid_Operation; return result,{} }
    published,native_error:=resources.write_atomic(&scope.root,scope.path,bytes)
    if published { result.data=response } else { delete(response,allocator) }; if native_error!=.None { result.error=.Invalid_Operation }
    return result,{}
}
@(private="package")
material_asset_apply :: proc(owner:^Authoring,request:asset.Material_Asset_Request,definition:Material_Asset,document:json.Value)->(editor.Tool_Result,editor.Undo_Group) {
    if error:=material_actor_targets(owner,request.entities); error!=.None { return error_result(&owner.world,error),{} }
    prepared,prepare_error:=material_images_prepare(owner,definition.textures); if prepare_error!=.None { return error_result(&owner.world,prepare_error),{} }; defer material_images_destroy(&prepared,owner.world.allocator)
    command:=scene_action_command_new(owner); transferred:=false; defer { if !transferred { scene_action_command_destroy(command,owner.world.allocator) } }
    policies:=material_sampling_roles(definition.material.sampling)
    choices:=definition.textures
    for id in request.entities {
        for source,index in texture_assignments_roles(&choices) { if source.kind!=.Neutral && !material_target_uv(owner,id,agent.Material_Texture_Role(index),policies[index].uv.tex_coord) { return error_result(&owner.world,.Invalid_Field_Value),{} } }
        before:=editor.entity_components_capture(&owner.world,&owner.registry,id); proposal:=scene_action_proposal_clone(owner,before[:])
        ecs.add_component(&owner.world,proposal,definition.material); ecs.add_component(&owner.world,proposal,texture_assignments_clone(definition.textures,owner.world.allocator))
        cloned:Material_Images; material_images_clone_value(&cloned,&prepared); ecs.add_component(&owner.world,proposal,cloned)
        after:=editor.entity_components_capture(&owner.world,&owner.registry,proposal); ecs.destroy_entity(&owner.world,proposal); append(&command.rows,Scene_Action_Row{id,true,true,before,after})
    }
    ids:=make([]string,len(request.entities),owner.world.allocator); defer { for id in ids { delete(id,owner.world.allocator) }; delete(ids,owner.world.allocator) }
    for id,index in request.entities { ids[index]=fmt.aprintf("%d",u64(id)) }
    data,error:=json.marshal(struct {path,name:string,entity_ids:[]string,batch_atomic,independently_editable:bool,document:json.Value}{request.path,definition.name,ids,true,true,document},allocator=owner.world.allocator)
    if error!=nil { return error_result(&owner.world,.Decode_Failed),{} }
    transferred=true; return material_actor_publish(owner,command,request.entities,data)
}
