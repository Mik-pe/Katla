// Plane and ray intersections. Optional results are returned as (value, ok).
package katla_math

import m "core:math"

Plane :: struct { normal: Vec3, distance: f32 }
Plane_Side :: enum { Front, Back, Intersecting }
Ray :: struct { origin, direction: Vec3 }
Ray_Intersection :: struct { point: Vec3, distance: f32, normal: Vec3 }

/// Plane equation dot(normal,point) = distance, normal normalized here.
plane_from_point_normal :: #force_inline proc(point,normal: Vec3) -> Plane { n := normalize(normal); return {n,dot(n,point)} }
/// Counter-clockwise vertex winding determines the normal.
plane_from_points :: #force_inline proc(a,b,c: Vec3) -> Plane { return plane_from_point_normal(a,cross(b-a,c-a)) }
/// Signed distance for a normalized plane.
plane_distance :: #force_inline proc(p: Plane,v: Vec3) -> f32 { return dot(p.normal,v)-p.distance }
/// Strict tolerance test.
plane_contains_point :: #force_inline proc(p: Plane,v: Vec3,tolerance: f32) -> bool { return abs(plane_distance(p,v)) < tolerance }
/// Closest point for a normalized plane.
plane_closest_point :: #force_inline proc(p: Plane,v: Vec3) -> Vec3 { return v-p.normal*plane_distance(p,v) }
/// Rust's 1e-5 classification tolerance.
plane_which_side :: #force_inline proc(p: Plane,v: Vec3) -> Plane_Side {
    d := plane_distance(p,v)
    if d > 1e-5 { return .Front }
    if d < -1e-5 { return .Back }
    return .Intersecting
}
/// Normalize normal and distance together, leaving a zero normal unchanged.
plane_normalize :: #force_inline proc(p: Plane) -> Plane { n := length(p.normal); if n > 0 { return {p.normal/n,p.distance/n} }; return p }
/// Reverse both equation coefficients.
plane_flip :: #force_inline proc(p: Plane) -> Plane { return {-p.normal,-p.distance} }
/// Intersection with a centered AABB includes touching.
plane_intersects_aabb :: #force_inline proc(p: Plane,a: AABB) -> bool {
    radius: f32
    for i in 0..<3 { radius += abs(p.normal[i])*a.extent[i] }
    return abs(plane_distance(p,a.center)) <= radius
}
/// Intersection includes tangent spheres.
plane_intersects_sphere :: #force_inline proc(p: Plane,s: Sphere) -> bool { return abs(plane_distance(p,s.center)) <= s.radius }
/// Forward intersection; parallel and backward hits return ok=false.
plane_intersects_ray :: proc(p: Plane,r: Ray) -> (f32,bool) {
    d := dot(p.normal,r.direction)
    if abs(d) < 1e-6 { return 0,false }
    t := (p.distance-dot(p.normal,r.origin))/d
    if t < 0 { return 0,false }
    return t,true
}
/// Inverse-transpose normal transformation; singular matrices retain input normal.
plane_transform :: proc(p: Plane,a: Mat4) -> Plane {
    point := transform_point(a,p.normal*p.distance)
    inv,ok := inverse(mat4_to_mat3(a))
    normal := p.normal
    if ok { normal = matrix_vector(transpose(inv),normal) }
    return plane_from_point_normal(point,normal)
}
/// Unit direction from start to end; coincident points produce zero direction.
ray_from_points :: #force_inline proc(start,end: Vec3) -> Ray { return {start,normalize(end-start)} }
/// Direction should be normalized when distance is intended as world units.
ray_at :: #force_inline proc(r: Ray,t: f32) -> Vec3 { return r.origin+r.direction*t }
/// Closest distance along the forward ray, assuming a unit direction.
ray_distance_to_point :: #force_inline proc(r: Ray,p: Vec3) -> f32 { return distance(p,ray_at(r,max(0,dot(p-r.origin,r.direction)))) }
/// Forward hit point.
ray_intersects_plane :: proc(r: Ray,p: Plane) -> (Vec3,bool) { t,ok := plane_intersects_ray(p,r); if !ok { return {},false }; return ray_at(r,t),true }
/// Slab intersection with outward normals; inside rays report the forward exit face.
ray_intersects_aabb :: proc(r: Ray,a: AABB) -> (Ray_Intersection,bool) {
    if is_zero(r.direction) { return {},false }
    t_min,t_max := m.NEG_INF_F32,m.INF_F32
    hit_axis := 0
    hit_sign: f32 = -1
    exit_axis := 0
    exit_sign: f32 = 1
    for i in 0..<3 {
        if r.direction[i] == 0 {
            if r.origin[i] < a.center[i]-a.extent[i] || r.origin[i] > a.center[i]+a.extent[i] { return {},false }
            continue
        }
        inv_d := 1/r.direction[i]
        t1 := (a.center[i]-a.extent[i]-r.origin[i])*inv_d
        t2 := (a.center[i]+a.extent[i]-r.origin[i])*inv_d
        tn,tf,sign := t2,t1,f32(1)
        if t1 < t2 { tn,tf,sign = t1,t2,-1 }
        if tn > t_min { t_min,hit_axis,hit_sign = tn,i,sign }
        if tf < t_max { t_max,exit_axis,exit_sign = tf,i,-sign }
        if t_min > t_max { return {},false }
    }
    normal: Vec3
    normal[hit_axis] = hit_sign
    if t_min < 0 {
        if t_max < 0 { return {},false }
        normal = {}; normal[exit_axis] = exit_sign
        return {ray_at(r,t_max),t_max,normal},true
    }
    return {ray_at(r,t_min),t_min,normal},true
}
/// First strictly positive sphere root; zero direction returns ok=false.
ray_intersects_sphere :: proc(r: Ray,s: Sphere) -> (Ray_Intersection,bool) {
    oc := r.origin-s.center
    a,b,c := dot(r.direction,r.direction),2*dot(oc,r.direction),dot(oc,oc)-s.radius*s.radius
    if a == 0 { return {},false }
    disc := b*b-4*a*c
    if disc < 0 { return {},false }
    t1,t2 := (-b-m.sqrt(disc))/(2*a),(-b+m.sqrt(disc))/(2*a)
    t := t1
    if t1 <= 0 { t = t2; if t2 <= 0 { return {},false } }
    point := ray_at(r,t)
    return {point,t,normalize(point-s.center)},true
}
/// Two-sided Moller-Trumbore intersection; degenerate/parallel and backward misses.
ray_intersects_triangle :: proc(r: Ray,a,b,c: Vec3) -> (Vec3,bool) {
    e1,e2 := b-a,c-a
    h := cross(r.direction,e2)
    det := dot(e1,h)
    if abs(det) < 1e-6 { return {},false }
    inv_det := 1/det
    s := r.origin-a
    u := inv_det*dot(s,h)
    if u < 0 || u > 1 { return {},false }
    q := cross(s,e1)
    v := inv_det*dot(r.direction,q)
    if v < 0 || u+v > 1 { return {},false }
    t := inv_det*dot(e2,q)
    if t <= 1e-6 { return {},false }
    return ray_at(r,t),true
}
/// Transform origin affinely and normalize transformed direction.
ray_transform :: #force_inline proc(r: Ray,a: Mat4) -> Ray { return {transform_point(a,r.origin),normalize(transform_direction(a,r.direction))} }
