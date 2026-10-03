package katla_math

import "core:testing"
import m "core:math"

@(private)
near :: proc(a,b: f32) -> bool { return abs(a-b) <= 2e-5+2e-5*abs(b) }
@(private)
expect_vector :: proc(t: ^testing.T,a,b: [$N]f32) { for i in 0..<N { testing.expect(t,near(a[i],b[i])) } }
@(private)
expect_matrix :: proc(t: ^testing.T,a,b: [$N][N]f32) { for i in 0..<N { expect_vector(t,a[i],b[i]) } }

@(test)
test_vector_dimensions_and_arithmetic :: proc(t: ^testing.T) {
    expect_vector(t,normalize(Vec2{3,4}),Vec2{0.6,0.8})
    expect_vector(t,normalize(Vec3{2,3,6}),Vec3{2/f32(7),3/f32(7),6/f32(7)})
    testing.expect(t,near(length(Vec4{1,2,2,4}),5))
    testing.expect(t,near(dot(Vec4{1,2,3,4},Vec4{4,3,2,1}),20))
    testing.expect(t,is_zero(normalize(VEC3_ZERO)) && is_normalized(VEC3_X))
    testing.expect(t,length_squared(Vec2{3,4}) == 25)
    expect_vector(t,lerp(Vec2{1,2},Vec2{3,4},2),Vec2{5,6})
    testing.expect(t,distance(VEC3_ZERO,Vec3{3,4,0}) == 5 && distance_squared(VEC3_ZERO,Vec3{3,4,0}) == 25)
    expect_vector(t,xyz(vec4(Vec3{2,3,4},1)),Vec3{2,3,4})
}
@(test)
test_vector_geometry :: proc(t: ^testing.T) {
    expect_vector(t,cross(VEC3_X,VEC3_Y),VEC3_Z)
    testing.expect(t,cross2(VEC2_X,VEC2_Y) == 1)
    expect_vector(t,perpendicular(VEC2_X),VEC2_Y)
    expect_vector(t,from_angle(FRAC_PI_2),VEC2_Y)
    testing.expect(t,near(angle(VEC2_Y),FRAC_PI_2))
    expect_vector(t,reflect(Vec3{1,-2,3},VEC3_Y),Vec3{1,2,3})
    expect_vector(t,project(Vec3{1,2,3},VEC3_Y),Vec3{0,2,0})
    expect_vector(t,reject(Vec3{1,2,3},VEC3_Y),Vec3{1,0,3})
    testing.expect(t,near(angle_between(VEC3_X,VEC3_Y),FRAC_PI_2))
    expect_vector(t,from_spherical(FRAC_PI_2,FRAC_PI_2),VEC3_Z)
    expect_vector(t,clamp_length(Vec3{0,3,4},2),Vec3{0,1.2,1.6})
    expect_vector(t,clamp_length_min_max(VEC3_X,2,3),Vec3{2,0,0})
    testing.expect(t,is_zero(clamp_length(VEC3_X,-1)) && is_zero(clamp_length_min_max(VEC3_X,3,2)))
}
@(test)
test_matrix_column_product :: proc(t: ^testing.T) {
    a := mat2(1,2,3,4)
    testing.expect(t,a[0] == Vec2{1,3} && a[1] == Vec2{2,4})
    expect_vector(t,matrix_vector(a,Vec2{2,3}),Vec2{8,18})
    expect_matrix(t,matrix_mul(a,mat2(5,6,7,8)),mat2(19,22,43,50))
    expect_matrix(t,transpose(transpose(a)),a)
    expect_vector(t,extract_row(a,1),Vec2{3,4})
    testing.expect(t,near(mat2_to_rotation(mat2_rotation(0.4)),0.4))
    expect_vector(t,mat2_to_scale(scale_matrix(Vec2{2,3})),Vec2{2,3})
}
@(test)
test_matrix_inverse_all_dimensions :: proc(t: ^testing.T) {
    a := mat2(1,2,3,4)
    ai,ok := inverse(a)
    testing.expect(t,ok && determinant(a) == -2)
    expect_matrix(t,matrix_mul(a,ai),identity(Mat2))
    b := Mat3{{1,0,5},{2,1,6},{3,4,0}}
    bi,bok := inverse(b)
    testing.expect(t,bok && near(determinant(b),1))
    expect_matrix(t,matrix_mul(b,bi),identity(Mat3))
    c := Mat4{{1,2,0,1},{0,1,2,0},{2,0,3,1},{1,3,2,2}}
    ci,cok := inverse(c)
    testing.expect(t,cok)
    expect_matrix(t,matrix_mul(c,ci),identity(Mat4))
    _,s2 := inverse(Mat2{})
    _,s3 := inverse(Mat3{})
    _,s4 := inverse(Mat4{})
    testing.expect(t,!s2 && !s3 && !s4)
    _,tiny2 := inverse(scale_matrix(Vec2{1,EPSILON*0.5}))
    _,tiny3 := inverse(scale_matrix(Vec3{1,1,0.5e-6}))
    testing.expect(t,!tiny2 && !tiny3)
}
@(test)
test_matrix_affine_and_upload_layout :: proc(t: ^testing.T) {
    a := mat4_translation(Vec3{4,5,6})
    expect_vector(t,transform_point(a,Vec3{1,2,3}),Vec3{5,7,9})
    expect_vector(t,transform_direction(a,Vec3{1,2,3}),Vec3{1,2,3})
    flat := mat4_to_array(a)
    testing.expect(t,flat[12] == 4 && flat[13] == 5 && flat[14] == 6 && flat[15] == 1)
    expect_matrix(t,mat3_to_mat4(mat4_to_mat3(a)),identity(Mat4))
    expect_vector(t,mat4_extract_translation(a),Vec3{4,5,6})
    expect_vector(t,mat4_extract_scale(mat4_scale(Vec3{-2,3,4})),Vec3{2,3,4})
}
@(test)
test_reverse_z_and_finite_projection :: proc(t: ^testing.T) {
    rz := mat4_reverse_z(90,2,0.1)
    n := matrix_vector(rz,Vec4{0,0,-0.1,1})
    f := matrix_vector(rz,Vec4{0,0,-1000,1})
    testing.expect(t,near(n[2]/n[3],1) && near(f[2]/f[3],0.0001))
    testing.expect(t,rz[1][1] < 0 && rz[0][0] > 0)
    p := mat4_perspective(90,1,0.1,100)
    pn,pf := matrix_vector(p,Vec4{0,0,-0.1,1}),matrix_vector(p,Vec4{0,0,-100,1})
    testing.expect(t,near(pn[2]/pn[3],0) && near(pf[2]/pf[3],1))
}
@(test)
test_orthographic_projection :: proc(t: ^testing.T) {
    p := mat4_ortho(-2,6,-3,5,1,9)
    expect_vector(t,matrix_vector(p,Vec4{-2,-3,-1,1}),Vec4{-1,1,0,1})
    expect_vector(t,matrix_vector(p,Vec4{6,5,-9,1}),Vec4{1,-1,1,1})
}
@(test)
test_quaternion_and_matrix_agree :: proc(t: ^testing.T) {
    for i in 0..<80 {
        axis := Vec3{f32(i%7)+1,f32(i%5)-2,f32(i%3)+0.5}
        q := quat_axis_angle(axis,f32(i)*0.12-4)
        v := Vec3{2,-3,4}
        expect_vector(t,quat_transform_vector(q,v),matrix_vector(quat_to_mat3(q),v))
        expect_vector(t,quat_rotate_vec3(q,v),quat_transform_vector(q,v))
        expect_vector(t,transform_direction(mat4_rotaxis(f32(i)*0.12-4,axis),v),quat_transform_vector(q,v))
        testing.expect(t,near(abs(quat_dot(q,quat_from_mat3(quat_to_mat3(q)))),1))
        expect_matrix(t,matrix_mul(quat_to_mat4(q),quat_to_mat4(quat_conjugate(q))),identity(Mat4))
    }
    testing.expect(t,quat_axis_angle(VEC3_ZERO,PI) == QUAT_IDENTITY)
    testing.expect(t,quat_normalize(Quat{}) == Quat{})
}
@(test)
test_quaternion_composition_and_half_turns :: proc(t: ^testing.T) {
    for axis in ([3]Vec3{VEC3_X,VEC3_Y,VEC3_Z}) {
        q := quat_axis_angle(axis,PI)
        testing.expect(t,near(abs(quat_dot(quat_from_mat4(quat_to_mat4(q)),q)),1))
    }
    a,b := quat_axis_angle(VEC3_Y,0.7),quat_axis_angle(VEC3_X,-0.4)
    expect_matrix(t,quat_to_mat4(quat_mul(a,b)),matrix_mul(quat_to_mat4(a),quat_to_mat4(b)))
}
@(test)
test_quaternion_between_vectors :: proc(t: ^testing.T) {
    expect_vector(t,quat_transform_vector(quat_rotation_between(VEC3_X,VEC3_Y),VEC3_X),VEC3_Y)
    expect_vector(t,quat_transform_vector(quat_rotation_between(VEC3_X,-VEC3_X),VEC3_X),-VEC3_X)
    testing.expect(t,quat_rotation_between(VEC3_Z,VEC3_Z) == QUAT_IDENTITY)
}
@(test)
test_quaternion_interpolation :: proc(t: ^testing.T) {
    q := quat_axis_angle(VEC3_Y,FRAC_PI_2)
    half := quat_slerp(QUAT_IDENTITY,q,0.5)
    expect_vector(t,quat_transform_vector(half,VEC3_X),normalize(Vec3{1,0,-1}))
    testing.expect(t,near(abs(quat_dot(quat_nlerp(q,-q,0.4),q)),1))
    testing.expect(t,near(abs(quat_dot(quat_slerp(q,-q,0.6),q)),1))
}
@(test)
test_transform_matrix_agreement :: proc(t: ^testing.T) {
    tr := transform(Vec3{3,4,5},quat_axis_angle(VEC3_Y,0.6),Vec3{2,3,-4})
    v := Vec3{1,-2,3}
    expect_vector(t,transform_vector(tr,v),transform_point(transform_to_mat4(tr),v))
    inv,ok := transform_inverse(tr)
    testing.expect(t,ok)
    expect_vector(t,transform_point(inv,transform_vector(tr,v)),v)
    _,singular := transform_inverse(transform(scale=Vec3{0,1,1}))
    testing.expect(t,!singular && transform_is_identity(TRANSFORM_IDENTITY) && !transform_is_identity(tr))
    expect_vector(t,transform_forward(tr),quat_transform_vector(tr.rotation,-VEC3_Z))
    expect_vector(t,transform_up(tr),quat_transform_vector(tr.rotation,VEC3_Y))
    expect_vector(t,transform_right(tr),quat_transform_vector(tr.rotation,VEC3_X))
}
@(test)
test_transform_composition_retains_shear :: proc(t: ^testing.T) {
    parent := transform(Vec3{1,2,3},quat_axis_angle(VEC3_Y,0.4),Vec3{2,3,4})
    child := transform(Vec3{3,2,1},quat_axis_angle(VEC3_Z,0.6),Vec3{1,2,1})
    combined := transform_compose(parent,child)
    v := Vec3{2,3,-1}
    expect_vector(t,transform_point(combined,v),transform_vector(parent,transform_vector(child,v)))
    testing.expect(t,abs(dot(xyz(combined[0]),xyz(combined[1]))) > 0.1)
}
@(test)
test_transform_decomposition_and_lerp :: proc(t: ^testing.T) {
    tr := transform(Vec3{3,4,5},quat_axis_angle(VEC3_Y,0.6),Vec3{2,3,4})
    decomposed,ok := mat4_decompose_approx(transform_to_mat4(tr))
    testing.expect(t,ok)
    expect_matrix(t,transform_to_mat4(decomposed),transform_to_mat4(tr))
    _,singular := mat4_decompose_approx(mat4_scale(Vec3{0,1,1}))
    testing.expect(t,!singular)
    half := transform_lerp(TRANSFORM_IDENTITY,tr,0.5)
    expect_vector(t,half.position,tr.position*0.5)
    expect_vector(t,half.scale,Vec3{1.5,2,2.5})
}
@(test)
test_camera_rotation_and_look_at :: proc(t: ^testing.T) {
    pos,target := Vec3{4,5,6},Vec3{2,1,-3}
    look := mat4_lookat(pos,target,VEC3_Y)
    tr := transform_look_at(transform(position=pos,scale=Vec3{2,3,4}),target,VEC3_Y)
    expect_vector(t,transform_forward(tr),normalize(target-pos))
    expect_vector(t,tr.position,pos)
    expect_vector(t,tr.scale,Vec3{2,3,4})
    expect_matrix(t,quat_to_mat3(tr.rotation),mat4_to_mat3(look))
}
@(test)
test_bounds_and_spheres :: proc(t: ^testing.T) {
    vertices := [3]Vec3{{-2,1,3},{4,-3,5},{1,2,-4}}
    a := aabb_from_verts(vertices[:])
    expect_vector(t,aabb_min(a),Vec3{-2,-3,-4})
    expect_vector(t,aabb_max(a),Vec3{4,2,5})
    testing.expect(t,aabb_intersects(a,a) && aabb_intersects(a,AABB{Vec3{5,0,0},Vec3{1,1,1}}))
    testing.expect(t,!aabb_intersects(a,AABB{Vec3{6,0,0},Vec3{1,1,1}}))
    expect_vector(t,aabb_closest_point(a,Vec3{8,-8,8}),Vec3{4,-3,5})
    merged := aabb_merge(a,AABB{Vec3{9,0,0},Vec3{1,1,1}})
    testing.expect(t,aabb_max(merged)[0] == 10)
    s := sphere_from_verts(vertices[:])
    for v in vertices { testing.expect(t,sphere_point_inside(s,v)) }
    testing.expect(t,sphere_intersects(Sphere{{},1},Sphere{Vec3{2,0,0},1}))
    s = Sphere{{},1}; sphere_maybe_expand(&s,Vec3{3,0,0})
    testing.expect(t,s.radius == 3 && aabb_intersects_sphere(a,s))
}
@(test)
test_bounds_affine_shear :: proc(t: ^testing.T) {
    a := AABB{Vec3{1,2,3},Vec3{2,3,4}}
    affine :=  Mat4{{-2,0,0,0},{1,3,0,0},{0,0,4,0},{5,6,7,1}}
    b := aabb_transform(a,affine)
    expect_vector(t,b.center,transform_point(affine,a.center))
    expect_vector(t,b.extent,Vec3{7,9,16})
}
@(test)
test_rectangles_edges_and_operations :: proc(t: ^testing.T) {
    a := rect_from_origin_size(Vec2{1,2},Vec2{3,4})
    testing.expect(t,rect_contains(a,a.max) && rect_area(a) == 12 && rect_perimeter(a) == 14)
    b := rect_from_center_size(rect_center(a),rect_size(a))
    testing.expect(t,a == b && rect_half_extents(a) == Vec2{1.5,2})
    testing.expect(t,rect_contains_rect(a,a) && !rect_is_empty(a))
    touch := rect_translate(a,Vec2{3,0})
    _,ok := rect_intersection(a,touch)
    testing.expect(t,!ok && !rect_overlaps(a,touch))
    hit,hits := rect_intersection(a,rect_translate(a,Vec2{1,1}))
    testing.expect(t,hits && rect_area(hit) == 6)
    expect_vector(t,rect_clamp(a,Vec2{-10,10}),Vec2{1,6})
    testing.expect(t,rect_corners(a)[3] == a.max && rect_to_clip_array(a) == Vec4{1,2,3,4})
    testing.expect(t,rect_inflate(rect_inflate(a,1),-1) == a && rect_lerp(a,b,0.5) == a)
    expanded := a; rect_expand_rect(&expanded,touch)
    testing.expect(t,expanded == rect_union(a,touch))
}
@(test)
test_plane_projection_and_classification :: proc(t: ^testing.T) {
    p := plane_from_points(Vec3{0,2,0},Vec3{0,2,1},Vec3{1,2,0})
    expect_vector(t,p.normal,VEC3_Y)
    testing.expect(t,plane_which_side(p,Vec3{0,3,0}) == .Front && plane_which_side(p,Vec3{0,1,0}) == .Back)
    testing.expect(t,plane_contains_point(p,Vec3{3,2,4},1e-5))
    expect_vector(t,plane_closest_point(p,Vec3{3,8,4}),Vec3{3,2,4})
    testing.expect(t,plane_intersects_aabb(p,AABB{Vec3{0,1,0},VEC3_ONE}) && plane_intersects_sphere(p,Sphere{Vec3{0,1,0},1}))
    testing.expect(t,plane_normalize(Plane{Vec3{0,2,0},4}) == p)
    testing.expect(t,plane_flip(plane_flip(p)) == p)
}
@(test)
test_plane_nonuniform_transform :: proc(t: ^testing.T) {
    p := plane_from_point_normal(Vec3{1,2,3},Vec3{1,2,3})
    a := mat4_trs(Vec3{4,5,6},quat_axis_angle(VEC3_Z,0.3),Vec3{2,3,4})
    q := plane_transform(p,a)
    transformed := transform_point(a,p.normal*p.distance)
    testing.expect(t,near(plane_distance(q,transformed),0))
    inv,ok := inverse(mat4_to_mat3(a)); testing.expect(t,ok)
    expect_vector(t,q.normal,normalize(matrix_vector(transpose(inv),p.normal)))
}
@(test)
test_ray_plane_sphere_triangle :: proc(t: ^testing.T) {
    r := ray_from_points(Vec3{0,0,3},VEC3_ZERO)
    expect_vector(t,ray_at(r,3),VEC3_ZERO)
    testing.expect(t,near(ray_distance_to_point(r,Vec3{2,0,1}),2))
    p,ok := ray_intersects_plane(r,Plane{VEC3_Z,0})
    testing.expect(t,ok); expect_vector(t,p,VEC3_ZERO)
    hit,sok := ray_intersects_sphere(r,Sphere{{},1})
    testing.expect(t,sok && near(hit.distance,2)); expect_vector(t,hit.normal,VEC3_Z)
    triangle,tok := ray_intersects_triangle(r,Vec3{-1,-1,0},Vec3{1,-1,0},Vec3{0,1,0})
    testing.expect(t,tok); expect_vector(t,triangle,VEC3_ZERO)
    _,parallel := ray_intersects_plane(Ray{{},VEC3_X},Plane{VEC3_Y,2})
    _,back := ray_intersects_sphere(Ray{Vec3{0,0,3},VEC3_Z},Sphere{{},1})
    _,degenerate := ray_intersects_triangle(r,VEC3_ZERO,VEC3_ZERO,VEC3_ZERO)
    _,zero := ray_intersects_sphere(Ray{},Sphere{{},1})
    testing.expect(t,!parallel && !back && !degenerate && !zero)
    rt := ray_transform(r,mat4_trs(Vec3{1,2,3},QUAT_IDENTITY,Vec3{2,3,4}))
    expect_vector(t,rt.origin,Vec3{1,2,15}); expect_vector(t,rt.direction,-VEC3_Z)
}
@(test)
test_ray_box_normals_parallel_and_inside :: proc(t: ^testing.T) {
    box := AABB{{},VEC3_ONE}
    hit,ok := ray_intersects_aabb(Ray{Vec3{-3,0,0},VEC3_X},box)
    testing.expect(t,ok && hit.distance == 2); expect_vector(t,hit.normal,-VEC3_X)
    exit,inside := ray_intersects_aabb(Ray{{},normalize(Vec3{1,2,0})},box)
    testing.expect(t,inside); expect_vector(t,exit.normal,VEC3_Y)
    _,miss := ray_intersects_aabb(Ray{Vec3{-3,2,0},VEC3_X},box)
    boundary,bok := ray_intersects_aabb(Ray{Vec3{-3,1,0},VEC3_X},box)
    testing.expect(t,!miss && bok && boundary.distance == 2)
    _,zero := ray_intersects_aabb(Ray{},box); testing.expect(t,!zero)
}
@(test)
test_frustum_camera_culling_and_finite_corners :: proc(t: ^testing.T) {
    for i in 0..<32 {
        pos := Vec3{f32(i),2,-4}
        dir := normalize(Vec3{1,f32(i)*0.03,-2})
        camera := frustum_from_camera(pos,pos+dir,VEC3_Y,60,1.7,0.1)
        testing.expect(t,frustum_contains_point(camera,pos+dir*5))
        testing.expect(t,!frustum_contains_point(camera,pos-dir*5))
        verts := frustum_corners(camera,12)
        for point,index in verts {
            testing.expect(t,near(plane_distance(camera.near,point),0 if index<4 else 12))
        }
    }
    f := frustum_from_camera(Vec3{2,3,4},Vec3{2,3,3},VEC3_Y,90,1,0.1)
    testing.expect(t,frustum_contains_point(f,Vec3{2,3,2}) && !frustum_contains_point(f,Vec3{2,3,5}))
    testing.expect(t,!frustum_contains_point(f,Vec3{20,3,2}) && frustum_contains_point(f,Vec3{2,3,-1e6}))
    testing.expect(t,frustum_contains_aabb(f,AABB{Vec3{2,3,2},Vec3{0.1,0.1,0.1}}))
    testing.expect(t,frustum_intersects_aabb(f,AABB{Vec3{2,3,3.9},Vec3{0.2,0.2,0.2}}))
    testing.expect(t,!frustum_intersects_sphere(f,Sphere{Vec3{2,3,5},0.1}))
    corners := frustum_corners(f,10)
    for v,i in corners { testing.expect(t,near(v[2],3.9 if i<4 else -6.1)) }
    sphere := frustum_bounding_sphere(f,10)
    for v in corners { testing.expect(t,distance(v,sphere.center) <= sphere.radius+1e-4) }
    expect_vector(t,sphere.center,frustum_center(f,10))
    view,ok := inverse(mat4_lookat(Vec3{2,3,4},Vec3{2,3,3},VEC3_Y)); testing.expect(t,ok)
    g := frustum_from_proj_and_view(mat4_reverse_z(90,1,0.1),view)
    expect_vector(t,g.near.normal,f.near.normal); testing.expect(t,near(g.near.distance,f.near.distance))
}
@(test)
test_color_encoding_and_arithmetic :: proc(t: ^testing.T) {
    c := color_from_rgba_hex(0x64329680)
    testing.expect(t,color_to_bytes(c) == [4]u8{100,50,150,128})
    testing.expect(t,color_from_rgb_hex(0xff0000) == COLOR_RED)
    testing.expect(t,color_from_array(color_to_array(c)) == c)
    expect_vector(t,color_to_array(color_sub(color_add(c,c),c)),color_to_array(c))
    expect_vector(t,color_to_array(color_modulate(c,COLOR_WHITE)),color_to_array(c))
    expect_vector(t,color_to_array(color_scale(c,2)),color_to_array(c)*2)
    expect_vector(t,color_to_array(color_lerp(COLOR_RED,COLOR_BLUE,0.5)),Vec4{0.5,0,0.5,1})
    testing.expect(t,color_brightness(c,2).a == c.a && color_with_alpha(c,0.4).a == 0.4)
    gray := color_saturate(c,0); testing.expect(t,near(gray.r,gray.g) && near(gray.g,gray.b))
    testing.expect(t,color_is_valid(c) && !color_is_valid(Color{-1,2,3,4}))
    testing.expect(t,color_clamped(Color{-1,2,3,-4}) == Color{0,1,1,0})
    testing.expect(t,color_to_bytes(Color{m.nan_f32(),-1,2,0.5}) == [4]u8{0,0,255,128})
}
@(test)
test_color_hsv_and_gamma :: proc(t: ^testing.T) {
    for c in ([5]Color{COLOR_RED,COLOR_GREEN,COLOR_BLUE,color_rgb(0.8,0.2,0.4),color_rgb(0.3,0.3,0.3)}) {
        expect_vector(t,color_to_array(color_from_hsv(color_to_hsv(c))),color_to_array(c))
        expect_vector(t,color_to_array(color_to_srgb(color_to_linear(c))),color_to_array(c))
    }
    c := Color{0.04045,0.5,0.003,0.4}
    lin := color_to_linear(c)
    testing.expect(t,near(lin.r,0.0031308) && near(lin.g,0.2140411) && lin.a == c.a)
}
