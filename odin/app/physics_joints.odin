//! Joint descriptions retain scene identity while the selected native backend owns constraints.
package app

import ecs "../ecs"
import editor "../editor"

Physics_Joint_Kind :: enum { PointToPoint, Hinge, Distance, Fixed }
/// Local anchors and optional limits carry no dependency-native handles.
Physics_Joint :: struct {
    kind:Physics_Joint_Kind `inspect:"skip"`,a,b:ecs.Entity_Id `inspect:"skip"`,
    anchor_a,anchor_b:[3]f32 `inspect:"skip"`,has_limits:bool `inspect:"skip"`,limits:[2]f32 `inspect:"skip"`,
}
/// Resolves the joint entity separately from its two mapped participants.
Physics_Resolved_Joint :: struct { id:u64,joint:Physics_Joint }
/// Registers typed references so scene staging and shared undo map both endpoints explicitly.
physics_joints_register :: proc(app:^Authoring) { editor.editor_register(&app.world,&app.registry,"PhysicsJoint",Physics_Joint{},spawn_default=false) }
/// Checks finite local anchors and ordered limits before any dependency allocation.
physics_joint_valid :: proc(joint:Physics_Joint)->bool {
    if joint.kind not_in (bit_set[Physics_Joint_Kind]{.PointToPoint,.Hinge,.Distance,.Fixed}) || joint.a==joint.b { return false }
    for vector in ([2][3]f32{joint.anchor_a,joint.anchor_b}) { for value in vector { if !finite_nonnegative(abs(value)) { return false } } }
    if joint.has_limits { if !finite_nonnegative(abs(joint.limits[0])) || !finite_nonnegative(abs(joint.limits[1])) || joint.limits[0]>joint.limits[1] { return false } }
    return true
}
/// Checks genuine participant ownership after all source scene keys have become live generations.
physics_joint_participants_valid :: proc(app:^Authoring,joint:Physics_Joint)->bool {
    if !physics_joint_valid(joint) { return false }
    for entity in ([2]ecs.Entity_Id{joint.a,joint.b}) {
        if !ecs.entity_exists(&app.world,entity) { return false }
        body,present:=ecs.get_component(&app.world,entity,Physics_Body)
        if !present || !body.has_rigid_body || !body.has_collider || body.body_type==.Fixed || !physics_body_valid(body) { return false }
    }
    return true
}
/// Collects every constraint only after its full participant batch has been validated.
physics_collect_joints :: proc(app:^Authoring)->([]Physics_Resolved_Joint,editor.Scene_Error) {
    context.allocator=app.world.allocator; collected:=make([dynamic]Physics_Resolved_Joint,app.world.allocator); defer delete(collected)
    ids:=ecs.entity_ids(&app.world); defer delete(ids)
    for entity in ids {
        joint,present:=ecs.get_component(&app.world,entity,Physics_Joint); if !present { continue }
        if !physics_joint_participants_valid(app,joint) { return nil,.Invalid_Operation }
        if len(collected)>=100_000 { return nil,.Invalid_Operation }; append(&collected,Physics_Resolved_Joint{u64(entity),joint})
    }
    result:=make([]Physics_Resolved_Joint,len(collected),app.world.allocator); copy(result,collected[:]); return result,.None
}
