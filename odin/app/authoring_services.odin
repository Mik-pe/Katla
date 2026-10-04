//! Application services install durable scene components and execute typed tools on the owner.
package app

import scene "../agent/scene"
import asset "../agent/assets"
import agent "../agent"
import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:strings"

/// Installs scene, animation, physics, preview and behavior authority without loading foreign runtimes.
authoring_services_init :: proc(app:^Authoring)->editor.Scene_Error {
    for name in ([14]string{"SceneKey","SceneName","SceneTransform","SceneParent","AnimationModel","AnimationPlayer","PhysicsBody","Velocity","Script","TriggerRules","TriggerVolume","ParticleEmitter","SceneMesh","SceneUnknown"}) {
        if app.registry.entries[name]!=nil { return .Invalid_Operation }
    }
    scene_components_register(app); scene_mesh_register(app); scene_document_register(app)
    animation_register(&app.world,&app.registry); physics_register(app); behavior_register(app)
    events_register(app); simulation_init(app)
    return .None
}
@(private="package")
authoring_application :: proc(app:^Authoring,op:editor.Scene_Op)->(editor.Tool_Result,editor.Undo_Group) {
    allocator:=app.world.allocator
    switch op.tool_name {
    case "material":
        decoded,err:=agent.decode_material(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer agent.decoded_material_destroy(&decoded)
        return material_execute(app,decoded.operation)
    case "animation":
        decoded,err:=scene.decode_animation(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer scene.decoded_animation_destroy(&decoded)
        return animation_execute(app,decoded.operation)
    case "simulation":
        decoded,err:=scene.decode_simulation(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }
        return simulation_execute(app,decoded)
    case "behavior":
        decoded,err:=scene.decode_behavior(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer scene.decoded_behavior_destroy(&decoded)
        return behavior_execute(app,decoded.operation)
    case "trigger":
        decoded,err:=scene.decode_trigger(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer scene.decoded_trigger_destroy(&decoded)
        return trigger_execute(app,decoded.operation)
    case "prefab":
        decoded,err:=asset.prefab_decode(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer asset.prefab_destroy(&decoded)
        return asset_authoring_execute(app,decoded.request)
    case "search_assets","list_resources","read_resource":
        decoded,err:=asset.decode(op.tool_name,op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer asset.destroy(&decoded)
        return asset_execute(app,decoded.request)
    }
    return error_result(&app.world,.Application_Owned),{}
}
@(private="package")
authoring_spawn :: proc(app:^Authoring,op:editor.Scene_Op)->(editor.Tool_Result,editor.Undo_Group) {
    if app.mode!=.Editing { return error_result(&app.world,.Editing_Required),{} }
    identity:=ecs.get_resource_mut(&app.world,Scene_Identity)
    if identity==nil || identity.next_entity_id==max(u64) { return error_result(&app.world,.Invalid_Operation),{} }
    w:=&app.world; reg:=&app.registry; context.allocator=w.allocator
    edit,err:=editor.entity_edit_begin(w,reg,0,false)
    if err!=.None { return error_result(w,err),{} }; defer editor.entity_edit_destroy(&edit)
    id:=ecs.create_entity(w)
    for _,entry in reg.entries { if entry.spawn_default { editor.editor_add_default(w,id,entry) } }
    rotation:=km.quat_mul(km.quat_mul(km.quat_axis_angle(km.VEC3_Z,op.rotation[2]),km.quat_axis_angle(km.VEC3_Y,op.rotation[1])),km.quat_axis_angle(km.VEC3_X,op.rotation[0]))
    ecs.add_component(w,id,Scene_Transform{km.Transform{position=op.position,scale=op.scale,rotation=rotation}})
    if name:=ecs.get_component_mut(w,id,Scene_Name); name!=nil { delete(name.name,w.allocator); name.name=strings.clone(op.name,w.allocator) }
    ecs.add_component(w,id,Scene_Key{identity.next_entity_id}); identity.next_entity_id+=1
    group:=editor.entity_edit_finish(&edit,w,id)
    result:=error_result(w,.None); append(&result.entities,id)
    return result,group
}
