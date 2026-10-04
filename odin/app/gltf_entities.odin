//! Imported scene controllers own one immutable model revision and independently authored primitive children.
package app

import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:fmt"

/// Resolves the nearest explicit player in the live hierarchy without duplicating playback state.
scene_model_animation_player :: proc(owner:^Authoring,id:ecs.Entity_Id)->^Animation_Player {
    cursor:=id
    for _ in 0..<owner.world.live_count {
        if player:=ecs.get_component_mut(&owner.world,cursor,Animation_Player); player!=nil { return player }
        parent,present:=ecs.get_component(&owner.world,cursor,Scene_Parent); if !present { return nil }
        cursor=parent.entity
    }
    return nil
}
@(private="package")
scene_model_expand_children :: proc(owner:^Authoring,id:ecs.Entity_Id,entities:^[dynamic]ecs.Entity_Id,next_key:^u64)->editor.Scene_Error {
    component:=ecs.get_component_mut(&owner.world,id,Scene_Model); if component==nil || component.source.kind!=.Group { return .None }
    // A local header remains stable while child insertions move ECS storage.
    source:=component^; scene_model_revision_own(&source,owner.world.allocator); component.revision=source.revision
    active,error:=gltf_active_nodes(&source.model,owner.world.allocator); if error!=.None { return error }; defer delete(active,owner.world.allocator)
    override,has_override:=ecs.get_component(&owner.world,id,Surface_Material)
    choices,has_choices:=ecs.get_component(&owner.world,id,Texture_Assignments)
    for node,node_index in source.model.nodes {
        if !active[node_index] || node.mesh<0 { continue }
        ordinal:u32
        for primitive in source.model.primitives {
            if primitive.mesh!=u32(node.mesh) { continue }
            primitive_index:=ordinal; ordinal+=1
            if next_key^==max(u64) || len(entities)>=100_000 { return .Invalid_Operation }
            selected,valid:=scene_model_select(&source,u32(node_index),primitive_index,owner.world.allocator); if !valid { return .Invalid_Operation }
            child:=ecs.create_entity(&owner.world); append(entities,child)
            ecs.add_component(&owner.world,child,selected)
            ecs.add_component(&owner.world,child,Scene_Parent{id})
            ecs.add_component(&owner.world,child,Scene_Transform{km.TRANSFORM_IDENTITY})
            ecs.add_component(&owner.world,child,Scene_Key{next_key^}); next_key^+=1
            name:=node.name; if name=="" { name="Primitive" }
            label:=fmt.aprintf("%s %d",name,primitive_index); ecs.add_component(&owner.world,child,Scene_Name{label})
            material:=material_primitive_defaults(&selected,primitive)
            if has_override { material=override }
            ecs.add_component(&owner.world,child,material)
            if has_choices { ecs.add_component(&owner.world,child,texture_assignments_clone(choices,owner.world.allocator)) }
        }
    }
    ecs.remove_component(&owner.world,id,Surface_Material)
    ecs.remove_component(&owner.world,id,Texture_Assignments)
    ecs.remove_component(&owner.world,id,Material_Images)
    return .None
}
