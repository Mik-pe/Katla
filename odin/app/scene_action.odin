//! Editor and agent mutations prepare one owned proposal and share atomic history replay.
package app

import ecs "../ecs"
import editor "../editor"
import km "../math"

/// Executes one selected batch without recording history; callers record the returned group once.
scene_action_execute :: proc(owner:^Authoring,op:editor.Scene_Op,selected:[]ecs.Entity_Id=nil)->(editor.Tool_Result,editor.Undo_Group) {
    context.allocator=owner.world.allocator
    if op.kind in (bit_set[editor.Scene_Op_Kind]{.Spawn,.Spawn_Model}) { return scene_action_execute_batch(owner,{op}) }
    if op.kind==.Application { return authoring_application(owner,op) }
    if op.kind in (bit_set[editor.Scene_Op_Kind]{.Query_Entities,.Get_Hierarchy}) { return scene_action_query(owner,op),{} }
    if op.kind in (bit_set[editor.Scene_Op_Kind]{.List_Components,.Get_Attributes}) {
        if op.kind==.Get_Attributes { if error:=scene_action_target(owner,op.entity); error!=.None { return error_result(&owner.world,error),{} } }
        return editor.scene_execute(&owner.world,&owner.registry,op)
    }
    if owner.mode!=.Editing { return error_result(&owner.world,.Editing_Required),{} }
    if error:=authoring_before_mutation(owner); error!=.None { return error_result(&owner.world,error),{} }
    command:=scene_action_command_new(owner)
    transferred:=false
    defer { if !transferred { scene_action_command_destroy(command,owner.world.allocator) } }
    created:=make([dynamic]ecs.Entity_Id,owner.world.allocator)
    accepted:=false
    defer { if !accepted { for id in created { ecs.destroy_entity(&owner.world,id) } }; delete(created) }
    identity:=ecs.get_resource_mut(&owner.world,Scene_Identity)
    if identity==nil { return error_result(&owner.world,.Invalid_Operation),{} }
    next_key:=identity.next_entity_id
    ids:=make([dynamic]ecs.Entity_Id,owner.world.allocator); defer delete(ids)
    {
        if len(selected)==0 { append(&ids,op.entity) } else {
            seen:=make(map[ecs.Entity_Id]bool,owner.world.allocator); defer delete(seen)
            for id in selected { if !seen[id] { append(&ids,id); seen[id]=true } }
        }
        for id in ids { if error:=scene_action_target(owner,id); error!=.None { return error_result(&owner.world,error),{} } }
        if op.kind in (bit_set[editor.Scene_Op_Kind]{.Destroy,.Duplicate}) {
            roots:=ids[:]
            expanded:=make([dynamic]ecs.Entity_Id,owner.world.allocator)
            seen:=make(map[ecs.Entity_Id]bool,owner.world.allocator); defer delete(seen)
            for root in roots {
                subtree,error:=scene_subtree(owner,root)
                if error!=.None { delete(expanded); return error_result(&owner.world,error),{} }
                for id in subtree { if !seen[id] { seen[id]=true; append(&expanded,id) } }; delete(subtree,owner.world.allocator)
            }
            delete(ids); ids=expanded
        }
        if op.kind==.Duplicate {
            if u64(len(ids))>max(u64)-next_key { return error_result(&owner.world,.Invalid_Operation),{} }
            mapping:=make(map[ecs.Entity_Id]ecs.Entity_Id,owner.world.allocator); defer delete(mapping)
            for source in ids { target:=ecs.create_entity(&owner.world); append(&created,target); mapping[source]=target }
            for source in ids {
                before:=editor.entity_components_capture(&owner.world,&owner.registry,source)
                proposal:=ecs.create_entity(&owner.world)
                for value in before {
                    if !value.entry.duplicate { continue }
                    clone:=editor.editor_clone_value(value.entry,value.value,owner.world.allocator)
                    assert(editor.component_map_references(value.entry,clone,{mapping,false}))
                    assert(ecs.insert_component_value(&owner.world,proposal,value.entry.T,clone))
                    free(clone,owner.world.allocator)
                }
                editor.entity_components_destroy(before,owner.world.allocator)
                ecs.add_component(&owner.world,proposal,Scene_Key{next_key}); next_key+=1
                parent,has_parent:=ecs.get_component(&owner.world,source,Scene_Parent)
                if op.has_position_offset && (!has_parent || !(parent.entity in mapping)) {
                    if transform:=ecs.get_component_mut(&owner.world,proposal,Scene_Transform); transform!=nil { transform.local.position+=km.Vec3(op.position_offset) }
                }
                values:=editor.entity_components_capture(&owner.world,&owner.registry,proposal); ecs.destroy_entity(&owner.world,proposal)
                append(&command.rows,Scene_Action_Row{entity=mapping[source],after_exists=true,after=values})
            }
        } else {
            for id in ids {
                before:=editor.entity_components_capture(&owner.world,&owner.registry,id)
                row:=Scene_Action_Row{entity=id,before_exists=true,before=before}
                if op.kind!=.Destroy {
                    proposal:=scene_action_proposal_clone(owner,before[:])
                    error:=scene_action_mutate_proposal(owner,proposal,op)
                    if error!=.None { ecs.destroy_entity(&owner.world,proposal); editor.entity_components_destroy(before,owner.world.allocator); return error_result(&owner.world,error),{} }
                    row.after_exists=true; row.after=editor.entity_components_capture(&owner.world,&owner.registry,proposal); ecs.destroy_entity(&owner.world,proposal)
                }
                append(&command.rows,row)
            }
            if op.kind==.Destroy { scene_action_remove_references(owner,command,ids[:]) }
        }
    }
    group:=scene_action_command_group(command); transferred=true
    error:=editor.redo_group(&owner.world,&owner.registry,&group)
    if error!=.None { editor.undo_group_destroy(&group); return error_result(&owner.world,error),{} }
    identity.next_entity_id=next_key; accepted=true
    result:=error_result(&owner.world,.None)
    if op.kind in (bit_set[editor.Scene_Op_Kind]{.Spawn,.Spawn_Model,.Duplicate}) { append(&result.entities,..created[:]) }
    else { append(&result.entities,..ids[:]) }
    return result,group
}
@(private="package")
scene_action_target :: proc(owner:^Authoring,id:ecs.Entity_Id)->editor.Scene_Error {
    if !ecs.entity_exists(&owner.world,id) { return .Entity_Not_Found }
    if _,hidden:=ecs.get_component(&owner.world,id,Editor_Hidden); hidden { return .Protected_Entity }
    return .None
}
@(private="package")
scene_action_mutate_proposal :: proc(owner:^Authoring,id:ecs.Entity_Id,op:editor.Scene_Op)->editor.Scene_Error {
    #partial switch op.kind {
    case .Set_Field:
        if entry:=owner.registry.entries[op.component]; entry!=nil && entry.T==Particle_Emitter { return particle_edit_field(owner,id,op) }
        return editor.editor_set_field(&owner.world,&owner.registry,id,op.component,op.field,op.value)
    case .Add_Component,.Remove_Component:
        entry:=owner.registry.entries[op.component]; if entry==nil { return .Component_Not_Found }
        if entry.T==Scene_Key { return .Protected_Entity }
        if op.kind==.Add_Component { editor.editor_add_default(&owner.world,id,entry) }
        else if !ecs.remove_component_type(&owner.world,id,entry.T) { return .Component_Not_Found }
        return .None
    case .Set_Parent:
        if op.has_parent {
            if error:=scene_action_target(owner,op.parent); error!=.None { return error }
            ecs.add_component(&owner.world,id,Scene_Parent{op.parent})
        } else { ecs.remove_component(&owner.world,id,Scene_Parent) }
        return .None
    case: return .Invalid_Operation
    }
}
@(private="package")
scene_action_remove_references :: proc(owner:^Authoring,command:^Scene_Action_Command,removed:[]ecs.Entity_Id) {
    removed_ids:=make(map[ecs.Entity_Id]bool,owner.world.allocator); defer delete(removed_ids)
    for target in removed { removed_ids[target]=true }
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    for id in ids {
        if removed_ids[id] { continue }
        joint,has_joint:=ecs.get_component(&owner.world,id,Physics_Joint)
        remove_joint:=has_joint && (removed_ids[joint.a] || removed_ids[joint.b])
        rules,has_rules:=ecs.get_component(&owner.world,id,Trigger_Rules)
        affected_rules:=false
        if has_rules { for rule in rules.rules {
            if rule.has_other && removed_ids[rule.other] { affected_rules=true; break }
            for action in rule.actions { if action.kind!=.Emit && action.target.kind==.Entity && removed_ids[action.target.entity] { affected_rules=true; break } }
            if affected_rules { break }
        } }
        volume,has_volume:=ecs.get_component(&owner.world,id,Trigger_Volume)
        affected_volume:=false
        if has_volume { for target in volume.overlapping { if removed_ids[target] { affected_volume=true; break } } }
        if !remove_joint && !affected_rules && !affected_volume { continue }
        before:=editor.entity_components_capture(&owner.world,&owner.registry,id)
        proposal:=scene_action_proposal_clone(owner,before[:])
        if remove_joint { ecs.remove_component(&owner.world,proposal,Physics_Joint) }
        if affected_rules { scene_action_prune_rules(ecs.get_component_mut(&owner.world,proposal,Trigger_Rules),removed_ids) }
        if affected_volume {
            pending:=ecs.get_component_mut(&owner.world,proposal,Trigger_Volume)
            for i:=len(pending.overlapping)-1; i>=0; i-=1 { if removed_ids[pending.overlapping[i]] { ordered_remove(&pending.overlapping,i) } }
        }
        after:=editor.entity_components_capture(&owner.world,&owner.registry,proposal); ecs.destroy_entity(&owner.world,proposal)
        append(&command.rows,Scene_Action_Row{id,true,true,before,after})
    }
}
