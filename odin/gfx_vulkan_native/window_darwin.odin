#+build darwin
//! A real Cocoa window proves acquire, abort, presentation, resize and retained output ownership.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import glfw "vendor:glfw"
import "core:fmt"
import "core:time"

window_clear :: proc(renderer:^gpu.Renderer,view:rawptr,width,height:u32,color:[4]f64,resize:bool,ticket:^gfx.Readback_Ticket) {
    if resize { assert(gpu.attach_surface(renderer,{view=view,width=width,height=height})==.None) }
    token,acquired:=gpu.acquire(renderer); assert(acquired==.None)
    abandoned,status,surface_error:=gpu.acquire_surface(renderer); assert(status==.Presented && surface_error==.None)
    assert(gpu.abort_surface(renderer,abandoned)==.None)
    assert(gpu.abort(renderer,token)==.None)
    token,acquired=gpu.acquire(renderer); assert(acquired==.None)
    frame: gfx.Surface_Frame
    frame,status,surface_error=gpu.acquire_surface(renderer); assert(status==.Presented && surface_error==.None)
    assert(frame.generation!=abandoned.generation && gpu.abort_surface(renderer,abandoned)==.Invalid_Resource)
    query:=gpu.graphics_query(renderer)
    info,ok:=query.texture(query.state,frame.texture); assert(ok)
    graph:gfx.Graph; gfx.graph_init(&graph); defer { assert(gpu.release_graph_exports(renderer,&graph)==.None); gfx.graph_destroy(&graph) }
    image,error:=gfx.graph_image(&graph,info.desc,{initial=.Undefined,final=.Present},true,true); assert(error==.None)
    access:=gfx.Image_Access{image,gfx.image_full_range(info.desc),.Write,.Color_Attachment}
    pass,pass_error:=gfx.graph_pass(&graph,"window-clear",.Graphics,nil,images={access}); assert(pass_error==.None)
    assert(gfx.graph_set_packet(&graph,pass,gfx.Render{colors={{access,.Clear,.Store,color}}})==.None)
    plan,plan_error:=gfx.graph_compile(&graph); assert(plan_error==.None); defer gfx.compiled_graph_destroy(&plan)
    submission,native_error,packet_error:=gpu.submit(renderer,token,&graph,&plan,nil,{{image,frame.texture}})
    assert(native_error==.None && packet_error==.None)
    source,source_error:=gpu.graph_texture_source(renderer,submission,image); assert(source_error==.None)
    if ticket!=nil {
        ticket^,source_error=gpu.queue_texture_readback(renderer,source,{0,0,0,0,frame.width,frame.height,.Color,0,1,0,0}); assert(source_error==.None)
    }
    outcome,present_error:=gpu.present_surface(renderer,frame,submission)
    assert(present_error==.None && outcome.submission==submission && (outcome.surface==.Presented || outcome.surface==.Recreate))
    assert(gpu.wait(renderer,submission)==.None)
}
run_window :: proc(renderer:^gpu.Renderer) {
    assert(bool(glfw.Init())); defer glfw.Terminate()
    glfw.WindowHint(glfw.CLIENT_API,glfw.NO_API)
    window:=glfw.CreateWindow(160,96,"Katla Odin Vulkan acceptance",nil,nil); assert(window!=nil)
    defer glfw.DestroyWindow(window)
    view:=rawptr(glfw.GetCocoaView(window))
    width,height:=glfw.GetFramebufferSize(window)
    ticket:gfx.Readback_Ticket
    window_clear(renderer,view,u32(width),u32(height),{0.25,0.5,0.75,1},true,&ticket)
    glfw.SetWindowSize(window,240,144); glfw.PollEvents()
    width,height=glfw.GetFramebufferSize(window)
    window_clear(renderer,view,u32(width),u32(height),{1,0,0,1},true,nil)
    completed:=false
    for _ in 0..<5000 {
        data,done,error:=gpu.poll_texture_readback(renderer,ticket); assert(error==.None)
        if done {
            assert(data.region.width>0 && data.region.height>0)
            for y in 0..<data.region.height { for x in 0..<data.region.width {
                i:=int(u64(y)*data.row_pitch+u64(x)*4)
                assert(data.bytes[i]==191 && data.bytes[i+1]==128 && data.bytes[i+2]==64 && data.bytes[i+3]==255)
            } }
            fmt.println("Window: acquired-image abort/reacquire, accepted present, resize and retained BGRA pixels verified",data.region.width,data.region.height)
            gfx.readback_data_destroy(&data); completed=true; break
        }
        time.sleep(time.Millisecond)
    }
    assert(completed)
    assert(gpu.detach_surface(renderer)==.None)
}
