#+build darwin, arm64
//! Actual scene geometry proves lighting, shadows, overlays and the final HDR transform.
package main

import app "../../app"
import render "../../app/render"
import ecs "../../ecs"
import gfx "../../gfx"
import metal "../../gfx/metal"
import vulkan "../../gfx/vulkan"
import km "../../math"
import shader "../../gfx/shader"
import "core:fmt"
import "core:mem"
import "core:math"
import "core:os"
import "core:time"
import NS "core:sys/darwin/Foundation"

Capture :: struct($R:typeid) { source:proc(^R,gfx.Submission,gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error),queue:proc(^R,gfx.Texture_Source,gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error),poll:proc(^R,gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error) }
capture :: proc(scene:^render.Native_Scene($R),ops:Capture(R),submission:gfx.Submission,id:gfx.Image_Id,aspect:gfx.Image_Aspect=.Color)->gfx.Readback_Data {
    source,error:=ops.source(scene.renderer,submission,id); assert(error==.None)
    ticket,queue_error:=ops.queue(scene.renderer,source,{width=source.desc.width,height=source.desc.height,depth=1,aspect=aspect}); assert(queue_error==.None)
    start:=time.tick_now()
    for { result,ready,poll_error:=ops.poll(scene.renderer,ticket); assert(poll_error==.None); if ready { return result }; assert(time.tick_since(start)<10*time.Second); time.sleep(time.Millisecond) }
}
draw :: proc(consumer:^render.Native_Consumer($R),frame:render.Frame_Data)->gfx.Submission {
    assert(render.native_consumer_refresh(consumer)=={})
    scene:=consumer.active
    token,error:=render.native_scene_acquire(scene); assert(error==.None)
    submission,render_error:=render.native_scene_render(scene,token,frame,consumer.batch.objects,consumer.batch.draws)
    if render_error!={} { fmt.println("Feature submission rejected:",render_error); assert(false) }
    return submission
}
changed_pixels :: proc(a,b:^gfx.Readback_Data,threshold:int)->int {
    assert(len(a.bytes)==len(b.bytes)); changed:=0
    for offset:=0;offset<len(a.bytes);offset+=4 { difference:=0; for channel in 0..<3 { difference+=abs(int(a.bytes[offset+channel])-int(b.bytes[offset+channel])) }; if difference>threshold { changed+=1 } }
    return changed
}
half :: proc(bytes:[]byte,offset:int)->f32 { bits:=u16(bytes[offset])|u16(bytes[offset+1])<<8; return f32(transmute(f16)bits) }
encode :: proc(x:f32)->f32 { return 12.92*x if x<=0.0031308 else 1.055*math.pow(x,1/f32(2.4))-0.055 }
aces :: proc(x:f32)->f32 { return clamp(x*(2.51*x+0.03)/(x*(2.43*x+0.59)+0.14),0,1) }
tone :: proc(x:f32,mode:render.Tonemap_Operator)->f32 {
    switch mode {
    case .ACES: return aces(x)
    case .Reinhard: return x/(x+1)
    case .Tony_McMapface: return clamp(1-math.exp(-math.pow(aces(x),1.2)*1.5),0,1)
    case .Linear: return clamp(x,0,1)
    }
    return 0
}
spawn_mesh :: proc(owner:^app.Authoring,source:string,position:km.Vec3,color:km.Color)->ecs.Entity_Id {
    mesh,error:=app.scene_mesh_prepare(owner,{kind=.Geometry,geometry=transmute([]byte)source}); assert(error==.None)
    return ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,mesh:app.Scene_Mesh,material:app.Surface_Material}{{km.transform(position=position)},mesh,{linear_color=color,has_tint=true,metallic=0,roughness=.7,ao=1}})
}
exercise :: proc(renderer:^$R,ops:render.GPU_Ops(R),captures:Capture(R),pipelines:render.Scene_Pipelines,backend:string,particle_ops:render.Particle_GPU_Ops(R),compiler:^shader.Compiler) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); assert(app.authoring_services_init(&owner)==.None)
    floor:=spawn_mesh(&owner,`{"kind":"plane","width":8,"height":8}`,{0,0,0},{0.55,0.55,0.55,1})
    sphere:=spawn_mesh(&owner,`{"kind":"sphere","radius":0.65,"segments":32,"rings":16}`,{-0.9,0.65,0},{0.7,0.18,0.06,1})
    cube:=spawn_mesh(&owner,`{"kind":"cube","size":[1.2,1.2,1.2]}`,{0.8,0.6,0},{0.12,0.25,0.6,1})
    hidden_sphere:=spawn_mesh(&owner,`{"kind":"sphere","radius":0.35,"segments":24,"rings":12}`,{0.8,0.55,-0.8},{0.1,0.5,0.1,1})
    sun:=ecs.spawn(&owner.world,struct {light:app.Scene_Directional_Light}{{{-0.7,-1,-0.3},{1,0.98,0.95},4}})
    point:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,light:app.Scene_Point_Light}{{km.transform(position={0.1,1.3,1.4})},{{0.1,0.2,1},20,4}})
    consumer:render.Native_Consumer(R); assert(render.native_consumer_init(&consumer,&owner,renderer,ops,pipelines,3,256,192)=={}); defer { assert(render.native_consumer_destroy(&consumer)==.None) }
    scene:=consumer.active
    scene.feature_settings.shadow_size=1024; assert(render.native_scene_resize(scene,256,192)=={})
    // The proof observes actual rendered HDR and generated shadow depth, without fabricated texture input.
    scene.graph.graph.images[scene.graph.color.index].exported=true
    scene.graph.graph.images[scene.graph.features.atlas.index].exported=true
    scene.graph.graph.images[scene.graph.features.indicator.index].exported=true
    scene.graph.graph.revision+=1
    camera:=render.camera_default(); camera.position={0,3,5}; camera.target={0,0.5,0}; camera.far=150
    frame,error:=render.frame_data(camera,256,192,backend=="vulkan"); assert(error==.None)
    original:=draw(&consumer,frame)
    original_pixels:=capture(scene,captures,original,scene.graph.output); defer gfx.readback_data_destroy(&original_pixels)
    depth:=capture(scene,captures,original,scene.graph.features.atlas,.Depth); defer gfx.readback_data_destroy(&depth)
    depth_pixels:=0
    for offset:=0;offset<len(depth.bytes);offset+=4 { bits:=u32(depth.bytes[offset])|u32(depth.bytes[offset+1])<<8|u32(depth.bytes[offset+2])<<16|u32(depth.bytes[offset+3])<<24; value:=transmute(f32)bits; assert(value>=0 && value<=1); if value<0.999 { depth_pixels+=1 } }
    fmt.println("Rendered shadow texels:",backend,depth_pixels)
    quadrant_counts:[4]int
    for quadrant in 0..<4 {
        for y in 0..<512 { for x in 0..<512 {
            offset:=((y+quadrant/2*512)*1024+x+quadrant%2*512)*4
            bits:=u32(depth.bytes[offset])|u32(depth.bytes[offset+1])<<8|u32(depth.bytes[offset+2])<<16|u32(depth.bytes[offset+3])<<24
            if transmute(f32)bits<0.999 { quadrant_counts[quadrant]+=1 }
        } }
        assert(quadrant_counts[quadrant]>100)
        if quadrant>0 { assert(quadrant_counts[quadrant-1]>quadrant_counts[quadrant],"cascades reused the final phase's projection constants") }
    }
    snapshot,_:=render.lighting_collect(&owner)
    effective:=render.lighting_apply(frame,snapshot)
    light_frame,_:=render.lighting_frame(effective,256,192,snapshot,true,true)
    cascades,_:=render.shadow_cascades(effective,light_frame,snapshot.sun.direction,1024,true)
    hits:=0
    for iz in -30..<31 { for ix in -30..<31 {
        world:=km.Vec3{f32(ix)/10,0,f32(iz)/10}
        distance:= -km.transform_point(light_frame.view,world)[2]
        index:=3; for cascade,i in cascades.cascades { if distance<=cascade.split_texel[0] { index=i; break } }
        clip:=km.matrix_vector(cascades.cascades[index].view_projection,km.vec4(world,1))
        uv:=km.Vec2{clip[0]*0.5+0.5,clip[1]*0.5+0.5}
        if uv[0]<0 || uv[0]>=1 || uv[1]<0 || uv[1]>=1 { continue }
        x:=int(uv[0]*512)+index%2*512
        y:=int(uv[1]*512)+index/2*512
        offset:=(y*1024+x)*4
        bits:=u32(depth.bytes[offset])|u32(depth.bytes[offset+1])<<8|u32(depth.bytes[offset+2])<<16|u32(depth.bytes[offset+3])<<24
        if transmute(f32)bits<clip[2]-cascades.bias[0] { hits+=1 }
    } }
    assert(hits>100,"actual cascade depth did not shadow world-space floor probes")
    assert(depth_pixels>100,"real scene geometry did not populate directional shadow depth")
    assert(render.native_scene_wait(scene,original)==.None)
    ecs.get_component_mut(&owner.world,point,app.Scene_Point_Light).intensity=0
    no_point:=draw(&consumer,frame); no_point_pixels:=capture(scene,captures,no_point,scene.graph.output); defer gfx.readback_data_destroy(&no_point_pixels)
    point_pixels:=changed_pixels(&original_pixels,&no_point_pixels,12); assert(point_pixels>400,"authored point light did not reach actual material shading through GPU tile culling"); assert(render.native_scene_wait(scene,no_point)==.None)
    scene.feature_settings.shadows=false
    no_shadow:=draw(&consumer,frame); no_shadow_pixels:=capture(scene,captures,no_shadow,scene.graph.output); defer gfx.readback_data_destroy(&no_shadow_pixels)
    shadow_pixels:=changed_pixels(&no_point_pixels,&no_shadow_pixels,8); fmt.println("Shadow changed pixels:",backend,shadow_pixels); assert(shadow_pixels>80,"sampled cascaded shadow did not change actual floor/sphere pixels"); assert(render.native_scene_wait(scene,no_shadow)==.None)
    scene.feature_settings.grid=false
    no_grid:=draw(&consumer,frame); no_grid_pixels:=capture(scene,captures,no_grid,scene.graph.output); defer gfx.readback_data_destroy(&no_grid_pixels)
    grid_pixels:=changed_pixels(&no_shadow_pixels,&no_grid_pixels,8); assert(grid_pixels>40,"real floor grid lines did not appear in the scene depth pass"); assert(render.native_scene_wait(scene,no_grid)==.None)
    scene.feature_settings.sky=false
    no_sky:=draw(&consumer,frame); no_sky_pixels:=capture(scene,captures,no_sky,scene.graph.output); defer gfx.readback_data_destroy(&no_sky_pixels)
    sky_pixels:=changed_pixels(&no_grid_pixels,&no_sky_pixels,12); assert(sky_pixels>1000,"camera-driven environment sky was not rendered"); assert(render.native_scene_wait(scene,no_sky)==.None)
    assert(render.native_scene_select(scene,{sphere})=={})
    outlined:=draw(&consumer,frame); outline_pixels:=capture(scene,captures,outlined,scene.graph.output); defer gfx.readback_data_destroy(&outline_pixels)
    outline_count:=changed_pixels(&no_sky_pixels,&outline_pixels,12); assert(outline_count>80,"selected real geometry did not produce a stencil-restricted silhouette outline"); assert(render.native_scene_wait(scene,outlined)==.None)
    assert(render.native_scene_select(scene,{hidden_sphere})=={})
    hidden:=draw(&consumer,frame); hidden_pixels:=capture(scene,captures,hidden,scene.graph.output); defer gfx.readback_data_destroy(&hidden_pixels)
    indicator:=capture(scene,captures,hidden,scene.graph.features.indicator); defer gfx.readback_data_destroy(&indicator)
    masked:=0; for byte in indicator.bytes { if byte>127 { masked+=1 } }; assert(masked>25,"occluded selected geometry did not produce the actual stencil indicator mask"); assert(render.native_scene_wait(scene,hidden)==.None)
    assert(render.native_scene_select(scene,nil)=={})
    checked:=0
    for mode in ([4]render.Tonemap_Operator{.ACES,.Reinhard,.Tony_McMapface,.Linear}) {
        scene.feature_settings.postprocess={0.7,mode}
        accepted:=draw(&consumer,frame)
        hdr:=capture(scene,captures,accepted,scene.graph.color); defer gfx.readback_data_destroy(&hdr)
        output:=capture(scene,captures,accepted,scene.graph.output); defer gfx.readback_data_destroy(&output)
        for pixel in 0..<len(output.bytes)/4 { for channel in 0..<3 {
            linear:=half(hdr.bytes,pixel*8+channel*2)*0.7
            expected:=encode(tone(linear,mode))*255
            assert(abs(f32(output.bytes[pixel*4+channel])-expected)<1.6,"final pixels did not match the canonical HDR operator followed by one sRGB transfer"); checked+=1
        } }
        assert(render.native_scene_wait(scene,accepted)==.None)
    }
    fmt.println("Native render features PASS:",backend,"shadow depth",depth_pixels,"point pixels",point_pixels,"shadow pixels",shadow_pixels,"grid",grid_pixels,"sky",sky_pixels,"outline",outline_count,"occluded mask",masked,"tone channels",checked)
    exercise_views(&owner,consumer.batch,renderer,ops,captures,pipelines,backend,particle_ops,compiler)
    exercise_particle_reset(&owner,consumer.batch,renderer,ops,captures,pipelines,backend,particle_ops,compiler)
    _=floor; _=cube; _=sun
}
main :: proc() {
    assert(len(os.args)==3)
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing); context.allocator=mem.tracking_allocator(&tracker)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0); mem.tracking_allocator_destroy(&tracker) }
    _=NS.scoped_autoreleasepool()
    compiler:shader.Compiler; assert(shader.compiler_init(&compiler,os.args[1])==.None); defer { assert(shader.compiler_destroy(&compiler)==.None) }
    shaders,error:=render.surface_shader_compile(&compiler); assert(error==.None); defer render.surface_shader_destroy(&shaders)
    {
        renderer:metal.Renderer; assert(metal.renderer_init(&renderer)==.None); defer { assert(metal.renderer_destroy(&renderer)==.None) }
        ops:=render.GPU_Ops(metal.Renderer){metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.create_texture,metal.destroy_texture,metal.acquire,metal.abort,metal.submit,metal.wait,metal.release_graph_exports,metal.create_pipeline,metal.destroy_pipeline,metal.create_sampler,metal.destroy_sampler}
        exercise(&renderer,ops,Capture(metal.Renderer){metal.graph_texture_source,metal.queue_texture_readback,metal.poll_texture_readback},render.surface_pipelines(&shaders),"metal",render.Particle_GPU_Ops(metal.Renderer){metal.create_pipeline,metal.destroy_pipeline,metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.read_buffer},&compiler)
    }
    {
        renderer:vulkan.Renderer; assert(vulkan.renderer_init(&renderer,validation=true,loader_path=os.args[2])==.None); defer { assert(renderer.validation_errors==0); assert(vulkan.renderer_destroy(&renderer)==.None) }
        ops:=render.GPU_Ops(vulkan.Renderer){vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.create_texture,vulkan.destroy_texture,vulkan.acquire,vulkan.abort,vulkan.submit,vulkan.wait,vulkan.release_graph_exports,vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_sampler,vulkan.destroy_sampler}
        exercise(&renderer,ops,Capture(vulkan.Renderer){vulkan.graph_texture_source,vulkan.queue_texture_readback,vulkan.poll_texture_readback},render.surface_pipelines(&shaders),"vulkan",render.Particle_GPU_Ops(vulkan.Renderer){vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.read_buffer},&compiler)
    }
}
