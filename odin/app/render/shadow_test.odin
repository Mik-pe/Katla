#+test
package render

import km "../../math"
import "core:testing"

@(test)
test_cascades_contain_near_scene_geometry_with_stable_square_extents :: proc(t:^testing.T) {
    camera:=camera_default(); camera.position={0,3,5}; camera.target={0,0.5,0}; camera.far=150
    frame,error:=frame_data(camera,256,192,true); testing.expect_value(t,error,Scene_Error.None)
    lighting,light_error:=lighting_frame(frame,256,192,{},true,true); testing.expect_value(t,light_error,Scene_Error.None)
    shadows,shadow_error:=shadow_cascades(frame,lighting,{-0.7,-1,-0.3},256,true); testing.expect_value(t,shadow_error,Scene_Error.None)
    testing.expect_value(t,size_of(Shadow_Frame),352)
    previous:f32
    for cascade in shadows.cascades {
        testing.expect(t,cascade.split_texel[0]>previous); previous=cascade.split_texel[0]
        testing.expect_value(t,cascade.split_texel[1],f32(1)/256)
    }
    for point in ([]km.Vec3{{0,0,0},{-0.9,0.65,0},{0.8,0.6,0}}) {
        clip:=km.matrix_vector(shadows.cascades[0].view_projection,km.vec4(point,1))
        testing.expect(t,abs(clip[0])<1 && abs(clip[1])<1 && clip[2]>0 && clip[2]<1)
    }
    disabled,disabled_error:=shadow_cascades(frame,lighting,{0,-1,0},256,false); testing.expect_value(t,disabled_error,Scene_Error.None)
    testing.expect_value(t,disabled.direction[3],f32(0))
    _,invalid_error:=shadow_cascades(frame,lighting,{0,0,0},256,true); testing.expect_value(t,invalid_error,Scene_Error.Invalid_Camera)
}
