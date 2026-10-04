#+build darwin, arm64
//! Real native windows consume the same material scene graph and acquired frame slots.
package main

import app "../../app"
import window "../../app/window"
import render "../../app/render"
import ecs "../../ecs"
import km "../../math"
import gfx "../../gfx"
import "core:fmt"
import "core:time"

Surface_Ops :: struct($R:typeid) {
    attach:proc(^R,gfx.Surface_Desc)->gfx.Gpu_Error,
    resize:proc(^R,u32,u32)->gfx.Gpu_Error,
    detach:proc(^R)->gfx.Gpu_Error,
    acquire:proc(^R)->(gfx.Surface_Frame,gfx.Surface_Result,gfx.Gpu_Error),
    abort:proc(^R,gfx.Surface_Frame)->gfx.Gpu_Error,
    present:proc(^R,gfx.Surface_Frame,gfx.Submission)->(gfx.Present_Outcome,gfx.Gpu_Error),
}
exercise_window :: proc(renderer:^$R,operations:render.GPU_Ops(R),capture:Capture_Ops(R),surface:Surface_Ops(R),descriptor:gfx.Graphics_Desc,backend:string,output:string) {
    native_window:window.Window; assert(window.window_create(&native_window,"Katla Odin materials",520,340)==.None)
    defer window.window_destroy(&native_window)
    initial:=window.window_poll(&native_window)
    assert(initial.visible && initial.width>0 && initial.height>0)
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner)
    ids:=[2]ecs.Entity_Id{
        ecs.spawn(&owner.world,struct { transform:app.Scene_Transform, surface:app.Surface_Material }{{km.transform(position={-0.65,0,0},scale={1.4,1.4,1.4})},{km.color_to_linear({0.85,0.12,0.08,1}),true,0,0.7,1}}),
        ecs.spawn(&owner.world,struct { transform:app.Scene_Transform, surface:app.Surface_Material }{{km.transform(position={0.65,0,0},scale={1.4,1.4,1.4})},{km.color_to_linear({0.08,0.2,0.85,1}),true,0,0.4,1}}),
    }
    geometry,error:=render.geometry_sphere(48,24); assert(error==.None); defer render.geometry_destroy(&geometry)
    native:render.Native_Scene(R)
    initialization_error:=render.native_scene_init(&native,renderer,operations,descriptor,&geometry,2,3,initial.width,initial.height)
    fmt.println("Window scene initialization:",initialization_error); assert(initialization_error=={})
    defer { assert(render.native_scene_destroy(&native)==.None) }
    assert(surface.attach(renderer,{view=window.window_view(&native_window),width=initial.width,height=initial.height})==.None)
    defer { assert(surface.detach(renderer)==.None) }
    camera:=render.camera_default(); camera.position={0,0,3.4}
    draws:=[1]gfx.Draw_Op{gfx.Draw{u32(len(geometry.vertices)),2,0,0}}
    gesture:app.Material_Gesture; defer app.material_gesture_destroy(&gesture)
    first_ticket:gfx.Readback_Ticket; first_source:gfx.Texture_Source
    newest_ticket:gfx.Readback_Ticket; newest_source:gfx.Texture_Source
    newest_pixels:gfx.Readback_Data; defer gfx.readback_data_destroy(&newest_pixels)
    for index in 0..<8 {
        if index==1 {
            assert(app.material_gesture_begin(&owner,&gesture,ids[:1])==.None)
            assert(app.material_gesture_preview(&owner,&gesture,{.Base_Color,.Roughness},{base_color={0.12,0.82,0.2,1},roughness=0.1})==.None)
        }
        if index==2 { assert(app.material_gesture_finish(&owner,&gesture)==.None && len(owner.agent.session.actions)==1) }
        if index==3 { assert(app.authoring_undo_last(&owner)==.None) }
        if index==4 { assert(app.authoring_redo_last(&owner)==.None) }
        if index==5 { assert(window.window_resize(&native_window,600,400)==.None) }
        state:=window.window_poll(&native_window); assert(state.visible && !state.closed)
        if state.width!=native.graph.color_desc.width || state.height!=native.graph.color_desc.height {
            assert(render.native_scene_resize(&native,state.width,state.height)=={})
            assert(surface.resize(renderer,state.width,state.height)==.None)
        }
        frame,camera_error:=render.frame_data(camera,state.width,state.height,backend=="vulkan"); assert(camera_error==.None)
        objects:=[2]render.Object_Data{}
        for id,i in ids { data,scene_error:=render.scene_object_data(&owner,id); assert(scene_error==.None); objects[i]=data }
        token,acquire_error:=render.native_scene_acquire(&native); assert(acquire_error==.None)
        target,result,surface_error:=surface.acquire(renderer); fmt.println("Surface acquire:",index,state.width,state.height,target.width,target.height,result,surface_error); assert(surface_error==.None && result==.Presented && target.width==state.width && target.height==state.height)
        submission,render_error:=render.native_scene_render(&native,token,frame,objects[:],draws[:],target.texture)
        if render_error!={} { surface.abort(renderer,target); operations.abort(renderer,token); fmt.println(render_error); assert(false,"window scene submission failed") }
        if index==0 || index==7 {
            source,source_error:=capture.source(renderer,submission,native.graph.color); assert(source_error==.None)
            queued,queue_error:=capture.queue(renderer,source,{width=source.desc.width,height=source.desc.height,aspect=.Color,depth=1}); assert(queue_error==.None)
            if index==0 { first_source=source; first_ticket=queued } else { newest_source=source; newest_ticket=queued }
        }
        outcome,present_error:=surface.present(renderer,target,submission)
        assert(present_error==.None && outcome.submission==submission && outcome.surface==.Presented)
        assert(render.native_scene_wait(&native,submission)==.None)
        if index==7 { newest_pixels=poll_pixels(renderer,capture,newest_ticket,newest_source) }
        time.sleep(10*time.Millisecond)
    }
    old_pixels,ready,poll_error:=capture.poll(renderer,first_ticket); assert(poll_error==.None && ready); defer gfx.readback_data_destroy(&old_pixels)
    assert(old_pixels.source==first_source && old_pixels.region.width==initial.width && old_pixels.region.height==initial.height)
    assert(newest_pixels.region.width>initial.width && newest_pixels.region.height>initial.height)
    before_left:=region_mean(&old_pixels,initial.width/8,initial.width*15/32)
    after_left:=region_mean(&newest_pixels,newest_pixels.region.width/8,newest_pixels.region.width*15/32)
    assert(before_left[0]>before_left[2]+15 && after_left[1]>after_left[0]+15,"presented window lost grouped material edits or the retained pre-resize source")
    save_pixels(&newest_pixels,output)
    fmt.printf("%s window accepted 8 presents, gesture/undo/redo and retained capture %dx%d -> %dx%d; %s\n",backend,initial.width,initial.height,newest_pixels.region.width,newest_pixels.region.height,output)
}
