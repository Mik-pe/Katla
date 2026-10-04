//! Packed joint indices and normalized skin weights are fetched by native vertex attributes.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import "core:mem"
import "core:fmt"

run_vertex_formats :: proc(r:^gpu.Renderer,vertex_code,fragment_code:[]u32) {
    desc:=gfx.Graphics_Desc{vertex_entry="main",fragment_entry="main",vertex_spirv=vertex_code,fragment_spirv=fragment_code,
        vertex={attributes={{0,0,0,.Uint8x4},{1,0,4,.Uint16x4},{2,0,12,.Unorm16x4}},buffers={{0,20,.Vertex}}},
        colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}}}
    pipeline,error:=gpu.create_graphics_pipeline(r,desc); assert(error==.None)
    source:[60]byte
    words:=[8]u16{0,256,32768,65535,0,16384,32768,65535}
    for i in 0..<3 { base:=i*20; copy(source[base:base+4],([]byte{0,1,127,255}));copy(source[base+4:base+20],mem.slice_to_bytes(words[:])) }
    vertex_desc:=gfx.Buffer_Desc{size=60,usage={.Vertex},memory=.GPU_Private}
    vertex,vertex_error:=gpu.create_buffer_with_data(r,vertex_desc,source[:]);assert(vertex_error==.None)
    source={}
    target_desc:=gfx.Texture_Desc{width=8,height=8,depth=1,layers=1,mip_levels=1,format=.RGBA8_Unorm,usage={.Color_Attachment,.Transfer_Source}}
    target,target_error:=gpu.create_texture(r,target_desc);assert(target_error==.None)
    output_desc:=gfx.Buffer_Desc{size=256,usage={.Readback,.Transfer_Destination}}
    output,output_error:=gpu.create_buffer(r,output_desc);assert(output_error==.None);defer assert(gpu.destroy_buffer(r,output)==.None)
    graph:gfx.Graph;gfx.graph_init(&graph);defer { assert(gpu.release_graph_exports(r,&graph)==.None);gfx.graph_destroy(&graph) }
    vb,_:=gfx.graph_buffer(&graph,vertex_desc,true,false); image,_:=gfx.graph_image(&graph,target_desc,{},false,true); bytes,_:=gfx.graph_buffer(&graph,output_desc,false,true)
    geometry:=gfx.Buffer_Access{vb,{0,60},.Read,.Vertex};color:=gfx.Image_Access{image,gfx.image_full_range(target_desc),.Write,.Color_Attachment}
    draw,_:=gfx.graph_pass(&graph,"packed-joints-and-weights",.Graphics,{geometry},images={color})
    assert(gfx.graph_set_packet(&graph,draw,gfx.Render{colors={{color,.Clear,.Store,{0,0,0,1}}},phases={{pipeline=pipeline,draws={gfx.Draw_Vertices{{{0,geometry}},3,1,0,0}}}}})==.None)
    copy_pass,_:=gfx.graph_pass(&graph,"packed-vertex-pixels",.Transfer,{{bytes,{0,256},.Write,.Transfer_Destination}},images={{image,gfx.image_full_range(target_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copy_pass,gfx.Copy_Image_Buffer{image,{0,0,0,0,8,8,.Color,0,1,0,0},bytes,0})==.None)
    plan,compile_error:=gfx.graph_compile(&graph);assert(compile_error==.None);defer gfx.compiled_graph_destroy(&plan)
    token,acquired:=gpu.acquire(r);assert(acquired==.None)
    submission,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,{{vb,vertex},{bytes,output}},{{image,target}});assert(native_error==.None&&packet_error==.None)
    assert(gpu.destroy_buffer(r,vertex)==.None);assert(gpu.destroy_texture(r,target)==.None);assert(gpu.destroy_graphics_pipeline(r,pipeline)==.None)
    assert(gpu.wait(r,submission)==.None)
    pixels:[256]byte;assert(gpu.read_buffer(r,output,0,pixels[:])==.None)
    for i in 0..<64 { assert(pixels[i*4]==64&&pixels[i*4+1]==128&&pixels[i*4+2]==191&&pixels[i*4+3]==255) }
    fmt.println("Vertex formats: native Uint8x4/Uint16x4 exact joint values and Unorm16x4 normalized weights verified by64 raster pixels after pending handle removal")
}
