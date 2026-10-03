// Numeric consumer paired with benchmarks/math_port/rust by the validation script.
package main

import "core:fmt"
import km "../math"

emit :: proc(name: string, values: [$N]f32) {
    fmt.print(name)
    for v in values { fmt.printf(" %.9e",v) }
    fmt.println()
}
flatten3 :: proc(a: km.Mat3) -> [9]f32 {
    values: [9]f32
    for c in 0..<3 { for r in 0..<3 { values[c*3+r] = a[c][r] } }
    return values
}
main :: proc() {
    for i in 0..<128 {
        f := f32(i)*0.037
        axis := km.Vec3{f32(i%7)+1,f32(i%5)-2,f32(i%3)+0.5}
        q := km.quat_axis_angle(axis,f-2)
        qb := km.quat_axis_angle(km.VEC3_Y,0.3-f*0.2)
        a,b := km.Vec3{1+f,-2+f*0.1,3-f*0.2},km.Vec3{-2,3+f,1}
        v2,v4 := km.Vec2{a[0],a[1]},km.Vec4{a[0],a[1],a[2],0.5}
        emit("vector_scalar",[7]f32{km.dot(a,b),km.length(a),km.distance(a,b),km.angle_between(a,b),km.cross2(v2,km.Vec2{3,4}),km.angle(v2),km.length(v4)})
        emit("normalize",km.normalize(a))
        emit("cross",km.cross(a,b))
        emit("reflect",km.reflect(a,km.normalize(b)))
        emit("project",km.project(a,b))
        emit("rotate",km.quat_transform_vector(q,a))
        emit("rotate_method",km.quat_rotate_vec3(q,a))
        emit("quat_matrix",flatten3(km.quat_to_mat3(q)))
        emit("quat_product",flatten3(km.quat_to_mat3(km.quat_mul(q,qb))))
        emit("slerp",flatten3(km.quat_to_mat3(km.quat_slerp(q,qb,0.37))))
        emit("nlerp",flatten3(km.quat_to_mat3(km.quat_nlerp(q,qb,0.37))))
        scale := km.Vec3{1.5+f*0.1,2,0.75}
        a4 := km.mat4_trs(a,q,scale)
        emit("trs",km.mat4_to_array(a4))
        emit("matrix_point",km.transform_point(a4,b))
        inv,ok := km.inverse(a4); assert(ok)
        emit("matrix_inverse",km.mat4_to_array(inv))
        emit("matrix_det",[1]f32{km.determinant(a4)})
        emit("matrix_product",km.mat4_to_array(km.matrix_mul(a4,km.mat4_translation(km.Vec3{1,2,3}))))
        a2 := km.mat2(1+f,2,3,5)
        inv2,ok2 := km.inverse(a2)
        if ok2 { emit("mat2_inverse",[4]f32{inv2[0][0],inv2[0][1],inv2[1][0],inv2[1][1]}) }
        inv3,ok3 := km.inverse(km.mat4_to_mat3(a4)); assert(ok3)
        emit("mat3_inverse",flatten3(inv3))
        p := km.mat4_reverse_z(60+f,1.7,0.1)
        emit("reverse_z",km.mat4_to_array(p))
        emit("perspective",km.mat4_to_array(km.mat4_perspective(60+f,1.7,0.1,100)))
        look := km.mat4_lookat(a,b,km.VEC3_Y)
        emit("lookat",km.mat4_to_array(look))
        fr := km.frustum_from_proj_and_lookat(p,look)
        emit("frustum_near",[4]f32{fr.near.normal[0],fr.near.normal[1],fr.near.normal[2],fr.near.distance})
        box := km.aabb_from_min_max(km.Vec3{-1,-2,-3},km.Vec3{2,3,4})
        bounds := km.aabb_transform(box,a4)
        emit("bounds",[6]f32{bounds.center[0],bounds.center[1],bounds.center[2],bounds.extent[0],bounds.extent[1],bounds.extent[2]})
        plane := km.plane_from_point_normal(a,b)
        transformed := km.plane_transform(plane,a4)
        emit("plane",[5]f32{km.plane_distance(plane,b),transformed.normal[0],transformed.normal[1],transformed.normal[2],transformed.distance})
        ray := km.ray_from_points(km.Vec3{0,0,4+f},km.VEC3_ZERO)
        hit,hits := km.ray_intersects_sphere(ray,km.Sphere{km.VEC3_ZERO,1}); assert(hits)
        emit("ray_sphere",[7]f32{hit.distance,hit.point[0],hit.point[1],hit.point[2],hit.normal[0],hit.normal[1],hit.normal[2]})
        color := km.Color{f32(i%11)/11,f32(i%13)/13,f32(i%17)/17,0.4}
        emit("linear",km.color_to_array(km.color_to_linear(color)))
        emit("gamma_roundtrip",km.color_to_array(km.color_to_srgb(km.color_to_linear(color))))
        hsv := km.color_to_hsv(color)
        emit("hsv",[3]f32{hsv.h,hsv.s,hsv.v})
        emit("hsv_roundtrip",km.color_to_array(km.color_from_hsv(hsv)))
    }
}
