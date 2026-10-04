//! Asset drops prepare all imports before publishing one native revision and history group.
package app

import ecs "../ecs"
import editor "../editor"
import asset "../agent/assets"
import resources "../resources"
import ron "../encoding/ron"
import km "../math"
import "core:strings"
import "core:path/filepath"
import "core:encoding/json"

/// Inserts a mixed primitive, model and prefab batch atomically; returned roots precede descendants.
scene_action_execute_batch :: proc(owner:^Authoring,operations:[]editor.Scene_Op)->(editor.Tool_Result,editor.Undo_Group) {
    context.allocator=owner.world.allocator
    model_preparation_owned:=scene_model_preparation_begin(owner); defer scene_model_preparation_end(owner,model_preparation_owned)
    if owner.mode!=.Editing { return error_result(&owner.world,.Editing_Required),{} }
    if len(operations)==0 || len(operations)>256 { return error_result(&owner.world,.Invalid_Operation),{} }
    if error:=authoring_before_mutation(owner); error!=.None { return error_result(&owner.world,error),{} }
    identity:=ecs.get_resource_mut(&owner.world,Scene_Identity); if identity==nil { return error_result(&owner.world,.Invalid_Operation),{} }
    next_key:=identity.next_entity_id
    created:=make([dynamic]ecs.Entity_Id,owner.world.allocator); defer delete(created)
    roots:=make([dynamic]ecs.Entity_Id,owner.world.allocator); defer delete(roots)
    source_preparations:=make([dynamic]Script_Source_Preparation,owner.world.allocator); defer delete(source_preparations)
    accepted:=false; defer { for &source in source_preparations { script_sources_finish(owner,&source,accepted) } }
    defer { if !accepted { for id in created { ecs.destroy_entity(&owner.world,id) } } }
    for op in operations {
        if op.kind in (bit_set[editor.Scene_Op_Kind]{.Spawn,.Spawn_Model}) {
            if next_key==max(u64) { return error_result(&owner.world,.Invalid_Operation),{} }
            id:=ecs.create_entity(&owner.world); append(&created,id); append(&roots,id)
            if error:=scene_action_spawn_proposal(owner,id,op,next_key); error!=.None { return error_result(&owner.world,error),{} }; next_key+=1
            if error:=scene_model_expand_children(owner,id,&created,&next_key); error!=.None { return error_result(&owner.world,error),{} }
        } else if op.kind==.Application && op.tool_name=="prefab" {
            decoded,decode_error:=asset.prefab_decode(op.value,owner.world.allocator)
            if decode_error!=.None { return error_result(&owner.world,.Invalid_Operation),{} }; defer asset.prefab_destroy(&decoded)
            request:=decoded.request
            if request.action!=.Instantiate { return error_result(&owner.world,.Invalid_Operation),{} }
            root_kind:Mesh_Path_Root=.Project; if filepath.is_abs(request.path) { root_kind=.File }
            scope,scope_error:=asset_path_scope(owner,root_kind,request.path); if scope_error!=.None { return error_result(&owner.world,.Invalid_Operation),{} }; defer asset_path_scope_destroy(&scope)
            data,read_error:=resources.read_text(&scope.root,scope.path)
            if read_error!=.None { return error_result(&owner.world,.Invalid_Operation),{} }; defer delete(data,owner.world.allocator)
            document,parse_error:=ron.parse(string(data),owner.world.allocator)
            if parse_error.kind!=.None { return error_result(&owner.world,.Decode_Failed),{} }; defer json.destroy_value(document)
            prefab,prepare_error:=prefab_document_decode(owner,document,request.path)
            if prepare_error!=.None { return error_result(&owner.world,prepare_error),{} }; defer prefab_document_destroy(&prefab)
            stage,stage_error:=scene_snapshot_stage(owner,&prefab.scene,next_key)
            if stage_error!=.None { return error_result(&owner.world,stage_error),{} }
            append(&created,..stage.entities[:]); next_key=stage.next_key
            root:=stage.mapping[prefab.root]; append(&roots,root)
            append(&source_preparations,stage.script_sources); stage.script_sources={}
            scene_stage_destroy(owner,&stage,false)
            placement:=km.Transform{position=request.position,rotation=km.Quat(request.rotation),scale=request.scale}
            if !km.quat_is_normalized(placement.rotation) { return error_result(&owner.world,.Invalid_Field_Value),{} }
            for vector in ([2]km.Vec3{placement.position,placement.scale}) { for axis in vector { if !mesh_finite(axis) { return error_result(&owner.world,.Invalid_Field_Value),{} } } }
            for axis in placement.scale { if axis==0 { return error_result(&owner.world,.Invalid_Field_Value),{} } }
            ecs.add_component(&owner.world,root,Scene_Transform{placement})
            if request.name!="" { ecs.add_component(&owner.world,root,Scene_Name{strings.clone(request.name,owner.world.allocator)}) }
        } else { return error_result(&owner.world,.Invalid_Operation),{} }
        if len(created)>100_000 { return error_result(&owner.world,.Invalid_Operation),{} }
    }
    command:=scene_action_command_new(owner)
    for id in created { values:=editor.entity_components_capture(&owner.world,&owner.registry,id); append(&command.rows,Scene_Action_Row{entity=id,after_exists=true,after=values}) }
    group:=scene_action_command_group(command)
    if error:=editor.redo_group(&owner.world,&owner.registry,&group); error!=.None { editor.undo_group_destroy(&group); return error_result(&owner.world,error),{} }
    identity.next_entity_id=next_key; accepted=true
    result:=error_result(&owner.world,.None); append(&result.entities,..roots[:])
    for id in created { is_root:=false; for root in roots { if root==id { is_root=true; break } }; if !is_root { append(&result.entities,id) } }
    return result,group
}
