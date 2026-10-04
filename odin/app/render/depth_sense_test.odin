#+test
package render

import gfx "../../gfx"
import km "../../math"
import "core:testing"

@(test)
test_explicit_depth_changes_only_camera_comparison :: proc(t:^testing.T) {
    forward:=gfx.Graphics_Desc{depth={enabled=true,test=true,write=true,compare=.Less,format=.D32_Float_S8_Uint}}
    reverse:=depth_descriptor(forward,.Reverse)
    testing.expect_value(t,reverse.depth.compare,gfx.Compare_Op.Greater)
    testing.expect_value(t,reverse.depth.write,true)
    testing.expect_value(t,depth_descriptor(forward,.Forward).depth,forward.depth)
    always:=forward; always.depth.compare=.Always
    testing.expect_value(t,depth_descriptor(always,.Reverse).depth,always.depth)
    testing.expect_value(t,depth_clear(.Forward),f32(1))
    testing.expect_value(t,depth_clear(.Reverse),f32(0))
    testing.expect(t,!depth_sense_valid(cast(Depth_Sense)99))
}

@(test)
test_infinite_reverse_camera_preserves_forward_shadow_cascades :: proc(t:^testing.T) {
    camera:=camera_default(); camera.position={0,3,5}; camera.target={0,.5,0}; camera.far=1000
    forward,frame_error:=frame_data(camera,256,192,true); testing.expect_value(t,frame_error,Scene_Error.None)
    reverse:=forward; view,ok:=km.inverse(km.mat4_lookat(camera.position,camera.target,camera.up)); testing.expect(t,ok)
    reverse.view_projection=km.matrix_mul(km.mat4_reverse_z(camera.fov_degrees,256.0/192.0,camera.near),view)
    lights_forward,light_error:=lighting_frame(forward,256,192,{},true,true); testing.expect_value(t,light_error,Scene_Error.None)
    lights_reverse,reverse_error:=lighting_frame(reverse,256,192,{},true,true); testing.expect_value(t,reverse_error,Scene_Error.None)
    a,error_a:=shadow_cascades(forward,lights_forward,{-1,-1,-1},256,true,.Forward)
    b,error_b:=shadow_cascades(reverse,lights_reverse,{-1,-1,-1},256,true,.Reverse)
    testing.expect_value(t,error_a,Scene_Error.None); testing.expect_value(t,error_b,Scene_Error.None)
    for cascade,i in a.cascades {
        testing.expect(t,abs(cascade.split_texel[0]-b.cascades[i].split_texel[0])<.001)
        for column,j in cascade.view_projection { for value,k in column { testing.expect(t,abs(value-b.cascades[i].view_projection[j][k])<.001) } }
    }
}
