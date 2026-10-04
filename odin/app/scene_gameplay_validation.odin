//! Complete staged gameplay references are admitted before scene publication or native preparation.
package app
import scene "../agent/scene"
import ecs "../ecs"
import editor "../editor"

/// Validates staged rule participants and explicit recipients after every document reference is bound.
scene_gameplay_validate_entities :: proc(app:^Authoring,entities:[]ecs.Entity_Id)->editor.Scene_Error {
    for entity in entities {
        if joint,present:=ecs.get_component(&app.world,entity,Physics_Joint); present && !physics_joint_participants_valid(app,joint) { return .Invalid_Operation }
        if body,present:=ecs.get_component(&app.world,entity,Physics_Body); present && !physics_body_valid(body) { return .Invalid_Field_Value }
        if emitter,present:=ecs.get_component(&app.world,entity,Particle_Emitter); present && !particle_descriptor_valid(emitter.descriptor) { return .Invalid_Field_Value }
        rules,has_rules:=ecs.get_component(&app.world,entity,Trigger_Rules)
        if !has_rules || len(rules.rules)==0 { continue }
        body,has_body:=ecs.get_component(&app.world,entity,Physics_Body); _,has_volume:=ecs.get_component(&app.world,entity,Trigger_Volume)
        if !has_body || !body.has_rigid_body || !body.has_collider || !body.sensor || !has_volume { return .Component_Not_Found }
        if !scene_trigger_descriptors_valid(app,rules.rules,entity) { return .Invalid_Operation }
    }
    return .None
}

@(private="package")
scene_trigger_descriptors_valid :: proc(app:^Authoring,rules:[]scene.Trigger_Rule,trigger:ecs.Entity_Id)->bool {
    if !scene.trigger_rules_valid(rules) { return false }
    for rule in rules {
        if rule.has_other && !ecs.entity_exists(&app.world,rule.other) { return false }
        for action in rule.actions {
            if action.kind==.Emit { continue }
            if action.target.kind==.Entity && !ecs.entity_exists(&app.world,action.target.entity) { return false }
            if action.target.kind==.Other { continue }
            if action.kind==.Burst_Particles || action.kind==.Set_Particles_Active {
                recipient:=trigger if action.target.kind==.Trigger else action.target.entity
                if _,present:=ecs.get_component(&app.world,recipient,Particle_Emitter); !present { return false }
            }
        }
    }
    return true
}
