//! Collider removal retires dependent authored settings and constraints in one reversible proposal.
package app
import ecs "../ecs"
import editor "../editor"

@(private="package")
physics_collider_edit_field :: proc(owner:^Authoring,id:ecs.Entity_Id,op:editor.Scene_Op)->editor.Scene_Error {
    body:=ecs.get_component_mut(&owner.world,id,Physics_Body); if body==nil { return .Component_Not_Found }
    had_collider:=body.has_collider
    if error:=editor.editor_set_field(&owner.world,&owner.registry,id,op.component,op.field,op.value); error!=.None { return error }
    body=ecs.get_component_mut(&owner.world,id,Physics_Body)
    if !had_collider && body.has_collider && body.shape.kind==.None {
        body.shape={kind=.Box,half_extents={.5,.5,.5},radius=.5,half_height=.5}
    }
    if had_collider && !body.has_collider {
        delete(body.shape.heights,owner.world.allocator); body.shape={kind=.None}
        body.has_material=false; body.has_filter=false; body.sensor=false
        body.friction=.5; body.restitution=0; body.density=1; body.layers=max(u32); body.mask=max(u32)
        ecs.remove_component(&owner.world,id,Trigger_Volume); ecs.remove_component(&owner.world,id,Trigger_Rules)
    }
    return .None
}
@(private="package")
physics_collider_remove_joints :: proc(owner:^Authoring,command:^Scene_Action_Command)->editor.Scene_Error {
    removed:=make(map[ecs.Entity_Id]bool,owner.world.allocator); defer delete(removed)
    body_entry:=owner.registry.entries["PhysicsBody"]
    for row in command.rows {
        if !row.before_exists || !row.after_exists { continue }
        before:=cast(^Physics_Body)scene_action_component_find(row.before[:],body_entry)
        after:=cast(^Physics_Body)scene_action_component_find(row.after[:],body_entry)
        if before!=nil && before.has_collider && after!=nil && !after.has_collider { removed[row.entity]=true }
    }
    if len(removed)==0 { return .None }
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    for id in ids {
        joint,present:=ecs.get_component(&owner.world,id,Physics_Joint)
        if !present || (!removed[joint.a] && !removed[joint.b]) { continue }
        if error:=scene_action_target(owner,id); error!=.None { return error }
        index:=-1; for row,i in command.rows { if row.entity==id { index=i; break } }
        if index>=0 {
            row:=&command.rows[index]; if !row.after_exists { continue }
            proposal:=scene_action_proposal_clone(owner,row.after[:]); ecs.remove_component(&owner.world,proposal,Physics_Joint)
            editor.entity_components_destroy(row.after,owner.world.allocator); row.after=editor.entity_components_capture(&owner.world,&owner.registry,proposal); ecs.destroy_entity(&owner.world,proposal)
        } else {
            before:=editor.entity_components_capture(&owner.world,&owner.registry,id)
            proposal:=scene_action_proposal_clone(owner,before[:]); ecs.remove_component(&owner.world,proposal,Physics_Joint)
            after:=editor.entity_components_capture(&owner.world,&owner.registry,proposal); ecs.destroy_entity(&owner.world,proposal)
            append(&command.rows,Scene_Action_Row{id,true,true,before,after})
        }
    }
    return .None
}
