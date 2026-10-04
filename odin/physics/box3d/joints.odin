//! Native constraint ownership is published only after complete body and joint admission.
package box3d
import "core:math"

Joint_Kind :: enum u32 { PointToPoint, Hinge, Distance, Fixed }
/// Endpoints are lossless scene identities; anchors stay in their bodies' local coordinates.
Joint :: struct { id,a,b:u64,kind:Joint_Kind,has_limits:u32,anchor_a,anchor_b:[3]f32,limits:[2]f32 }
#assert(size_of(Joint)==64)
@(private="package")
Joint_Entry :: struct { joint:Joint,native,a_native,b_native:rawptr }
@(private="package")
joint_valid :: proc(joint:Joint,bodies:map[u64]Body)->Error {
    if joint.kind>Joint_Kind.Fixed || joint.has_limits>1 || joint.a==joint.b { return .Invalid }
    for entity in ([2]u64{joint.a,joint.b}) { body,present:=bodies[entity]; if !present || body.body_type==.Fixed || body.shape_kind==.None { return .Invalid } }
    for vector in ([2][3]f32{joint.anchor_a,joint.anchor_b}) { for value in vector { if !finite(value) || abs(value)>1e8 { return .Invalid } } }
    if joint.has_limits!=0 { if !finite(joint.limits[0]) || !finite(joint.limits[1]) || abs(joint.limits[0])>1e8 || abs(joint.limits[1])>1e8 || joint.limits[0]>joint.limits[1] { return .Invalid } }
    if joint.kind==.Hinge && joint.has_limits!=0 && (joint.limits[0] < -0.99*f32(math.PI) || joint.limits[1]>0.99*f32(math.PI)) { return .Unsupported }
    if joint.kind==.Distance { rest:=f32(0.5); if joint.has_limits!=0 { rest=(joint.limits[0]+joint.limits[1])*0.5 }; if rest<0.005 { return .Unsupported } }
    return .None
}
