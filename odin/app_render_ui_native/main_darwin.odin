#+build darwin, arm64
//! Native UI acceptance pairs actual glyph/scissor/layer pixels with depth-tested entity IDs.
package main

import gfx "../gfx"
import render "../app/render"
import ui "../ui"
import shader "../gfx/shader"
import metal "../gfx/metal"
import vulkan "../gfx/vulkan"
import ecs "../ecs"
import km "../math"
import NS "core:sys/darwin/Foundation"
import "core:mem"
import "core:os"
import math "core:math"
import "core:time"
import "core:fmt"

API :: struct($R:typeid) {
    ui:render.UI_GPU_Ops(R), pick:render.Picking_Ops(R),
    texture:proc(^R,gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error),
    acquire:proc(^R)->(gfx.Frame_Token,gfx.Gpu_Error),
    abort:proc(^R,gfx.Frame_Token)->gfx.Gpu_Error,
    submit:proc(^R,gfx.Frame_Token,^gfx.Graph,^gfx.Compiled_Graph,[]gfx.Buffer_Input,[]gfx.Texture_Input)->(gfx.Submission,gfx.Gpu_Error,gfx.Packet_Error),
    wait:proc(^R,gfx.Submission)->gfx.Gpu_Error,
    release:proc(^R,^gfx.Graph)->gfx.Gpu_Error,
}
pixel :: proc(snapshot:^render.Picking_Snapshot,x,y:int)->[4]u8 {
    offset:=int(snapshot.color.row_pitch*u64(y))+x*4; return {snapshot.color.bytes[offset],snapshot.color.bytes[offset+1],snapshot.color.bytes[offset+2],snapshot.color.bytes[offset+3]}
}
display_channel :: proc(linear:f32)->u8 { return u8(math.round(clamp(f32(12.92)*linear if linear<=0.0031308 else f32(1.055)*math.pow(linear,f32(1.0/2.4))-f32(0.055),0,1)*255)) }
expect_pixel :: proc(actual,expected:[4]u8) { for value,i in actual { assert(abs(int(value)-int(expected[i]))<=1,fmt.tprintf("actual %v expected %v",actual,expected)) } }
IMAGE_BMP :: #load("../app/render/texture_image_fixtures/rgb.bmp",[]byte)
IMAGE_TIFF :: #load("../app/render/texture_image_fixtures/rgba.tiff",[]byte)
IMAGE_ROTATED :: #load("../app/render/texture_image_fixtures/rotated.tiff",[]byte)
IMAGE_DEFLATE :: #load("../app/render/texture_image_fixtures/deflate.tiff",[]byte)
exercise :: proc(renderer:^$R,api:API(R),compiler:^shader.Compiler,fonts:^render.UI_Font_System,down:bool,name:string) {
    ui_shader,error:=render.ui_shader_compile(compiler,.RGBA8_Unorm); assert(error==.None); defer render.ui_shader_destroy(&ui_shader)
    picking_owner:render.Picking_Native(R)
    picking_operations:=render.GPU_Ops(R){create_pipeline=api.ui.create_pipeline,destroy_pipeline=api.ui.destroy_pipeline}
    assert(render.picking_native_init(&picking_owner,renderer,picking_operations,compiler)=={}); defer assert(render.picking_native_destroy(&picking_owner)==.None)
    pipeline:=picking_owner.pipelines.opaque[1]; masked_pipeline:=picking_owner.pipelines.masked[1]
    owner:render.UI_GPU(R); assert(render.ui_gpu_init(&owner,renderer,api.ui,&ui_shader)==.None); defer assert(render.ui_gpu_destroy(&owner)==.None)
    retained:render.Picking_Snapshot
    defer render.picking_snapshot_destroy(&retained)
    for cycle in 0..<3 {
        width:=u32(128+cycle*16); height:=u32(96+cycle*8)
        if cycle==2 { width=320; height=160 }
        clip:=ui.Rect{0,0,f32(width),f32(height)}
        commands:=[4]ui.Draw_Command{
            ui.Rect_Draw{bounds=clip,clip=clip,color={1,0,0,1}},
            ui.Text_Draw{font=render.UI_FONT_REGULAR,text="Åäö välj dörren",position={4,4},size=20,clip={0,0,64,32},color={1,1,1,1}},
            ui.Rect_Draw{bounds={20,40,20,20},clip={25,40,10,20},color={0,1,0,1}},
            ui.Rect_Draw{bounds={28,44,6,8},clip=clip,color={1,1,0,1}},
        }
        if cycle==1 { first,ok:=commands[0].(ui.Rect_Draw); assert(ok); first.bounds.height-=4; commands[0]=first }
        theme:=ui.theme_default()
        context_ui:ui.Context; assert(ui.context_init(&context_ui,render.ui_font_provider(fonts),theme)==.None); defer ui.context_destroy(&context_ui)
        cell:=ui.state(&context_ui,1,0,string("Åäö välj dörren"))
        editor:=ui.Descriptor{key=1,kind=.Text_Input,state=cell,action=9,font_size=20,has_fixed_bounds=true,fixed_bounds={4,4,60,28},background={1,0,0,1},has_background=true}
        size:ui.Vec2={f32(width),f32(height)}
        _,first_ui:=ui.frame(&context_ui,editor,{},size); assert(first_ui.error==.None); assert(ui.focus(&context_ui,context_ui.nodes[1].id))
        composing_events:=[2]ui.Input_Event{ui.Key_Down{key=.End},ui.IME_Preedit{text="é",cursor=3}}
        _,composing_result:=ui.frame(&context_ui,editor,{events=composing_events[:]},size); assert(composing_result.error==.None && composing_result.ime.active)
        commit_events:=[3]ui.Input_Event{ui.Text_Commit{text="é"},ui.Key_Down{key=.Backspace},ui.Text_Commit{text="ä😊"}}
        _,commit_result:=ui.frame(&context_ui,editor,{events=commit_events[:]},size); assert(commit_result.error==.None)
        entered:=[1]ui.Input_Event{ui.Key_Down{key=.Enter}}
        ui_draw,ui_result:=ui.frame(&context_ui,editor,{events=entered[:]},size); assert(ui_result.error==.None)
        actual_text,text_live:=ui.state_get(&context_ui,cell,string); assert(text_live && actual_text=="Åäö välj dörrenä😊","actual provider grapheme deletion/preedit commit diverged")
        text_actions:=ui.actions_drain(&context_ui,ui.Text_Action); defer delete(text_actions); assert(len(text_actions)>0 && text_actions[len(text_actions)-1].submitted)
        provider:=render.ui_font_provider(fonts)
        bidi_text:="שלום"; visual_right:=provider.navigate(provider.state,render.UI_FONT_REGULAR,bidi_text,20,0,len(bidi_text),1)
        assert(visual_right<len(bidi_text),"RTL visual arrow must move toward a preceding logical character")
        layered:=make([dynamic]ui.Draw_Command); append(&layered,..commands[:]); append(&layered,..ui_draw.commands); defer delete(layered)
        controls:ui.Context; assert(ui.context_init(&controls,provider)==.None); defer ui.context_destroy(&controls)
        if cycle==2 {
            rows:=[]ui.Descriptor{
                {key=21,kind=.Text,text="Åäö 👩‍💻 väldigt långt materialnamn.png",text_max_width=152,font_size=14,has_fixed_bounds=true,fixed_bounds={160,4,152,24}},
                {key=22,kind=.Slider,text="Scale",value=-0.25,minimum=-1,maximum=1,has_fixed_bounds=true,fixed_bounds={160,34,152,30}},
            }
            control_draw,control_result:=ui.frame(&controls,{key=20,kind=.Stack,children=rows},{},{f32(width),f32(height)})
            assert(control_result.error==.None)
            short_found,value_found:=false,false
            for command in control_draw.commands {
                if text,ok:=command.(ui.Text_Draw);ok {
                    if text.position.y==4 { short_found=true; assert(len(text.text)>=3 && text.text[len(text.text)-3:]=="…" && provider.measure(provider.state,text.font,text.text,text.size,0).x<=152,"native shaped ellipsis width diverged") }
                    if text.text=="-0.25" { value_found=true }
                }
            }
            assert(short_found && value_found,"native retained numeric/ellipsis controls missing")
            append(&layered,..control_draw.commands)
        }
        registered:=make([dynamic]gfx.Texture_Handle); defer delete(registered)
        sample_desc:=gfx.Texture_Desc{width=1,height=1,depth=1,layers=1,mip_levels=1,format=.RGBA8_Unorm,usage={.Sampled,.Transfer_Destination}}
        for index in 0..<66 {
            bytes:=[4]byte{0,255,255,255}; if index==65 { bytes={255,0,255,255} }
            handle,sample_error:=api.ui.create_texture(renderer,sample_desc,bytes[:]); assert(sample_error==.None)
            append(&registered,handle)
            texture_id:=ui.Texture_Id(index+2); assert(render.ui_gpu_texture(&owner,texture_id,{handle=handle,desc=sample_desc})==.None)
            append(&layered,ui.Image_Draw{texture=texture_id,bounds={120,70,4,4},uv={0,0,1,1},clip=clip,tint={1,1,1,1}})
        }
        append(&layered,ui.Rect_Draw{bounds={44,40,8,8},clip=clip,color={1,1,1,0.5}})
        encoded:=[4]byte{64,128,192,255}
        for srgb in 0..<2 {
            encoded_desc:=sample_desc; if srgb==1 { encoded_desc.format=.RGBA8_Srgb }
            handle,upload_error:=api.ui.create_texture(renderer,encoded_desc,encoded[:]); assert(upload_error==.None); append(&registered,handle)
            identity:=ui.Texture_Id(90+srgb); assert(render.ui_gpu_texture(&owner,identity,{handle=handle,desc=encoded_desc})==.None)
            append(&layered,ui.Image_Draw{texture=identity,bounds={f32(112+srgb*6),44,4,4},uv={0,0,1,1},clip=clip,tint={1,1,1,1}})
        }
        expected_images:[4][16]byte
        for source,i in ([]([]byte){IMAGE_BMP,IMAGE_TIFF,IMAGE_ROTATED,IMAGE_DEFLATE}) {
            decoded,decode_error:=render.texture_image_decode(source); assert(decode_error==.None && decoded.width==2 && decoded.height==2); copy(expected_images[i][:],decoded.pixels)
            desc:=sample_desc; desc.width=2; desc.height=2
            handle,upload_error:=api.ui.create_texture(renderer,desc,decoded.pixels); render.texture_image_destroy(&decoded); assert(upload_error==.None); append(&registered,handle)
            identity:=ui.Texture_Id(100+i); assert(render.ui_gpu_texture(&owner,identity,{handle=handle,desc=desc})==.None)
            append(&layered,ui.Image_Draw{texture=identity,bounds={f32(68+i*4),64,2,2},uv={0,0,1,1},clip=clip,tint={1,1,1,1}})
        }
        append(&layered,ui.Text_Draw{font=render.UI_FONT_ICONS,text="",position={80,36},size=18,clip=clip,color={0,1,1,1}},ui.Text_Draw{font=render.UI_FONT_REGULAR,text="שלום العربية 你好 👩‍💻",position={4,72},size=14,clip=clip,color={1,1,1,1}})
        if cycle==1 { append(&layered,ui.Text_Draw{font=render.UI_FONT_REGULAR,text="亜",position={100,48},size=14,clip=clip,color={1,1,1,1}}) }
        mesh,mesh_error:=render.ui_prepare(fonts,{layered[:],{f32(width),f32(height)},1}); assert(mesh_error==.None); defer render.ui_mesh_destroy(&mesh)
        token,acquire_error:=api.acquire(renderer); assert(acquire_error==.None)
        prepared,prepare_error:=render.ui_gpu_prepare(&owner,fonts,&mesh); assert(prepare_error==.None)
        graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
        color_desc:=gfx.Texture_Desc{width=width,height=height,depth=1,layers=1,mip_levels=1,format=.RGBA8_Unorm,usage={.Color_Attachment,.Transfer_Source}}
        if cycle==1 { color_desc.usage+= {.Sampled} }
        id_desc:=color_desc; id_desc.format=.R32_Uint
        depth_desc:=color_desc; depth_desc.format=.D32_Float; depth_desc.usage={.Depth_Attachment}
        color,_:=gfx.graph_image(&graph,color_desc,{},false,true)
        id,_:=gfx.graph_image(&graph,id_desc,{},false,true)
        depth,_:=gfx.graph_image(&graph,depth_desc,{},false,false)
        color_handle,color_error:=api.texture(renderer,color_desc); assert(color_error==.None)
        id_handle,id_error:=api.texture(renderer,id_desc); assert(id_error==.None)
        depth_handle,depth_error:=api.texture(renderer,depth_desc); assert(depth_error==.None)
        if cycle==1 {
            access:=gfx.Image_Access{color,gfx.image_full_range(color_desc),.Write,.Color_Attachment}
            accesses:=[1]gfx.Image_Access{access}
            colors:=[1]gfx.Color_Attachment{{access,.Clear,.Store,{0.5,0.25,0.75,1}}}
            initial,initial_error:=gfx.graph_pass(&graph,"Existing encoded target",.Graphics,nil,images=accesses[:]); assert(initial_error==.None)
            assert(gfx.graph_set_packet(&graph,initial,gfx.Render{colors=colors[:]})==.None)
        }
        geometry:=[6]render.Vertex{
            {position={-0.9,-0.9,0.6,1}}, {position={0.9,-0.9,0.6,1}}, {position={0,0.9,0.6,1}},
            {position={-0.6,-0.6,0.2,1}}, {position={0.6,-0.6,0.2,1}}, {position={0,0.6,0.2,1}},
        }
        objects:=[2]render.Object_Data{{model=km.identity(km.Mat4),linear_color={1,1,1,1}},{model=km.identity(km.Mat4),linear_color={1,1,1,1}}}
        camera:=render.Frame_Data{view_projection=km.identity(km.Mat4),ambient={0,0,0,1 if down else -1}}
        camera_data:=[1]render.Frame_Data{camera}
        geometry_desc:=gfx.Buffer_Desc{size=u64(size_of(geometry)),usage={.Storage,.Transfer_Destination},memory=.GPU_Private}
        object_desc:=gfx.Buffer_Desc{size=u64(size_of(objects)),usage={.Storage,.Transfer_Destination},memory=.GPU_Private}
        frame_desc:=gfx.Buffer_Desc{size=u64(size_of(camera)),usage={.Uniform,.Transfer_Destination},memory=.GPU_Private}
        geometry_handle,geometry_error:=api.ui.create_buffer(renderer,geometry_desc,mem.slice_to_bytes(geometry[:])); assert(geometry_error==.None)
        object_handle,object_error:=api.ui.create_buffer(renderer,object_desc,mem.slice_to_bytes(objects[:])); assert(object_error==.None)
        frame_handle,frame_error:=api.ui.create_buffer(renderer,frame_desc,mem.slice_to_bytes(camera_data[:])); assert(frame_error==.None)
        geometry_id,_:=gfx.graph_buffer(&graph,geometry_desc,true,false); object_id,_:=gfx.graph_buffer(&graph,object_desc,true,false); frame_id,_:=gfx.graph_buffer(&graph,frame_desc,true,false)
        draws:=[2]render.Picking_Draw{
            {geometry={geometry_id,geometry_handle,geometry_desc},objects={object_id,object_handle,object_desc},vertex_stride=u32(size_of(render.Vertex)),object_stride=u32(size_of(render.Object_Data)),first_vertex=0,vertex_count=3,object_index=0,encoded=19,entity=ecs.Entity_Id(0x100000003)},
            {geometry={geometry_id,geometry_handle,geometry_desc},objects={object_id,object_handle,object_desc},vertex_stride=u32(size_of(render.Vertex)),object_stride=u32(size_of(render.Object_Data)),first_vertex=3,vertex_count=3,object_index=1,encoded=7,entity=ecs.Entity_Id(0x20000000a)},
        }
        transparent:=[4]byte{255,255,255,0}
        mask_handle,mask_error:=api.ui.create_texture(renderer,sample_desc,transparent[:]); assert(mask_error==.None)
        if cycle==1 { draws[1].pipeline=masked_pipeline; draws[1].mask={enabled=true,texture={handle=mask_handle,desc=sample_desc},sampler=owner.sampler,uv_offset=32,object_alpha_offset=140,cutoff=0.5} }
        picking,picking_error:=render.picking_graph_append(&graph,pipeline,{frame_id,frame_handle,frame_desc},draws[:],id,depth); assert(picking_error=={}); defer render.picking_graph_input_destroy(&picking)
        composition,composition_error:=render.ui_graph_append(&graph,&owner,&prepared,&mesh,color,color_desc,cycle!=1,down); assert(composition_error=={}); defer render.ui_graph_input_destroy(&composition)
        buffers:=make([dynamic]gfx.Buffer_Input); append(&buffers,..picking.buffers); append(&buffers,..composition.buffers); defer delete(buffers)
        textures:=make([dynamic]gfx.Texture_Input); append(&textures,gfx.Texture_Input{color,color_handle},gfx.Texture_Input{id,id_handle},gfx.Texture_Input{depth,depth_handle}); append(&textures,..composition.textures); append(&textures,..picking.textures); defer delete(textures)
        plan,plan_error:=gfx.graph_compile(&graph); assert(plan_error==.None); defer gfx.compiled_graph_destroy(&plan)
        submission,submit_error,packet_error:=api.submit(renderer,token,&graph,&plan,buffers[:],textures[:]); assert(submit_error==.None && packet_error==.None)
        capture,capture_error:=render.picking_queue(renderer,api.pick,submission,color,id,{frame=u64(cycle+1),serial=42,width=width,height=height,pointer={i32(width/2),i32(height/2)},has_pointer=true},picking.entries); assert(capture_error==.None)
        assert(render.ui_gpu_frame_destroy(&owner,&prepared)==.None)
        assert(api.ui.destroy_buffer(renderer,geometry_handle)==.None && api.ui.destroy_buffer(renderer,object_handle)==.None && api.ui.destroy_buffer(renderer,frame_handle)==.None)
        assert(api.ui.destroy_texture(renderer,color_handle)==.None && api.ui.destroy_texture(renderer,id_handle)==.None && api.ui.destroy_texture(renderer,depth_handle)==.None)
        assert(api.ui.destroy_texture(renderer,mask_handle)==.None)
        for handle in registered { assert(api.ui.destroy_texture(renderer,handle)==.None) }
        assert(api.release(renderer,&graph)==.None)
        assert(gfx.graph_truncate(&graph,0,0,0)==.None)
        stale_token,stale_acquire:=api.acquire(renderer); assert(stale_acquire==.None)
        rejected,reject_error,reject_packet:=api.submit(renderer,stale_token,&graph,&plan,buffers[:],textures[:]); assert(rejected.owner==nil && reject_error!=.None && reject_packet==.Invalid_Plan)
        assert(api.abort(renderer,stale_token)==.None)
        assert(api.wait(renderer,submission)==.None)
        snapshot:render.Picking_Snapshot
        complete:=false
        for _ in 0..<10000 {
            poll_error:gfx.Gpu_Error
            snapshot,complete,poll_error=render.picking_poll(renderer,api.pick,&capture); assert(poll_error==.None)
            if complete { break }; time.sleep(time.Millisecond)
        }
        assert(complete,"paired capture completion timed out")
        foreground:=render.picking_sample(&snapshot,i32(width/2),i32(height/2)); assert(foreground.mapped && foreground.encoded==(19 if cycle==1 else 7) && foreground.entity==draws[0 if cycle==1 else 1].entity)
        assert(render.picking_sample(&snapshot,0,0).encoded==0)
        assert(pixel(&snapshot,22,42)==[4]u8{255,0,0,255})
        assert(pixel(&snapshot,26,42)==[4]u8{0,255,0,255})
        assert(pixel(&snapshot,30,46)==[4]u8{255,255,0,255})
        assert(pixel(&snapshot,36,42)==[4]u8{255,0,0,255})
        assert(pixel(&snapshot,121,71)==[4]u8{255,0,255,255},"ordered UI image passes lost last texture beyond64unique IDs")
        if cycle==1 { expect_pixel(pixel(&snapshot,int(width)-2,int(height)-2),{128,64,191,255}) }
        expect_pixel(pixel(&snapshot,46,42),{255,188,188,255})
        expect_pixel(pixel(&snapshot,113,45),{64,128,192,255})
        expect_pixel(pixel(&snapshot,119,45),{64,128,192,255})
        for image,i in expected_images {
            for texel in 0..<4 {
                alpha:=f32(image[texel*4+3])/255
                expected:=[4]u8{display_channel((1-alpha)+alpha*f32(image[texel*4])/255),display_channel(alpha*f32(image[texel*4+1])/255),display_channel(alpha*f32(image[texel*4+2])/255),255}
                expect_pixel(pixel(&snapshot,68+i*4+texel%2,64+texel/2),expected)
            }
        }
        unicode_pixels:=0
        for y in 74..<min(int(height),96) { for x in 0..<116 { rgba:=pixel(&snapshot,x,y); if rgba[1]>32 && rgba[2]>32 { unicode_pixels+=1 } } }
        assert(unicode_pixels>60,"actual Unicode fallback glyph pixels missing")
        glyph_pixels:=0
        for y in 0..<32 { for x in 0..<64 { rgba:=pixel(&snapshot,x,y); if rgba[1]>32 && rgba[2]>32 { glyph_pixels+=1 } } }
        assert(glyph_pixels>100,"actual Swedish glyph raster coverage missing")
        for y in 0..<32 { assert(pixel(&snapshot,65,y)==[4]u8{255,0,0,255},"glyph clipping leaked") }
        if cycle==2 {
            label_pixels,value_pixels:=0,0
            for y in 4..<28 { for x in 160..<312 { rgba:=pixel(&snapshot,x,y); if rgba[1]>100 && rgba[2]>100 { label_pixels+=1 } } }
            for y in 34..<64 { for x in 249..<312 { rgba:=pixel(&snapshot,x,y); if rgba[1]>100 && rgba[2]>100 { value_pixels+=1 } } }
            assert(label_pixels>40 && value_pixels>20,"actual shaped truncated label or numeric slider pixels absent")
            fmt.printf("%s native retained ellipsis and actual numeric slider pixels %d/%d PASS\n",name,label_pixels,value_pixels)
        }
        if cycle==0 { retained=snapshot } else {
            assert(render.picking_sample(&retained,64,48).entity==draws[1].entity && retained.metadata.width==128 && retained.metadata.frame==1)
            assert(pixel(&retained,30,46)==[4]u8{255,255,0,255},"older paired snapshot changed after nextframe resize")
            render.picking_snapshot_destroy(&snapshot)
        }
        fmt.printf("%s UI/picking %dx%d actual Swedish glyph pixels %d; clip/layer/linearblend/displaypassthrough/BMP-TIFF/foremost entity and retained same-submission pixels PASS\n",name,width,height,glyph_pixels)
    }
}
main :: proc() {
    assert(len(os.args)==6,"Pass shader compiler, font library and resources directory, explicit Vulkan loader and --metal/--vulkan/--both")
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing); context.allocator=mem.tracking_allocator(&tracker)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0,"UI/picking owner leaked allocations"); mem.tracking_allocator_destroy(&tracker) }
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    compiler:shader.Compiler; assert(shader.compiler_init(&compiler,os.args[1])==.None); defer assert(shader.compiler_destroy(&compiler)==.None)
    fonts:render.UI_Font_System; assert(render.ui_font_init(&fonts,os.args[2],os.args[3])==.None); defer render.ui_font_destroy(&fonts)
    backend:=os.args[5]; assert(backend=="--metal" || backend=="--vulkan" || backend=="--both")
    if backend=="--metal" || backend=="--both" {
    m:metal.Renderer; assert(metal.renderer_init(&m)==.None)
    ma:=API(metal.Renderer){ui={metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.create_texture_with_data,metal.destroy_texture,metal.create_sampler,metal.destroy_sampler,metal.create_texture},pick={metal.graph_texture_source,metal.queue_texture_readback,metal.poll_texture_readback,metal.destroy_readback},texture=metal.create_texture,acquire=metal.acquire,abort=metal.abort,submit=metal.submit,wait=metal.wait,release=metal.release_graph_exports}
    exercise(&m,ma,&compiler,&fonts,false,"Metal"); assert(metal.renderer_destroy(&m)==.None)
    }
    if backend=="--vulkan" || backend=="--both" {
    v:vulkan.Renderer; assert(vulkan.renderer_init(&v,validation=true,loader_path=os.args[4])==.None)
    va:=API(vulkan.Renderer){ui={vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.create_texture_with_data,vulkan.destroy_texture,vulkan.create_sampler,vulkan.destroy_sampler,vulkan.create_texture},pick={vulkan.graph_texture_source,vulkan.queue_texture_readback,vulkan.poll_texture_readback,vulkan.destroy_readback},texture=vulkan.create_texture,acquire=vulkan.acquire,abort=vulkan.abort,submit=vulkan.submit,wait=vulkan.wait,release=vulkan.release_graph_exports}
    exercise(&v,va,&compiler,&fonts,true,"Vulkan"); assert(vulkan.validation_error_count(&v)==0); assert(vulkan.renderer_destroy(&v)==.None)
    }
}
