#+build darwin, arm64
//! Native depth/stencil, blending and wireframe state are accepted only with rendered evidence.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:testing"
import "core:fmt"
import "core:time"

@(test)
test_native_depth_stencil_blend_wireframe :: proc(t:^testing.T) {
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    r:Renderer; assert(renderer_init(&r)==.None); defer { assert(renderer_destroy(&r)==.None) }
    source:=`#include <metal_stdlib>
using namespace metal;
vertex float4 near_vertex(uint i [[vertex_id]]) { const float2 p[3]={float2(-1,-1),float2(3,-1),float2(-1,3)}; return float4(p[i],.25,1); }
vertex float4 far_vertex(uint i [[vertex_id]]) { const float2 p[3]={float2(-1,-1),float2(3,-1),float2(-1,3)}; return float4(p[i],.75,1); }
vertex float4 wire_vertex(uint i [[vertex_id]]) { const float2 p[3]={float2(-.5,-.5),float2(.5,-.5),float2(0,.5)}; return float4(p[i],.75,1); }
fragment float4 red() { return float4(1,0,0,1); }
fragment float4 green() { return float4(0,1,0,1); }
fragment float4 blue() { return float4(0,0,1,.5); }`
    descriptor:=gfx.Graphics_Desc{vertex_entry="near_vertex",fragment_entry="red",vertex_metal_entry="near_vertex",fragment_metal_entry="red",vertex_metal_source=source,fragment_metal_source=source,vertex_sizes_index=-1,fragment_sizes_index=-1,colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}},depth={enabled=true,test=true,write=true,compare=.Less,format=.D32_Float_S8_Uint},stencil={enabled=true,front={compare=.Always,pass=.Replace},back={compare=.Always,pass=.Replace},reference=3,read_mask=255,write_mask=255},front_counter_clockwise=true,depth_bias={1e8,0,.05}}
    pipelines:[6]gfx.Graphics_Pipeline_Handle
    pipelines[0],_=create_graphics_pipeline(&r,descriptor); assert(pipelines[0].owner!=nil)
    descriptor.vertex_entry="far_vertex"; descriptor.vertex_metal_entry="far_vertex"; descriptor.fragment_entry="green"; descriptor.fragment_metal_entry="green"; descriptor.depth.write=false; descriptor.stencil.front.pass=.Keep; descriptor.stencil.back.pass=.Keep
    pipelines[1],_=create_graphics_pipeline(&r,descriptor); assert(pipelines[1].owner!=nil)
    descriptor.depth.test=false; descriptor.stencil.front.compare=.Equal; descriptor.stencil.back.compare=.Equal; descriptor.stencil.reference=2
    pipelines[2],_=create_graphics_pipeline(&r,descriptor); assert(pipelines[2].owner!=nil)
    descriptor.stencil.reference=3; descriptor.fragment_entry="blue"; descriptor.fragment_metal_entry="blue"; descriptor.colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue},blend_enabled=true,source_color=.Source_Alpha,destination_color=.One_Minus_Source_Alpha,source_alpha=.One,destination_alpha=.Zero}}; descriptor.depth_bias={.01,1,.05}
    pipelines[3],_=create_graphics_pipeline(&r,descriptor); assert(pipelines[3].owner!=nil)
    descriptor.vertex_entry="wire_vertex"; descriptor.vertex_metal_entry="wire_vertex"; descriptor.fragment_entry="green"; descriptor.fragment_metal_entry="green"; descriptor.wireframe=true; descriptor.colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}}
    pipelines[4],_=create_graphics_pipeline(&r,descriptor); assert(pipelines[4].owner!=nil)
    descriptor.wireframe=false; descriptor.vertex_entry="far_vertex"; descriptor.vertex_metal_entry="far_vertex"; descriptor.depth.write=true; descriptor.depth_bias={}; descriptor.colors={{format=.RGBA8_Unorm}}
    pipelines[5],_=create_graphics_pipeline(&r,descriptor); assert(pipelines[5].owner!=nil)
    color_desc:=gfx.Texture_Desc{32,16,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    depth_desc:=gfx.Texture_Desc{32,16,1,1,.D32_Float_S8_Uint,{.Depth_Attachment,.Transfer_Source},1}
    color,_:=create_texture(&r,color_desc); depth,_:=create_texture(&r,depth_desc)
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    image,_:=gfx.graph_image(&graph,color_desc,{},false,false); z,_:=gfx.graph_image(&graph,depth_desc,{},false,true)
    output_desc:=gfx.Buffer_Desc{4096,{.Transfer_Destination,.Readback},.CPU_Visible}; output,_:=create_buffer(&r,output_desc); destination,_:=gfx.graph_buffer(&graph,output_desc,false,true)
    ca:=gfx.Image_Access{image,gfx.image_full_range(color_desc),.Write,.Color_Attachment}; da:=gfx.Image_Access{z,gfx.image_full_range(depth_desc),.Write,.Depth_Attachment}
    phases:[6]gfx.Render_Phase; draws:[6][1]gfx.Draw_Op
    for pipeline,i in pipelines { draws[i][0]=gfx.Draw{vertex_count=3,instance_count=1}; phases[i]={pipeline=pipeline,draws=draws[i][:]} }
    phases[3].scissor={true,0,0,16,16}; phases[4].scissor={true,16,0,16,16}; phases[5].scissor={true,31,0,1,16}
    render,_:=gfx.graph_pass(&graph,"native-state-phases",.Graphics,nil,images={ca,da})
    assert(gfx.graph_set_packet(&graph,render,gfx.Render{colors={{ca,.Clear,.Store,{0,0,0,1}}},depth={true,da,.Clear,.Store,1,0},phases=phases[:]})==.None)
    copy_color,_:=gfx.graph_pass(&graph,"state-color-copy",.Transfer,{{destination,{0,2048},.Write,.Transfer_Destination}},images={{image,gfx.image_full_range(color_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copy_color,gfx.Copy_Image_Buffer{image,{0,0,0,0,32,16,.Color,0,1,0,0},destination,0})==.None)
    copy_depth,_:=gfx.graph_pass(&graph,"state-depth-copy",.Transfer,{{destination,{2048,2048},.Write,.Transfer_Destination}},images={{z,{0,1,0,1,{.Depth}},.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copy_depth,gfx.Copy_Image_Buffer{z,{0,0,0,0,32,16,.Depth,0,1,0,0},destination,2048})==.None)
    plan,plan_error:=gfx.graph_compile(&graph); assert(plan_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,_:=acquire(&r); submission,native_error,packet_error:=submit(&r,token,&graph,&plan,{{destination,output}},{{image,color},{z,depth}}); assert(native_error==.None && packet_error==.None)
    assert(wait(&r,submission)==.None)
    captured:[4096]byte; assert(read_buffer(&r,output,0,captured[:])==.None)
    green_pixels:=0
    for y in 0..<16 { for x in 0..<32 {
        i:=(y*32+x)*4
        if x<16 { assert(captured[i]>=127 && captured[i]<=128 && captured[i+1]==0 && captured[i+2]>=127 && captured[i+2]<=128 && captured[i+3]==255) }
        else { if captured[i+1]==255 { green_pixels+=1 } else { assert(captured[i]==255 && captured[i+1]==0 && captured[i+2]==0 && captured[i+3]==255) } }
        bias_depth:=(cast(^f32)&captured[2048+i])^
        if x==31 { assert(bias_depth==.75) } else { assert(bias_depth>.29999 && bias_depth<.30001) }
    } }
    assert(green_pixels>0 && green_pixels<64)
    assert(captured[(7*32+18)*4]==255 && captured[(7*32+18)*4+1]==0)
    source_image,source_error:=graph_texture_source(&r,submission,z); assert(source_error==.None)
    ticket,ticket_error:=queue_texture_readback(&r,source_image,{0,0,0,0,32,16,.Stencil,0,1,0,0}); assert(ticket_error==.None)
    assert(release_graph_exports(&r,&graph)==.None)
    complete:=false
    for _ in 0..<5000 {
        data,done,readback_error:=poll_texture_readback(&r,ticket); assert(readback_error==.None)
        if done { assert(len(data.bytes)==512 && data.row_pitch==32); for stencil in data.bytes { assert(stencil==3) }; gfx.readback_data_destroy(&data); complete=true; break }
        time.sleep(time.Millisecond)
    }
    assert(complete)
    for pipeline in pipelines { assert(destroy_graphics_pipeline(&r,pipeline)==.None) }
    assert(destroy_texture(&r,color)==.None && destroy_texture(&r,depth)==.None && destroy_buffer(&r,output)==.None)
    fmt.println("Metal 4 graphics state: 512 D32S8 depth/stencil values, clamped depth bias, depth/stencil rejection, disabled-test depth writes, blending/channel mask and wireframe interior/edges verified")
}
