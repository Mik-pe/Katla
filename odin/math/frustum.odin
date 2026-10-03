// Infinite reverse-Z frustum planes follow the Rust camera-to-world convention.
package katla_math

import m "core:math"

Frustum :: struct { left,right,top,bottom,near,far: Plane }

@(private)
frustum_plane :: proc(v: Vec4) -> Plane { n := xyz(v); len := length(n); return {n/len,-v[3]/len} }
@(private)
frustum_extract :: proc(proj,view,lookat: Mat4) -> Frustum {
    a := matrix_mul(proj,view)
    r0,r1,r3 := extract_row(a,0),extract_row(a,1),extract_row(a,3)
    pos,forward := xyz(lookat[3]),normalize(-xyz(lookat[2]))
    return {frustum_plane(r3+r0),frustum_plane(r3-r0),frustum_plane(r3-r1),frustum_plane(r3+r1),{forward,dot(pos,forward)+proj[3][2]},{-forward,m.NEG_INF_F32}}
}
/// Camera-to-world input, as returned by mat4_lookat; singular inverse uses identity.
frustum_from_proj_and_lookat :: proc(proj,lookat: Mat4) -> Frustum {
    view,ok := inverse(lookat)
    if !ok { view = identity(Mat4) }
    return frustum_extract(proj,view,lookat)
}
/// World-to-camera input; only infinite reverse-Z projections have this contract.
frustum_from_proj_and_view :: proc(proj,view: Mat4) -> Frustum {
    lookat,ok := inverse(view)
    if !ok { lookat = identity(Mat4) }
    return frustum_extract(proj,view,lookat)
}
/// Construct infinite reverse-Z frustum with FOV in degrees.
frustum_from_camera :: proc(position,target,up: Vec3,fov_degrees,aspect,near: f32) -> Frustum { return frustum_from_proj_and_lookat(mat4_reverse_z(fov_degrees,aspect,near),mat4_lookat(position,target,up)) }
@(private)
frustum_planes :: #force_inline proc(f: Frustum) -> [6]Plane { return {f.left,f.right,f.top,f.bottom,f.near,f.far} }
/// All inward half-spaces must contain the point; far plane is disabled by -infinity.
frustum_contains_point :: #force_inline proc(f: Frustum,v: Vec3) -> bool { for p in frustum_planes(f) { if !(plane_distance(p,v) >= 0) { return false } }; return true }
/// Positive support point outside any plane rejects the AABB.
frustum_intersects_aabb :: #force_inline proc(f: Frustum,a: AABB) -> bool {
    for p in frustum_planes(f) {
        v := a.center
        for i in 0..<3 { if p.normal[i] >= 0 { v[i] += a.extent[i] } else { v[i] -= a.extent[i] } }
        if plane_distance(p,v) < 0 { return false }
    }
    return true
}
/// Negative support points must lie inside all planes.
frustum_contains_aabb :: #force_inline proc(f: Frustum,a: AABB) -> bool {
    for p in frustum_planes(f) {
        v := a.center
        for i in 0..<3 { if p.normal[i] >= 0 { v[i] -= a.extent[i] } else { v[i] += a.extent[i] } }
        if plane_distance(p,v) < 0 { return false }
    }
    return true
}
/// Plane/sphere rejection includes tangency.
frustum_intersects_sphere :: #force_inline proc(f: Frustum,s: Sphere) -> bool { for p in frustum_planes(f) { if plane_distance(p,s.center) < -s.radius { return false } }; return true }
@(private)
intersect_three_planes :: proc(a,b,c: Plane) -> (Vec3,bool) {
    denom := dot(a.normal,cross(b.normal,c.normal))
    if abs(denom) < 1e-6 { return {},false }
    return (a.distance*cross(b.normal,c.normal)+b.distance*cross(c.normal,a.normal)+c.distance*cross(a.normal,b.normal))/denom,true
}
/// Finite visualization corners at far_distance units forward from the near plane.
/// Degenerate triples use the origin, matching the reference; default distance is 1000.
frustum_corners :: proc(f: Frustum,far_distance: f32 = 1000) -> [8]Vec3 {
    far := Plane{f.near.normal,f.near.distance+far_distance}
    result: [8]Vec3
    pairs := [4][2]Plane{{f.left,f.top},{f.right,f.top},{f.left,f.bottom},{f.right,f.bottom}}
    for pair,i in pairs {
        result[i],_ = intersect_three_planes(pair[0],pair[1],f.near)
        result[i+4],_ = intersect_three_planes(pair[0],pair[1],far)
    }
    return result
}
/// Average of visualization corners at a finite distance.
frustum_center :: proc(f: Frustum,far_distance: f32 = 1000) -> Vec3 { sum: Vec3; for v in frustum_corners(f,far_distance) { sum += v }; return sum/8 }
/// Sphere enclosing all eight visualization corners.
frustum_bounding_sphere :: proc(f: Frustum,far_distance: f32 = 1000) -> Sphere {
    center := frustum_center(f,far_distance)
    radius: f32
    for v in frustum_corners(f,far_distance) { radius = max(radius,distance(v,center)) }
    return {center,radius}
}
