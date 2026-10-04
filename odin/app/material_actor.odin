//! Image and sampling mutations publish one prepared application command for the entire target batch.
package app

import agent "../agent"
import ecs "../ecs"
import editor "../editor"
import gfx "../gfx"
import "core:encoding/json"
import "core:strings"
import "core:fmt"

@(private="package")
material_actor_targets :: proc(owner:^Authoring,ids:[]ecs.Entity_Id,gate:bool=true)->editor.Scene_Error {
    if owner.mode!=.Editing { return .Editing_Required }
    if len(ids)==0 || len(ids)>256 { return .Invalid_Operation }
    for id,index in ids { for previous in ids[:index] { if previous==id { return .Invalid_Operation } }; if !scene_material_editable(owner,id) { return .Component_Not_Found } }
    return authoring_before_mutation(owner) if gate else .None
}
@(private="package")
material_actor_publish :: proc(owner:^Authoring,command:^Scene_Action_Command,ids:[]ecs.Entity_Id,data:[]byte=nil)->(editor.Tool_Result,editor.Undo_Group) {
    data:=data
    if data==nil { bytes,error:=json.marshal(struct {batch_atomic:bool,independently_editable:bool}{true,true},allocator=owner.world.allocator); if error!=nil { scene_action_command_destroy(command,owner.world.allocator); return error_result(&owner.world,.Decode_Failed),{} }; data=bytes }
    group:=scene_action_command_group(command)
    error:=editor.redo_group(&owner.world,&owner.registry,&group)
    if error!=.None { delete(data,owner.world.allocator); editor.undo_group_destroy(&group); return error_result(&owner.world,error),{} }
    result:=error_result(&owner.world,.None); append(&result.entities,..ids)
    result.data=data
    return result,group
}
@(private="package")
Material_Role_Inspect :: struct {
    uv:Uv_Transform,
    sampler:struct {minification,magnification,wrap_u,wrap_v:string,anisotropy:u32},
}
material_role_inspect :: proc(value:Texture_Sampling)->Material_Role_Inspect {
    minimum:="nearest"
    if value.sampler.mip_filter==.None || value.sampler.max_lod==0 { minimum="linear" if value.sampler.min_filter==.Linear else "nearest" }
    else if value.sampler.mip_filter==.Nearest { minimum="linear_mipmap_nearest" if value.sampler.min_filter==.Linear else "nearest_mipmap_nearest" }
    else { minimum="linear_mipmap_linear" if value.sampler.min_filter==.Linear else "nearest_mipmap_linear" }
    wraps:=[4]string{"repeat","mirrored_repeat","clamp_to_edge","clamp_to_edge"}
    return {value.uv,{minimum,"linear" if value.sampler.mag_filter==.Linear else "nearest",wraps[int(value.sampler.address_u)],wraps[int(value.sampler.address_v)],value.sampler.max_anisotropy}}
}
/// Encodes the five effective UI and transport roles using primary sampler vocabulary.
material_sampling_inspect :: proc(value:Material_Sampling)->struct {albedo,normal,metallic_roughness,occlusion,emission:Material_Role_Inspect} {
    return {material_role_inspect(value.albedo),material_role_inspect(value.normal),material_role_inspect(value.metallic_roughness),material_role_inspect(value.occlusion),material_role_inspect(value.emission)}
}
@(private="package")
material_role_needs_uv :: proc(owner:^Authoring,id:ecs.Entity_Id,role:agent.Material_Texture_Role)->bool {
    if assignments,present:=ecs.get_component(&owner.world,id,Texture_Assignments); present { source:=texture_assignments_roles(&assignments)[int(role)]; if source.kind==.Neutral { return false }; if source.kind!=.Inherit { return true } }
    model,material,error:=material_imported_material(owner,id); return error==.None && model!=nil && material_imported_views(material)[int(role)].texture>=0
}
/// Patches one complete role, preserving every untouched imported or authored role.
material_sampling_execute :: proc(owner:^Authoring,op:agent.Material_Set_Sampling)->(editor.Tool_Result,editor.Undo_Group) { return material_sampling_execute_internal(owner,op,true) }
@(private="package")
material_sampling_execute_internal :: proc(owner:^Authoring,op:agent.Material_Set_Sampling,gate:bool)->(editor.Tool_Result,editor.Undo_Group) {
    context.allocator=owner.world.allocator
    if error:=material_actor_targets(owner,op.entities,gate); error!=.None { return error_result(&owner.world,error),{} }
    if int(op.role)<0 || int(op.role)>=5 || op.patch.fields=={} { return error_result(&owner.world,.Invalid_Operation),{} }
    rows:=make([]struct {entity_id:string,before,sampling:Material_Role_Inspect},len(op.entities),owner.world.allocator)
    defer { for row in rows { delete(row.entity_id,owner.world.allocator) }; delete(rows,owner.world.allocator) }
    command:=scene_action_command_new(owner); transferred:=false; defer { if !transferred { scene_action_command_destroy(command,owner.world.allocator) } }
    for id,index in op.entities {
        before:=editor.entity_components_capture(&owner.world,&owner.registry,id); proposal:=scene_action_proposal_clone(owner,before[:])
        accepted:=false; defer { ecs.destroy_entity(&owner.world,proposal); if !accepted { editor.entity_components_destroy(before,owner.world.allocator) } }
        surface:=ecs.get_component_mut(&owner.world,proposal,Surface_Material)
        effective,error:=material_effective_sampling(owner,id); if error!=.None { return error_result(&owner.world,error),{} }
        rows[index].entity_id=fmt.aprintf("%d",u64(id)); rows[index].before=material_role_inspect(material_sampling_roles(effective)[int(op.role)])
        surface.sampling=effective; surface.has_sampling=true
        roles:=[5]^Texture_Sampling{&surface.sampling.albedo,&surface.sampling.normal,&surface.sampling.metallic_roughness,&surface.sampling.occlusion,&surface.sampling.emission}; role:=roles[int(op.role)]; patch:=op.patch
        if .Tex_Coord in patch.fields { role.uv.tex_coord=patch.tex_coord }
        if .Offset in patch.fields { role.uv.offset=patch.offset }
        if .Rotation in patch.fields { role.uv.rotation=patch.rotation }
        if .Scale in patch.fields { role.uv.scale=patch.scale }
        if .Minification in patch.fields { role.sampler.min_filter=.Nearest; role.sampler.mip_filter=.None; role.sampler.max_lod=0; switch patch.minification {
            case .Nearest:
            case .Linear: role.sampler.min_filter=.Linear
            case .Nearest_Mipmap_Nearest: role.sampler.mip_filter=.Nearest; role.sampler.max_lod=32
            case .Linear_Mipmap_Nearest: role.sampler.min_filter=.Linear; role.sampler.mip_filter=.Nearest; role.sampler.max_lod=32
            case .Nearest_Mipmap_Linear: role.sampler.mip_filter=.Linear; role.sampler.max_lod=32
            case .Linear_Mipmap_Linear: role.sampler.min_filter=.Linear; role.sampler.mip_filter=.Linear; role.sampler.max_lod=32
            } }
        if .Magnification in patch.fields { role.sampler.mag_filter=.Linear if patch.magnification==.Linear else .Nearest }
        wraps:=[3]gfx.Address_Mode{.Repeat,.Clamp_Edge,.Mirror_Repeat}
        if .Wrap_U in patch.fields { role.sampler.address_u=wraps[int(patch.wrap_u)] }
        if .Wrap_V in patch.fields { role.sampler.address_v=wraps[int(patch.wrap_v)] }
        if .Anisotropy in patch.fields { role.sampler.max_anisotropy=u32(patch.anisotropy) }
        if !material_sampling_valid(surface.sampling) || (material_role_needs_uv(owner,id,op.role) && !material_target_uv(owner,id,op.role,role.uv.tex_coord)) { return error_result(&owner.world,.Invalid_Field_Value),{} }
        rows[index].sampling=material_role_inspect(role^)
        after:=editor.entity_components_capture(&owner.world,&owner.registry,proposal); append(&command.rows,Scene_Action_Row{id,true,true,before,after}); accepted=true
    }
    payload,payload_error:=json.marshal(struct {role:string,materials:type_of(rows),rotation_unit:string,image_bindings_preserved,batch_atomic:bool}{agent.material_texture_role_name(op.role),rows,"radians",true,true},allocator=owner.world.allocator)
    if payload_error!=nil { return error_result(&owner.world,.Decode_Failed),{} }
    transferred=true; return material_actor_publish(owner,command,op.entities,payload)
}
/// Resolves and decodes a new role before preparing any entity or native resource publication.
material_texture_execute :: proc(owner:^Authoring,op:agent.Material_Set_Texture)->(editor.Tool_Result,editor.Undo_Group) {
    context.allocator=owner.world.allocator
    if error:=material_actor_targets(owner,op.entities); error!=.None { return error_result(&owner.world,error),{} }
    if int(op.role)<0 || int(op.role)>=5 { return error_result(&owner.world,.Invalid_Operation),{} }
    source:Texture_Source; source.kind=Texture_Source_Kind(op.source.kind); source.image_index=op.source.image_index
    defer delete(source.path,owner.world.allocator)
    if source.kind==.File || source.kind==.GltfImage {
        variant:="Resource"; if op.source.root==.Scene { variant="Scene" }; if op.source.root==.File { variant="File" }
        value:=make(json.Object,owner.world.allocator); scene_json_put(&value,variant,strings.clone(op.source.path)); defer json.destroy_value(value)
        metadata:=ecs.get_resource_mut(&owner.world,Scene_File_State); origin:=""; if metadata!=nil { origin=metadata.path }
        path,root,valid:=scene_asset_path(owner,value,origin); if !valid { return error_result(&owner.world,.Invalid_Operation),{} }; source.path=path; source.root=root
    }
    command:=scene_action_command_new(owner); transferred:=false; defer { if !transferred { scene_action_command_destroy(command,owner.world.allocator) } }
    for id in op.entities {
        before:=editor.entity_components_capture(&owner.world,&owner.registry,id); proposal:=scene_action_proposal_clone(owner,before[:]); accepted:=false
        defer { ecs.destroy_entity(&owner.world,proposal); if !accepted { editor.entity_components_destroy(before,owner.world.allocator) } }
        assignments,present:=ecs.get_component(&owner.world,proposal,Texture_Assignments); if present { assignments=texture_assignments_clone(assignments,owner.world.allocator) }; defer texture_assignments_destroy(&assignments,owner.world.allocator)
        roles:=texture_assignments_roles(&assignments); target:=roles[int(op.role)]; delete(target.path,owner.world.allocator); target^=source; target.path=strings.clone(source.path,owner.world.allocator)
        sampling,error:=material_effective_sampling(owner,id); if error!=.None { return error_result(&owner.world,error),{} }; policies:=material_sampling_roles(sampling)
        if (source.kind==.File || source.kind==.GltfImage || (source.kind==.Inherit && material_role_needs_uv(owner,id,op.role))) && !material_target_uv(owner,id,op.role,policies[int(op.role)].uv.tex_coord) { return error_result(&owner.world,.Invalid_Field_Value),{} }
        previous:=ecs.get_component_mut(&owner.world,proposal,Material_Images)
        prepared,prepare_error:=material_images_prepare(owner,assignments,previous,int(op.role)); if prepare_error!=.None { return error_result(&owner.world,prepare_error),{} }
        ecs.add_component(&owner.world,proposal,texture_assignments_clone(assignments,owner.world.allocator)); ecs.add_component(&owner.world,proposal,prepared)
        after:=editor.entity_components_capture(&owner.world,&owner.registry,proposal); append(&command.rows,Scene_Action_Row{id,true,true,before,after}); accepted=true
    }
    payload,valid:=material_texture_receipt(owner,op,command,source); if !valid { return error_result(&owner.world,.Decode_Failed),{} }
    transferred=true; return material_actor_publish(owner,command,op.entities,payload)
}
