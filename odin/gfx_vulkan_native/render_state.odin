//! Independent stencil, depth writing, bias and color blending produce observable GPU values.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import "core:mem"
import "core:fmt"

run_render_state :: proc(r:^gpu.Renderer,vertex_code,fragment_code:[]u32) {
    color_desc:=gfx.Texture_Desc{16,8,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    depth_desc:=gfx.Texture_Desc{16,8,1,1,.D32_Float_S8_Uint,{.Depth_Attachment,.Transfer_Source},1}
    first_desc:=gfx.Graphics_Desc{vertex_entry="main",fragment_entry="main",vertex_spirv=vertex_code,fragment_spirv=fragment_code,colors={{format=.RGBA8_Unorm}},depth={enabled=true,format=.D32_Float_S8_Uint},stencil={enabled=true,front={compare=.Always,pass=.Replace},back={compare=.Always,pass=.Replace},reference=7,read_mask=255,write_mask=255}}
    first,first_error:=gpu.create_graphics_pipeline(r,first_desc); assert(first_error==.None)
    defer assert(gpu.destroy_graphics_pipeline(r,first)==.None)
    second_desc:=first_desc
    second_desc.colors={{format=.RGBA8_Unorm,write_mask={.Red,.Blue},blend_enabled=true,source_color=.One,destination_color=.One,source_alpha=.One,destination_alpha=.Zero}}
    second_desc.depth.write=true; second_desc.depth.test=false; second_desc.depth_bias.constant=2
    second_desc.stencil.front={compare=.Equal}; second_desc.stencil.back={compare=.Equal}; second_desc.stencil.write_mask=0
    second,second_error:=gpu.create_graphics_pipeline(r,second_desc); assert(second_error==.None)
    defer assert(gpu.destroy_graphics_pipeline(r,second)==.None)
    color,color_error:=gpu.create_texture(r,color_desc); assert(color_error==.None)
    defer assert(gpu.destroy_texture(r,color)==.None)
    depth,depth_error:=gpu.create_texture(r,depth_desc); assert(depth_error==.None)
    defer assert(gpu.destroy_texture(r,depth)==.None)
    output_desc:=gfx.Buffer_Desc{size=1152,usage={.Readback,.Transfer_Destination}}
    output,output_error:=gpu.create_buffer(r,output_desc); assert(output_error==.None)
    defer assert(gpu.destroy_buffer(r,output)==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer { assert(gpu.release_graph_exports(r,&graph)==.None); gfx.graph_destroy(&graph) }
    image,_:=gfx.graph_image(&graph,color_desc,{},false,true)
    depth_image,_:=gfx.graph_image(&graph,depth_desc,{},false,true)
    destination,_:=gfx.graph_buffer(&graph,output_desc,false,true)
    color_write:=gfx.Image_Access{image,gfx.image_full_range(color_desc),.Write,.Color_Attachment}
    depth_write:=gfx.Image_Access{depth_image,gfx.image_full_range(depth_desc),.Write,.Depth_Attachment}
    pass,_:=gfx.graph_pass(&graph,"stencil-blend-depth-write",.Graphics,nil,images={color_write,depth_write})
    assert(gfx.graph_set_packet(&graph,pass,gfx.Render{colors={{color_write,.Clear,.Store,{0.25,0.25,0.25,0}}},depth={true,depth_write,.Clear,.Store,1,0},phases={{pipeline=first,scissor={true,0,0,8,8},draws={gfx.Draw{3,1,0,0}}},{pipeline=second,draws={gfx.Draw{3,1,0,0}}}}})==.None)
    ranges:=[3]gfx.Image_Range{gfx.image_full_range(color_desc),{0,1,0,1,{.Depth}},{0,1,0,1,{.Stencil}}}
    sources:=[3]gfx.Image_Id{image,depth_image,depth_image}
    aspects:=[3]gfx.Image_Aspect{.Color,.Depth,.Stencil}
    offsets:=[3]u64{0,512,1024}
    sizes:=[3]u64{512,512,128}
    names:=[3]string{"state-color","state-depth","state-stencil"}
    for i in 0..<3 {
        copy,_:=gfx.graph_pass(&graph,names[i],.Transfer,{{destination,{offsets[i],sizes[i]},.Write,.Transfer_Destination}},images={{sources[i],ranges[i],.Read,.Transfer_Source}})
        assert(gfx.graph_set_packet(&graph,copy,gfx.Copy_Image_Buffer{sources[i],{0,0,0,0,16,8,aspects[i],0,1,0,0},destination,offsets[i]})==.None)
    }
    plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,acquire_error:=gpu.acquire(r); assert(acquire_error==.None)
    submission,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,{{destination,output}},{{image,color},{depth_image,depth}})
    assert(native_error==.None && packet_error==.None)
    assert(gpu.wait(r,submission)==.None)
    data:[1152]byte; assert(gpu.read_buffer(r,output,0,data[:])==.None)
    depths:=mem.slice_data_cast([]f32,data[512:1024])
    for y in 0..<8 { for x in 0..<16 {
        pixel:=y*16+x
        expected:=[4]byte{64,64,64,0}
        if x<8 { expected={128,64,255,0} }
        for channel in 0..<4 { assert(data[pixel*4+channel]==expected[channel]) }
        assert(data[1024+pixel]==(7 if x<8 else 0))
        if x<8 { assert(depths[pixel]>0.25 && depths[pixel]<0.26) }
        else { assert(depths[pixel]==1) }
    } }
    fmt.println("State: stencil mask, additive blend, channel mask, biased depth write without comparison and independent D32S8 aspect copies verified")
}
