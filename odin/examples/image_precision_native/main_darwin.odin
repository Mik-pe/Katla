#+build darwin,arm64
//! Actual precision image decoding, role conversion, native upload, mips and byte readback.
package main
import render "../../app/render"
import image "../../image"
import gfx "../../gfx"
import metal "../../gfx/metal"
import vulkan "../../gfx/vulkan"
import "core:mem"
import "core:os"
import "core:fmt"
import NS "core:sys/darwin/Foundation"

Read :: struct($R:typeid) {read:proc(^R,gfx.Buffer_Handle,u64,[]byte)->gfx.Gpu_Error}
exercise :: proc(renderer:^$R,ops:render.GPU_Ops(R),reads:Read(R),backend:string) {
    fixtures:=[3][]byte{#load("../../image/fixtures/png16_4.png",[]byte),#load("../../image/fixtures/rgba16_be.tiff",[]byte),#load("../../image/fixtures/rgba32f.tiff",[]byte)}
    for encoded,index in fixtures {
        decoded,error:=image.texture_image_decode(encoded);assert(error==.None);defer image.texture_image_destroy(&decoded)
        for color in ([2]bool{false,true}) {
            format,expected,owned,conversion_error:=render.native_texture_samples(&decoded,color);assert(conversion_error=={});defer { if owned { delete(expected) } }
            texture,upload_error:=render.native_texture_upload(renderer,ops,&decoded,color);assert(upload_error=={});defer assert(ops.destroy_texture(renderer,texture.texture)==.None)
            assert(texture.desc.format==format && texture.desc.mip_levels>=1)
            graph:gfx.Graph;gfx.graph_init(&graph);defer { assert(ops.release_exports(renderer,&graph)==.None);gfx.graph_destroy(&graph) }
            source,source_error:=gfx.graph_image(&graph,texture.desc,{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false);assert(source_error==.None)
            buffer_desc:=gfx.Buffer_Desc{size=u64(len(expected)),usage={.Transfer_Destination,.Readback},memory=.CPU_Visible}
            zero:=make([]byte,len(expected)); defer delete(zero)
            buffer,buffer_error:=ops.create_buffer(renderer,buffer_desc,zero);assert(buffer_error==.None);defer assert(ops.destroy_buffer(renderer,buffer)==.None)
            destination,destination_error:=gfx.graph_buffer(&graph,buffer_desc,false,true);assert(destination_error==.None)
            pass,pass_error:=gfx.graph_pass(&graph,"precision-image-byte-readback",.Transfer,{{destination,{0,buffer_desc.size},.Write,.Transfer_Destination}},images={{source,{0,1,0,1,{.Color}},.Read,.Transfer_Source}});assert(pass_error==.None)
            assert(gfx.graph_set_packet(&graph,pass,gfx.Copy_Image_Buffer{source,{width=decoded.width,height=decoded.height,depth=1,aspect=.Color},destination,0})==.None)
            plan,plan_error:=gfx.graph_compile(&graph);assert(plan_error==.None);defer gfx.compiled_graph_destroy(&plan)
            token,acquire_error:=ops.acquire(renderer);assert(acquire_error==.None)
            submission,submit_error,packet_error:=ops.submit(renderer,token,&graph,&plan,{{destination,buffer}},{{source,texture.texture}});assert(submit_error==.None && packet_error==.None)
            assert(ops.wait(renderer,submission)==.None)
            actual:=make([]byte,len(expected));defer delete(actual);assert(reads.read(renderer,buffer,0,actual)==.None)
            assert(mem.compare(actual,expected)==0,fmt.tprintf("%s fixture%d color%v nativeprecision mismatch actual%v expected%v dims%dx%d",backend,index,color,actual,expected,decoded.width,decoded.height))
        }
    }
    fmt.println("Actual precision upload PASS",backend,"PNG16/TIFF16BE exact data + linear color half/alpha; TIFF32 float HDR; real mip generation and native byte readback")
}
main :: proc() {
    assert(len(os.args)==2,"Pass Vulkan loader")
    backing:=context.allocator;tracker:mem.Tracking_Allocator;mem.tracking_allocator_init(&tracker,backing);context.allocator=mem.tracking_allocator(&tracker)
    defer {context.allocator=backing;assert(len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0);mem.tracking_allocator_destroy(&tracker)}
    pool:=NS.AutoreleasePool.alloc()->init();defer pool->drain()
    m:metal.Renderer;assert(metal.renderer_init(&m)==.None)
    mo:=render.GPU_Ops(metal.Renderer){metal.create_graphics_pipeline,metal.destroy_graphics_pipeline,metal.create_buffer_with_data,metal.destroy_buffer,metal.write_buffer,metal.create_texture,metal.destroy_texture,metal.acquire,metal.abort,metal.submit,metal.wait,metal.release_graph_exports,metal.create_pipeline,metal.destroy_pipeline,metal.create_sampler,metal.destroy_sampler}
    exercise(&m,mo,Read(metal.Renderer){metal.read_buffer},"Metal");assert(metal.renderer_destroy(&m)==.None)
    v:vulkan.Renderer;assert(vulkan.renderer_init(&v,validation=true,loader_path=os.args[1])==.None)
    vo:=render.GPU_Ops(vulkan.Renderer){vulkan.create_graphics_pipeline,vulkan.destroy_graphics_pipeline,vulkan.create_buffer_with_data,vulkan.destroy_buffer,vulkan.write_buffer,vulkan.create_texture,vulkan.destroy_texture,vulkan.acquire,vulkan.abort,vulkan.submit,vulkan.wait,vulkan.release_graph_exports,vulkan.create_pipeline,vulkan.destroy_pipeline,vulkan.create_sampler,vulkan.destroy_sampler}
    exercise(&v,vo,Read(vulkan.Renderer){vulkan.read_buffer},"Vulkan");assert(vulkan.validation_error_count(&v)==0);assert(vulkan.renderer_destroy(&v)==.None)
}
