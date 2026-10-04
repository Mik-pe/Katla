//! Durable joint descriptors use document keys; common staging resolves both endpoints before publication.
package app
import editor "../editor"
import ecs "../ecs"
import "core:encoding/json"

@(private="package")
scene_gameplay_joint_decode :: proc(app:^Authoring,row:^Scene_Entity,value:json.Value)->editor.Scene_Error {
    object,is_object:=value.(json.Object)
    if !is_object || !recipe_keys(object,{"kind","a","b","anchor_a","anchor_b","limits"}) || !scene_gameplay_required(object,{"kind","a","b","anchor_a","anchor_b"}) { return .Decode_Failed }
    kind,payload,valid:=scene_variant(object["kind"]); if !valid || payload!=nil { return .Decode_Failed }
    joint:Physics_Joint
    switch kind {
    case "PointToPoint": joint.kind=.PointToPoint
    case "Hinge": joint.kind=.Hinge
    case "Distance": joint.kind=.Distance
    case "Fixed": joint.kind=.Fixed
    case: return .Invalid_Field_Value
    }
    a,a_valid:=scene_document_key(object["a"]); b,b_valid:=scene_document_key(object["b"])
    if !a_valid || !b_valid || a==0 || b==0 { return .Invalid_Field_Value }; joint.a=ecs.Entity_Id(a); joint.b=ecs.Entity_Id(b)
    if !scene_gameplay_vector(object,"anchor_a",&joint.anchor_a) || !scene_gameplay_vector(object,"anchor_b",&joint.anchor_b) { return .Decode_Failed }
    if limits,present:=scene_gameplay_present(object,"limits"); present { vector,vector_valid:=recipe_vector(limits,2); if !vector_valid { return .Decode_Failed }; joint.has_limits=true; copy(joint.limits[:],vector[:2]) }
    if !physics_joint_valid(joint) { return .Invalid_Field_Value }
    return scene_row_component(app,row,"PhysicsJoint",joint)
}
@(private="package")
scene_gameplay_joint_encode :: proc(joint:Physics_Joint)->(json.Value,editor.Scene_Error) {
    if !physics_joint_valid(joint) || joint.a==0 || joint.b==0 { return nil,.Invalid_Field_Value }
    kinds:=[4]string{"PointToPoint","Hinge","Distance","Fixed"}; limits:json.Value=json.Null{}
    if joint.has_limits { limits=trigger_json_value(joint.limits) }; defer json.destroy_value(limits)
    a:=scene_document_key_value(u64(joint.a)); b:=scene_document_key_value(u64(joint.b)); defer json.destroy_value(a); defer json.destroy_value(b)
    value:=trigger_json_value(struct {kind:string,a,b:json.Value,anchor_a,anchor_b:[3]f32,limits:json.Value}{kinds[joint.kind],a,b,joint.anchor_a,joint.anchor_b,limits})
    if value==nil { return nil,.Decode_Failed }; return value,.None
}
