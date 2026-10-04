//! Application services install durable scene components and execute typed tools on the owner.
package app

import scene "../agent/scene"
import asset "../agent/assets"
import agent "../agent"
import editor "../editor"

/// Installs scene, animation, physics, preview and behavior authority without loading foreign runtimes.
authoring_services_init :: proc(app:^Authoring)->editor.Scene_Error {
    for name in ([25]string{"Billboard","Perspective","AudioSource","AudioEmitter","AudioListener","ReverbZone","SceneSource","PointLight","DirectionalLight","SceneKey","SceneName","SceneTransform","SceneParent","AnimationModel","AnimationPlayer","PhysicsBody","Velocity","Script","TriggerRules","TriggerVolume","ParticleEmitter","SceneMesh","SceneUnknown","SceneModel","PhysicsJoint"}) {
        if app.registry.entries[name]!=nil { return .Invalid_Operation }
    }
    scene_components_register(app); scene_mesh_register(app); scene_model_register(app); scene_document_register(app); light_register(app); audio_register(app); perspective_register(app); billboard_register(app)
    animation_register(&app.world,&app.registry); physics_register(app); behavior_register(app)
    events_register(app); simulation_init(app)
    for name in ([8]string{"SceneKey","SceneParent","SceneMesh","SceneModel","SceneSource","SceneUnknown","AnimationModel","PhysicsJoint"}) {
        entry:=app.registry.entries[name]; if entry!=nil { entry.inspector_add=false; entry.inspector_remove=false }
    }
    return .None
}
@(private="package")
authoring_application :: proc(app:^Authoring,op:editor.Scene_Op)->(editor.Tool_Result,editor.Undo_Group) {
    allocator:=app.world.allocator
    switch op.tool_name {
    case "generate_resource":
        decoded,err:=agent.resource_generation_decode(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer agent.resource_generation_destroy(&decoded)
        return resource_generation_execute(app,decoded.request)
    case "material":
        decoded,err:=agent.decode_material(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer agent.decoded_material_destroy(&decoded)
        if _,mutation:=decoded.operation.(agent.Material_Set); mutation { if error:=authoring_before_mutation(app); error!=.None { return error_result(&app.world,error),{} } }
        return material_execute(app,decoded.operation)
    case "material_asset":
        decoded,err:=asset.material_asset_decode(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer asset.material_asset_destroy(&decoded)
        if decoded.request.action in (bit_set[asset.Material_Asset_Action]{.Apply,.Capture,.Write}) { if error:=authoring_before_mutation(app); error!=.None { return error_result(&app.world,error),{} } }
        return material_asset_execute(app,decoded.request)
    case "animation":
        decoded,err:=scene.decode_animation(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer scene.decoded_animation_destroy(&decoded)
        if decoded.operation.action!=.Inspect { if error:=authoring_before_mutation(app); error!=.None { return error_result(&app.world,error),{} } }
        return animation_execute(app,decoded.operation)
    case "simulation":
        decoded,err:=scene.decode_simulation(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }
        if decoded!=.Inspect { if error:=authoring_before_mutation(app); error!=.None { return error_result(&app.world,error),{} } }
        return simulation_execute(app,decoded)
    case "behavior":
        decoded,err:=scene.decode_behavior(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer scene.decoded_behavior_destroy(&decoded)
        if decoded.operation.action not_in (bit_set[scene.Behavior_Action]{.Describe,.Inspect}) { if error:=authoring_before_mutation(app); error!=.None { return error_result(&app.world,error),{} } }
        return behavior_execute(app,decoded.operation)
    case "trigger":
        decoded,err:=scene.decode_trigger(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer scene.decoded_trigger_destroy(&decoded)
        if decoded.operation.action!=.Inspect { if error:=authoring_before_mutation(app); error!=.None { return error_result(&app.world,error),{} } }
        return trigger_execute(app,decoded.operation)
    case "prefab":
        decoded,err:=asset.prefab_decode(op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer asset.prefab_destroy(&decoded)
        if decoded.request.action in (bit_set[asset.Prefab_Action]{.Instantiate,.Capture,.Remove}) { if error:=authoring_before_mutation(app); error!=.None { return error_result(&app.world,error),{} } }
        return asset_authoring_execute(app,decoded.request)
    case "load_scene","save_scene":
        decoded,err:=asset.scene_file_decode(op.tool_name,op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer asset.scene_file_destroy(&decoded)
        if error:=authoring_before_mutation(app); error!=.None { return error_result(&app.world,error),{} }
        return scene_file_execute(app,decoded.request)
    case "search_assets","list_resources","read_resource":
        decoded,err:=asset.decode(op.tool_name,op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer asset.destroy(&decoded)
        return asset_execute(app,decoded.request)
    case "create_resource","write_resource":
        decoded,err:=asset.resource_write_decode(op.tool_name,op.value,allocator); if err!=.None { return error_result(&app.world,.Invalid_Operation),{} }; defer asset.resource_write_destroy(&decoded)
        return resource_write_execute(app,decoded.request)
    }
    return error_result(&app.world,.Application_Owned),{}
}
