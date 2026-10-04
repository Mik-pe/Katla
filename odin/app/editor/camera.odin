//! Editor cameras are view state; navigation never creates authored scene undo entries.
package editor_app

import km "../../math"
import ui "../../ui"
import "core:math"

Viewport_Layout :: enum { Single,Horizontal_2,Vertical_2,Quad }
Focus_Move :: struct { active:bool,elapsed,duration:f32,from_target,to_target:km.Vec3,from_distance,to_distance:f32 }
Camera :: struct { target:km.Vec3,yaw,pitch,distance,fov,near,far:f32,focus:Focus_Move }
Viewport :: struct { camera:Camera,bounds:ui.Rect,texture:ui.Texture_Id,focused:bool }
Viewport_Grid :: struct { layout:Viewport_Layout,slots:[4]Viewport,active:int,has_active:bool }

camera_default :: proc()->Camera { return {target={},yaw=0,pitch=0.19739556,distance=10.198039,fov=60,near=0.01,far=10000} }
viewport_grid_init :: proc(grid:^Viewport_Grid) { grid^={}; for &slot,index in grid.slots { slot.camera=camera_default(); slot.texture=ui.Texture_Id(index+16) } }
viewport_count :: proc(layout:Viewport_Layout)->int {
    switch layout {
    case .Single: return 1
    case .Horizontal_2,.Vertical_2: return 2
    case .Quad: return 4
    }
    return 0
}
/// Camera owners persist when the visible grid layout changes.
viewport_grid_layout :: proc(grid:^Viewport_Grid,bounds:ui.Rect,gap:f32=4) {
    columns,rows:=1,1
    if grid.layout==.Horizontal_2 || grid.layout==.Quad { columns=2 }
    if grid.layout==.Vertical_2 || grid.layout==.Quad { rows=2 }
    width:=max(0,(bounds.width-f32(columns-1)*gap)/f32(columns))
    height:=max(0,(bounds.height-f32(rows-1)*gap)/f32(rows))
    for i in 0..<viewport_count(grid.layout) { grid.slots[i].bounds={bounds.x+f32(i%columns)*(width+gap),bounds.y+f32(i/columns)*(height+gap),width,height} }
    if grid.has_active && grid.active>=viewport_count(grid.layout) { grid.has_active=false }
}
camera_position :: proc(camera:^Camera)->km.Vec3 {
    cp:=math.cos(camera.pitch)
    return camera.target+camera.distance*km.Vec3{cp*math.sin(camera.yaw),math.sin(camera.pitch),cp*math.cos(camera.yaw)}
}
camera_view_projection :: proc(camera:^Camera,aspect:f32)->km.Mat4 {
    projection:=km.mat4_perspective(camera.fov,max(0.001,aspect),camera.near,camera.far)
    view,valid:=km.inverse(km.mat4_lookat(camera_position(camera),camera.target,km.VEC3_Y)); if !valid { return km.identity(km.Mat4) }; return km.matrix_mul(projection,view)
}
/// Uses logical-pixel movement and viewport height, independent of Retina backing scale.
camera_navigate :: proc(camera:^Camera,delta:ui.Vec2,wheel:f32,orbit,pan:bool,height:f32,speed:f32=50) {
    if !orbit && !pan && wheel==0 { return }
    camera.focus.active=false
    factor:=clamp(speed,1,200)/50
    if orbit { camera.yaw-=delta[0]*0.005*factor; camera.pitch=clamp(camera.pitch+delta[1]*0.005*factor,-1.553343,1.553343) }
    if pan && height>0 {
        eye:=camera_position(camera); forward:=km.normalize(camera.target-eye)
        right:=km.normalize(km.cross(forward,km.VEC3_Y)); up:=km.normalize(km.cross(right,forward))
        per_pixel:=2*camera.distance*math.tan(camera.fov*math.PI/360)/height
        camera.target-=right*delta[0]*per_pixel*factor; camera.target+=up*delta[1]*per_pixel*factor
    }
    if wheel!=0 { camera.distance=clamp(camera.distance*math.exp(-wheel*0.1*factor),0.05,100000) }
}
/// Fits a real world-space selection bound while retaining the camera's viewing direction.
camera_focus :: proc(camera:^Camera,bounds:km.AABB,aspect:f32) {
    radius:=max(0.1,km.length(bounds.extent))
    half_fov:=camera.fov*math.PI/360
    if aspect<1 { half_fov=math.atan(math.tan(half_fov)*max(0.001,aspect)) }
    distance:=radius/max(0.001,math.sin(half_fov))*1.15
    camera.focus={true,0,0.25,camera.target,bounds.center,camera.distance,max(distance,camera.near+radius)}
}
/// A bounded ease-out keeps focus continuous and can be interrupted by actual navigation input.
camera_tick :: proc(camera:^Camera,dt:f32) {
    if !camera.focus.active || dt<0 || math.is_nan(dt) || math.is_inf(dt) { return }
    focus:=&camera.focus; focus.elapsed=min(focus.duration,focus.elapsed+dt)
    t:=focus.elapsed/focus.duration; eased:=1-(1-t)*(1-t)*(1-t)
    camera.target=focus.from_target+(focus.to_target-focus.from_target)*eased
    camera.distance=focus.from_distance+(focus.to_distance-focus.from_distance)*eased
    if t>=1 { focus.active=false }
}
/// Applies grid quantization only on the manipulated axes; untouched coordinates remain exact.
snap_translation :: proc(value:km.Vec3,axes:[3]bool,size:f32)->km.Vec3 {
    if size<=0 || math.is_nan(size) || math.is_inf(size) { return value }
    result:=value
    for enabled,i in axes { if enabled { result[i]=math.round(value[i]/size)*size } }
    return result
}
