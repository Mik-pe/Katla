#+test
package render

import app ".."
import km "../../math"
import "core:testing"
import m "core:math"

@(test)
test_scene_gpu_abi_linear_material_and_normal_matrix :: proc(t:^testing.T) {
    testing.expect_value(t,size_of(Vertex),48)
    testing.expect_value(t,size_of(Object_Data),160)
    testing.expect_value(t,size_of(Frame_Data),128)
    color:=km.Color{0.17,0.33,0.61,0.9}
    surface:=app.Surface_Material{color,true,0.7,0.3,0.8}
    transform:=km.transform(position={1,2,3},rotation=km.quat_axis_angle(km.VEC3_Y,km.FRAC_PI_4),scale={2,3,4})
    data,error:=object_data(transform,surface)
    testing.expect_value(t,error,Scene_Error.None)
    testing.expect_value(t,data.linear_color,km.color_to_array(color))
    testing.expect_value(t,data.factors,km.Vec4{0.7,0.3,0.8,0})
    testing.expect_value(t,km.transform_point(data.model,{0,0,0}),km.Vec3{1,2,3})
    normal:=km.normalize(km.transform_direction(data.normal_model,{1,1,0}))
    tangent:=km.transform_direction(data.model,{1,-1,0})
    testing.expect(t,abs(km.dot(normal,tangent))<1e-5)
    bad:=transform; bad.scale[0]=0
    _,error=object_data(bad,surface); testing.expect_value(t,error,Scene_Error.Invalid_Transform)
    surface.roughness=m.nan_f32()
    _,error=object_data(transform,surface); testing.expect_value(t,error,Scene_Error.Invalid_Material)
}

@(test)
test_scene_camera_backend_clip_and_rejections :: proc(t:^testing.T) {
    camera:=camera_default()
    vulkan,err:=frame_data(camera,320,160,true); testing.expect_value(t,err,Scene_Error.None)
    metal:Frame_Data; metal,err=frame_data(camera,320,160,false); testing.expect_value(t,err,Scene_Error.None)
    testing.expect_value(t,vulkan.view_projection,metal.view_projection)
    testing.expect(t,vulkan.ambient[3]==1 && metal.ambient[3]==-1)
    center:=km.matrix_vector(vulkan.view_projection,km.Vec4{0,0,0,1})
    testing.expect(t,abs(center[0])<1e-5 && abs(center[1])<1e-5 && center[3]>0 && center[2]/center[3]>0 && center[2]/center[3]<1)
    camera.target=camera.position
    _,err=frame_data(camera,320,160,true); testing.expect_value(t,err,Scene_Error.Invalid_Camera)
    camera=camera_default(); camera.far=m.inf_f32(1)
    _,err=frame_data(camera,320,160,true); testing.expect_value(t,err,Scene_Error.Invalid_Camera)
}

@(test)
test_scene_sphere_triangle_geometry_and_index_preflight :: proc(t:^testing.T) {
    sphere,err:=geometry_sphere(32,16); testing.expect_value(t,err,Scene_Error.None); defer geometry_destroy(&sphere)
    testing.expect_value(t,len(sphere.vertices),32*15*6)
    for i:=0;i<len(sphere.vertices);i+=3 {
        a,b,c:=km.xyz(sphere.vertices[i].position),km.xyz(sphere.vertices[i+1].position),km.xyz(sphere.vertices[i+2].position)
        outward:=km.cross(b-a,c-a)
        testing.expect(t,km.dot(outward,a+b+c)>0)
        for vertex in sphere.vertices[i:i+3] { testing.expect(t,abs(km.length(km.xyz(vertex.position))-0.5)<1e-5 && abs(km.length(km.xyz(vertex.normal))-1)<1e-5) }
    }
    positions:=[3]km.Vec3{{0,0,0},{1,0,0},{0,1,0}}
    normals:=[3]km.Vec3{{0,0,1},{0,0,1},{0,0,1}}
    indices:=[3]u32{0,1,3}
    rejected,error:=geometry_from_indexed(positions[:],normals[:],nil,indices[:]); testing.expect_value(t,error,Scene_Error.Invalid_Geometry); testing.expect(t,rejected.vertices==nil)
    indices[2]=2
    triangle:Geometry; triangle,error=geometry_from_indexed(positions[:],normals[:],nil,indices[:]); testing.expect_value(t,error,Scene_Error.None); defer geometry_destroy(&triangle)
    testing.expect_value(t,triangle.vertices[1].position,km.Vec4{1,0,0,1})
}
