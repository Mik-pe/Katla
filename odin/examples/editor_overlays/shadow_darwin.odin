#+build darwin, arm64
//! Lit planar receiver pixels reject shadow acne and selected-face stencil artifacts without erasing cast shadows.
package main

import render "../../app/render"
import gfx "../../gfx"
import ecs "../../ecs"
import km "../../math"
import "core:fmt"

prove_shadow_receiver :: proc(scene:^render.Native_Scene($R),captures:Capture(R),selected:ecs.Entity_Id,objects:[]render.Object_Data,draws:[]gfx.Draw_Op,backend:string) {
    camera:=render.camera_default(); camera.position={0,2.5,5}; camera.target={0,.6,0}
    frame,frame_error:=render.frame_data(camera,256,192,backend=="vulkan"); assert(frame_error==.None)
    images:[3]gfx.Readback_Data; defer { for &image in images { gfx.readback_data_destroy(&image) } }
    settings:=scene.feature_settings; defer { scene.feature_settings=settings; assert(render.native_scene_select(scene,{selected})=={}) }
    scene.feature_settings.grid=false
    for cycle in 0..<3 {
        scene.feature_settings.shadows=cycle!=0
        assert(render.native_scene_select(scene,{selected} if cycle==2 else nil)=={})
        token,acquire_error:=render.native_scene_acquire(scene); assert(acquire_error==.None)
        submission,error:=render.native_scene_render(scene,token,frame,objects,draws); assert(error=={})
        images[cycle]=capture(scene,captures,submission,scene.graph.output)
        assert(render.native_scene_wait(scene,submission)==.None)
    }
    samples:=0
    for face in 0..<2 { for y in 0..<8 { for x in 0..<8 {
        world:=km.Vec3{.8+(f32(x)/7-.5)*.6,.5+(f32(y)/7-.5)*.6,.5}
        if face==1 { world={world[0],1,(f32(y)/7-.5)*.6} }
        clip:=km.matrix_vector(frame.view_projection,km.vec4(world,1)); point:=clip/clip[3]
        px,py:=int((point[0]*.5+.5)*256),int((point[1]*.5+.5)*192)
        assert(px>1 && px<254 && py>1 && py<190)
        offset:=py*int(images[0].row_pitch)+px*4
        for channel in 0..<3 {
            if abs(int(images[0].bytes[offset+channel])-int(images[1].bytes[offset+channel]))>2 { fmt.println("Shadow face difference:",backend,face,world,px,py,channel,images[0].bytes[offset+channel],images[1].bytes[offset+channel]) }
            assert(abs(int(images[0].bytes[offset+channel])-int(images[1].bytes[offset+channel]))<=2,"an unobstructed planar cube face retained patterned self-shadow acne")
            assert(images[1].bytes[offset+channel]==images[2].bytes[offset+channel],"selection stencil exposed expanded shell inside the visible cube face")
        }
        samples+=1
    } } }
    cast_pixels:=changed_pixels(&images[0],&images[1],6); outline:=changed_pixels(&images[1],&images[2],3)
    assert(cast_pixels>50,"acne correction erased genuine scene cast shadows"); assert(outline>50,"selection no longer drew a real outside contour")
    fmt.println("Native planar shadow/stencil PASS:",backend,"lit face samples",samples,"cast shadow pixels",cast_pixels,"outside outline pixels",outline)
}
