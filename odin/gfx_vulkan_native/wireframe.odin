//! Filled and wireframe pipelines render the same private mesh into adjacent viewports.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import "core:mem"
import "core:fmt"

run_wireframe :: proc(r:^gpu.Renderer,vertex_code,fragment_code:[]u32) {
    desc:=gfx.Graphics_Desc{vertex_entry="main",fragment_entry="main",vertex_spirv=vertex_code,fragment_spirv=fragment_code,vertex={attributes={{0,0,0,.Float2}},buffers={{0,8,.Vertex}}},buffers={{group=2,slot=3,stages={.Fragment},usage=.Uniform,mode=.Read,minimum_size=16}},colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}}}
    solid,solid_error:=gpu.create_graphics_pipeline(r,desc); assert(solid_error==.None)
    defer assert(gpu.destroy_graphics_pipeline(r,solid)==.None)
    desc.wireframe=true
    wire,wire_error:=gpu.create_graphics_pipeline(r,desc); assert(wire_error==.None)
    defer assert(gpu.destroy_graphics_pipeline(r,wire)==.None)
    vertices:=[6]f32{-0.75,-0.75,0.75,-0.75,0,0.75}
    vertex_desc:=gfx.Buffer_Desc{size=24,usage={.Vertex},memory=.GPU_Private}
    vertex,vertex_error:=gpu.create_buffer_with_data(r,vertex_desc,mem.slice_to_bytes(vertices[:])); assert(vertex_error==.None)
    target_desc:=gfx.Texture_Desc{64,32,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    target,target_error:=gpu.create_texture(r,target_desc); assert(target_error==.None)
    output_desc:=gfx.Buffer_Desc{size=8192,usage={.Readback,.Transfer_Destination}}
    output,output_error:=gpu.create_buffer(r,output_desc); assert(output_error==.None)
    defer assert(gpu.destroy_buffer(r,output)==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer { assert(gpu.release_graph_exports(r,&graph)==.None); gfx.graph_destroy(&graph) }
    image,_:=gfx.graph_image(&graph,target_desc,{},false,true)
    vb,_:=gfx.graph_buffer(&graph,vertex_desc,true,false)
    destination,_:=gfx.graph_buffer(&graph,output_desc,false,true)
    geometry:=gfx.Buffer_Access{vb,{0,24},.Read,.Vertex}
    color:=gfx.Image_Access{image,gfx.image_full_range(target_desc),.Write,.Color_Attachment}
    tint:=[4]f32{0,1,0,1}
    draw,_:=gfx.graph_pass(&graph,"solid-versus-wireframe",.Graphics,{geometry},images={color})
    assert(gfx.graph_set_packet(&graph,draw,gfx.Render{colors={{color,.Clear,.Store,{1,0,0,1}}},constants={{2,3,{.Fragment},.Uniform,mem.slice_to_bytes(tint[:])}},phases={{pipeline=solid,viewport={true,0,0,32,32,0,1},draws={gfx.Draw_Vertices{{{0,geometry}},3,1,0,0}}},{pipeline=wire,viewport={true,32,0,32,32,0,1},draws={gfx.Draw_Vertices{{{0,geometry}},3,1,0,0}}}}})==.None)
    copy,_:=gfx.graph_pass(&graph,"wireframe-pixels",.Transfer,{{destination,{0,8192},.Write,.Transfer_Destination}},images={{image,gfx.image_full_range(target_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copy,gfx.Copy_Image_Buffer{image,{0,0,0,0,64,32,.Color,0,1,0,0},destination,0})==.None)
    plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,acquire_error:=gpu.acquire(r); assert(acquire_error==.None)
    submission,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,{{vb,vertex},{destination,output}},{{image,target}})
    assert(native_error==.None && packet_error==.None)
    assert(gpu.destroy_buffer(r,vertex)==.None); assert(gpu.destroy_texture(r,target)==.None)
    assert(gpu.wait(r,submission)==.None)
    data:[8192]byte; assert(gpu.read_buffer(r,output,0,data[:])==.None)
    filled,edges:int
    for y in 0..<32 { for x in 0..<64 {
        offset:=(y*64+x)*4
        assert(data[offset+2]==0 && data[offset+3]==255)
        if data[offset+1]==255 { assert(data[offset]==0); if x<32 { filled+=1 } else { edges+=1 } }
        else { assert(data[offset]==255 && data[offset+1]==0) }
    } }
    left_center:=(16*64+16)*4
    right_center:=(16*64+48)*4
    assert(data[left_center]==0 && data[left_center+1]==255)
    assert(data[right_center]==255 && data[right_center+1]==0)
    assert(edges>=24 && filled>edges*2)
    fmt.println("Wireframe: actual green edge pixels",edges,"versus filled green pixels",filled,"with red wireframe interior and independent viewports verified")
}
