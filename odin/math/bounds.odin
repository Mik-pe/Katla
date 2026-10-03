// Axis-aligned bounds, spheres, and rectangles.
package katla_math

/// Center and half extent representation.
AABB :: struct { center, extent: Vec3 }
Sphere :: struct { center: Vec3, radius: f32 }
Rect2D :: struct { min, max: Vec2 }

/// Component-wise vertex extrema. Empty input retains Rust's sentinel extrema.
compute_bounds :: proc(verts: []Vec3) -> (minimum, maximum: Vec3) {
    minimum = {3.402823466e38,3.402823466e38,3.402823466e38}
    maximum = -minimum
    for v in verts { for i in 0..<3 { if v[i] < minimum[i] { minimum[i] = v[i] }; if v[i] > maximum[i] { maximum[i] = v[i] } } }
    return
}
/// Construct from extrema without sorting or validating them.
aabb_from_min_max :: #force_inline proc(minimum,maximum: Vec3) -> AABB { extent := (maximum-minimum)*0.5; return {minimum+extent,extent} }
/// Construct bounds from scalar vertices.
aabb_from_verts :: proc(verts: []Vec3) -> AABB { minimum,maximum := compute_bounds(verts); return aabb_from_min_max(minimum,maximum) }
/// Minimum corner.
aabb_min :: #force_inline proc(a: AABB) -> Vec3 { return a.center-a.extent }
/// Maximum corner.
aabb_max :: #force_inline proc(a: AABB) -> Vec3 { return a.center+a.extent }
/// Touching boxes intersect.
aabb_intersects :: #force_inline proc(a,b: AABB) -> bool {
    for i in 0..<3 { if abs(a.center[i]-b.center[i]) > a.extent[i]+b.extent[i] { return false } }
    return true
}
/// Closest clamped point.
aabb_closest_point :: #force_inline proc(a: AABB,v: Vec3) -> Vec3 {
    result: Vec3
    lo,hi := aabb_min(a),aabb_max(a)
    for i in 0..<3 { result[i] = clamp(v[i],lo[i],hi[i]) }
    return result
}
/// Sphere/AABB overlap includes tangency.
aabb_intersects_sphere :: #force_inline proc(a: AABB,s: Sphere) -> bool { return distance_squared(s.center,aabb_closest_point(a,s.center)) <= s.radius*s.radius }
/// Smallest AABB enclosing both inputs.
aabb_merge :: #force_inline proc(a,b: AABB) -> AABB {
    lo,hi := aabb_min(a),aabb_max(a)
    blo,bhi := aabb_min(b),aabb_max(b)
    for i in 0..<3 { lo[i] = min(lo[i],blo[i]); hi[i] = max(hi[i],bhi[i]) }
    return aabb_from_min_max(lo,hi)
}
/// Transform all eight corners; preserves affine shear and negative scale.
aabb_transform :: proc(a: AABB,m: Mat4) -> AABB {
    lo,hi := aabb_min(a),aabb_max(a)
    corners: [8]Vec3
    for mask in 0..<8 {
        v := lo
        for i in 0..<3 { if mask & (1 << uint(i)) != 0 { v[i] = hi[i] } }
        corners[mask] = transform_point(m,v)
    }
    return aabb_from_verts(corners[:])
}
/// Includes the Rust 1e-5 radius tolerance.
sphere_point_inside :: #force_inline proc(s: Sphere,v: Vec3) -> bool { return distance_squared(s.center,v) <= (s.radius+1e-5)*(s.radius+1e-5) }
/// Increase radius at fixed center only when the point is outside the tolerance.
sphere_maybe_expand :: #force_inline proc(s: ^Sphere,v: Vec3) { if !sphere_point_inside(s^,v) { s.radius = distance(s.center,v) } }
/// Touching spheres intersect.
sphere_intersects :: #force_inline proc(a,b: Sphere) -> bool { return distance_squared(a.center,b.center) <= (a.radius+b.radius)*(a.radius+b.radius) }
/// Rust's AABB-midpoint sphere, not a minimum enclosing sphere.
sphere_from_verts :: proc(verts: []Vec3) -> Sphere { a := aabb_from_verts(verts); return {a.center,length(a.extent)} }
/// Construct from origin and full size.
rect_from_origin_size :: #force_inline proc(origin,size: Vec2) -> Rect2D { return {origin,origin+size} }
/// Construct from center and half extents.
rect_from_center_half_extents :: #force_inline proc(center,extent: Vec2) -> Rect2D { return {center-extent,center+extent} }
/// Construct from center and full size.
rect_from_center_size :: #force_inline proc(center,size: Vec2) -> Rect2D { return rect_from_center_half_extents(center,size*0.5) }
/// Full extent, which can be negative for invalid rectangles.
rect_size :: #force_inline proc(r: Rect2D) -> Vec2 { return r.max-r.min }
/// Midpoint.
rect_center :: #force_inline proc(r: Rect2D) -> Vec2 { return (r.min+r.max)*0.5 }
/// Half extent.
rect_half_extents :: #force_inline proc(r: Rect2D) -> Vec2 { return rect_size(r)*0.5 }
/// Offset both corners.
rect_translate :: #force_inline proc(r: Rect2D,offset: Vec2) -> Rect2D { return {r.min+offset,r.max+offset} }
/// Point containment includes edges.
rect_contains :: #force_inline proc(r: Rect2D,p: Vec2) -> bool { return p[0]>=r.min[0] && p[0]<=r.max[0] && p[1]>=r.min[1] && p[1]<=r.max[1] }
/// Contains both opposing corners.
rect_contains_rect :: #force_inline proc(r,other: Rect2D) -> bool { return rect_contains(r,other.min) && rect_contains(r,other.max) }
/// Strict overlap excludes merely touching edges.
rect_overlaps :: #force_inline proc(a,b: Rect2D) -> bool { return a.min[0]<b.max[0] && a.max[0]>b.min[0] && a.min[1]<b.max[1] && a.max[1]>b.min[1] }
/// Expand the existing rectangle to include a point.
rect_expand :: #force_inline proc(r: ^Rect2D,p: Vec2) { for i in 0..<2 { r.min[i] = min(r.min[i],p[i]); r.max[i] = max(r.max[i],p[i]) } }
/// Expand to include both corners.
rect_expand_rect :: #force_inline proc(r: ^Rect2D,other: Rect2D) { rect_expand(r,other.min); rect_expand(r,other.max) }
/// Width times height, without clamping.
rect_area :: #force_inline proc(r: Rect2D) -> f32 { s := rect_size(r); return s[0]*s[1] }
/// Twice width plus height.
rect_perimeter :: #force_inline proc(r: Rect2D) -> f32 { s := rect_size(r); return 2*(s[0]+s[1]) }
/// Nonpositive width or height means empty.
rect_is_empty :: #force_inline proc(r: Rect2D) -> bool { s := rect_size(r); return s[0]<=0 || s[1]<=0 }
/// Positive amount expands; negative amount contracts.
rect_inflate :: #force_inline proc(r: Rect2D,amount: f32) -> Rect2D { return {r.min-amount,r.max+amount} }
/// Strict overlap intersection, ok=false at touching edges.
rect_intersection :: proc(a,b: Rect2D) -> (Rect2D,bool) {
    r: Rect2D
    for i in 0..<2 { r.min[i] = max(a.min[i],b.min[i]); r.max[i] = min(a.max[i],b.max[i]) }
    if rect_is_empty(r) { return {},false }
    return r,true
}
/// Smallest enclosing rectangle.
rect_union :: #force_inline proc(a,b: Rect2D) -> Rect2D { r := a; rect_expand_rect(&r,b); return r }
/// Clamp component-wise, following Rust max/min order even for invalid bounds.
rect_clamp :: #force_inline proc(r: Rect2D,p: Vec2) -> Vec2 { return {min(max(p[0],r.min[0]),r.max[0]),min(max(p[1],r.min[1]),r.max[1])} }
/// Bottom-left, bottom-right, top-left, top-right.
rect_corners :: #force_inline proc(r: Rect2D) -> [4]Vec2 { return {r.min,{r.max[0],r.min[1]},{r.min[0],r.max[1]},r.max} }
/// Position and full extent for clipping.
rect_to_clip_array :: #force_inline proc(r: Rect2D) -> [4]f32 { s := rect_size(r); return {r.min[0],r.min[1],s[0],s[1]} }
/// Unclamped corner interpolation.
rect_lerp :: #force_inline proc(a,b: Rect2D,t: f32) -> Rect2D { return {lerp(a.min,b.min,t),lerp(a.max,b.max,t)} }
