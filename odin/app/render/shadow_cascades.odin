//! Four stabilized PSSM cascades share one column-major camera and shadow convention.
package render

import km "../../math"
import "core:math"

Shadow_Cascade :: struct { view_projection:km.Mat4, split_texel:km.Vec4 }
Shadow_Frame :: struct { cascades:[4]Shadow_Cascade, direction,bias:km.Vec4 }
/// Builds normal-depth shadow projections from explicit finite or infinite camera depth conventions.
shadow_cascades :: proc(frame:Frame_Data,lighting:Lighting_Frame,direction:km.Vec3,size:u32,enabled:bool,sense:Depth_Sense=.Forward)->(Shadow_Frame,Scene_Error) {
    if size<32 || size>8192 || size%2!=0 || km.length_squared(direction)<1e-10 { return {},.Invalid_Camera }
    camera_world,ok:=km.inverse(lighting.view); if !ok { return {},.Invalid_Camera }
    projection:=km.matrix_mul(frame.view_projection,camera_world)
    near:=projection[3][2]/projection[2][2] if sense==.Forward else projection[3][2]/(projection[2][2]+1)
    far:=f32(100)
    if sense==.Forward { far=min(far,projection[3][2]/(projection[2][2]+1)) } else if abs(projection[2][2])>1e-8 { far=min(far,projection[3][2]/projection[2][2]) }
    if near<=0 || far<=near || math.is_nan(near) || math.is_inf(far) { return {},.Invalid_Camera }
    normalized:=km.normalize(direction)
    result:=Shadow_Frame{direction=km.vec4(normalized,4 if enabled else 0),bias={1.5/f32(size),3,0,f32(size)}}
    corner_xy:=[4]km.Vec2{{-1,-1},{1,-1},{1,1},{-1,1}}
    previous:=near
    for cascade in 0..<4 {
        fraction:=f32(cascade+1)/4
        split:=0.65*near*math.pow(far/near,fraction)+0.35*(near+(far-near)*fraction)
        corners:[8]km.Vec3
        center:=km.Vec3{}
        for xy,i in corner_xy {
            world:=km.matrix_vector(lighting.inverse_view_projection,km.Vec4{xy[0],xy[1],0.5,1})
            ray:=km.xyz(world)/world[3]-km.xyz(frame.camera_position)
            view_ray:=km.transform_direction(lighting.view,ray)
            if abs(view_ray[2])<1e-8 { return {},.Invalid_Camera }
            ray/= -view_ray[2]
            corners[i]=km.xyz(frame.camera_position)+ray*previous
            corners[i+4]=km.xyz(frame.camera_position)+ray*split
            center+=corners[i]+corners[i+4]
        }
        center/=8
        radius:f32
        for corner in corners { radius=max(radius,km.length(corner-center)) }
        radius=math.ceil(radius*16)/16
        up:=km.Vec3{0,0,1} if abs(normalized[1])>0.999 else km.Vec3{0,1,0}
        view,view_ok:=km.inverse(km.mat4_lookat(center,center+normalized,up)); if !view_ok { return {},.Invalid_Camera }
        texel:=2*radius/f32(size/2)
        // Snap the light-space world origin so sub-texel camera motion does not shimmer.
        view[3][0]=math.floor(view[3][0]/texel)*texel
        view[3][1]=math.floor(view[3][1]/texel)*texel
        minimum,maximum:=f32(max(f32)),f32(-max(f32))
        for corner in corners { z:=km.transform_point(view,corner)[2]; minimum=min(minimum,z); maximum=max(maximum,z) }
        ortho:=km.mat4_ortho(-radius,radius,-radius,radius,-maximum-1,-minimum)
        result.cascades[cascade]={km.matrix_mul(ortho,view),{split,1/f32(size),texel*abs(ortho[2][2]),0}}
        previous=split
    }
    return result,.None
}
