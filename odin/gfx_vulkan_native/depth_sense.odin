//! Forward and infinite reverse-Z cameras preserve their native depth ordering and clear value.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import km "../math"
import "core:mem"
import "core:math"
import "core:fmt"

run_depth_sense :: proc(r:^gpu.Renderer,vertex_code,fragment_code:[]u32) {
    vertices:=[18]f32{-16,-8,-8,48,-8,-8,-16,24,-8,-4,-2,-2,12,-2,-2,-4,6,-2}
    vertex_desc:=gfx.Buffer_Desc{size=size_of(vertices),usage={.Vertex},memory=.GPU_Private}
    vertex,vertex_error:=gpu.create_buffer_with_data(r,vertex_desc,mem.slice_to_bytes(vertices[:]));assert(vertex_error==.None)
    defer assert(gpu.destroy_buffer(r,vertex)==.None)
    color_desc:=gfx.Texture_Desc{width=16,height=8,depth=1,layers=1,mip_levels=1,format=.RGBA8_Unorm,usage={.Color_Attachment,.Transfer_Source}}
    depth_desc:=gfx.Texture_Desc{width=16,height=8,depth=1,layers=1,mip_levels=1,format=.D32_Float,usage={.Depth_Attachment,.Transfer_Source}}
    color,color_error:=gpu.create_texture(r,color_desc);assert(color_error==.None);defer assert(gpu.destroy_texture(r,color)==.None)
    depth,depth_error:=gpu.create_texture(r,depth_desc);assert(depth_error==.None);defer assert(gpu.destroy_texture(r,depth)==.None)
    output_desc:=gfx.Buffer_Desc{size=1024,usage={.Transfer_Destination,.Readback}}
    output,output_error:=gpu.create_buffer(r,output_desc);assert(output_error==.None);defer assert(gpu.destroy_buffer(r,output)==.None)
    senses:=[2]bool{false,true}
    for reverse in senses {
        compare:=gfx.Compare_Op.Greater if reverse else gfx.Compare_Op.Less
        clear_depth:f64=0 if reverse else 1
        projection:=km.mat4_reverse_z(90,2,0.1) if reverse else km.mat4_perspective(90,2,0.1,100)
        descriptor:=gfx.Graphics_Desc{vertex_entry="main",fragment_entry="main",vertex_spirv=vertex_code,fragment_spirv=fragment_code,
            vertex={attributes={{0,0,0,.Float3}},buffers={{0,12,.Vertex}}},
            buffers={{group=0,slot=0,stages={.Vertex},usage=.Uniform,mode=.Read,minimum_size=64},{group=2,slot=3,stages={.Fragment},usage=.Uniform,mode=.Read,minimum_size=16}},
            colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}},depth={enabled=true,test=true,write=true,compare=compare,format=.D32_Float}}
        pipeline,pipeline_error:=gpu.create_graphics_pipeline(r,descriptor);assert(pipeline_error==.None);defer assert(gpu.destroy_graphics_pipeline(r,pipeline)==.None)
        graph:gfx.Graph;gfx.graph_init(&graph);defer { assert(gpu.release_graph_exports(r,&graph)==.None);gfx.graph_destroy(&graph) }
        geometry,geometry_error:=gfx.graph_buffer(&graph,vertex_desc,true,false);assert(geometry_error==.None)
        destination,destination_error:=gfx.graph_buffer(&graph,output_desc,false,true);assert(destination_error==.None)
        image,image_error:=gfx.graph_image(&graph,color_desc,{},false,true);assert(image_error==.None)
        depth_image,depth_image_error:=gfx.graph_image(&graph,depth_desc,{},false,true);assert(depth_image_error==.None)
        vertex_access:=gfx.Buffer_Access{geometry,{0,vertex_desc.size},.Read,.Vertex}
        color_access:=gfx.Image_Access{image,gfx.image_full_range(color_desc),.Write,.Color_Attachment}
        depth_access:=gfx.Image_Access{depth_image,gfx.image_full_range(depth_desc),.Write,.Depth_Attachment}
        pass,pass_error:=gfx.graph_pass(&graph,"native-camera-depth-order",.Graphics,{vertex_access},images={color_access,depth_access});assert(pass_error==.None)
        colors:=[3][4]f32{{1,0,0,1},{0,1,0,1},{0,0,1,1}}
        constants:[3][1]gfx.Constant_Binding
        operations:[3][1]gfx.Draw_Op
        phases:[3]gfx.Render_Phase
        for i in 0..<3 {
            constants[i][0]={2,3,{.Fragment},.Uniform,mem.slice_to_bytes(colors[i][:])}
            operations[i][0]=gfx.Draw_Vertices{{{0,vertex_access}},3,1,3 if i==1 else 0,0}
            phases[i]={pipeline=pipeline,constants=constants[i][:],scissor={true,0,0,8,8},draws=operations[i][:]}
        }
        packet:=gfx.Render{colors={{color_access,.Clear,.Store,{0,0,0,1}}},depth={true,depth_access,.Clear,.Store,clear_depth,0},constants={{0,0,{.Vertex},.Uniform,mem.slice_to_bytes(projection[:])}},phases=phases[:]}
        assert(gfx.graph_set_packet(&graph,pass,packet)==.None)
        aspects:=[2]gfx.Image_Aspect{.Color,.Depth}
        for aspect,index in aspects {
            source:=depth_image if aspect==.Depth else image
            copy,copy_error:=gfx.graph_pass(&graph,"native-depth-copy" if aspect==.Depth else "native-color-copy",.Transfer,{{destination,{u64(index)*512,512},.Write,.Transfer_Destination}},images={{source,{0,1,0,1,{aspect}},.Read,.Transfer_Source}});assert(copy_error==.None)
            assert(gfx.graph_set_packet(&graph,copy,gfx.Copy_Image_Buffer{source,{width=16,height=8,depth=1,aspect=aspect},destination,u64(index)*512})==.None)
        }
        plan,compile_error:=gfx.graph_compile(&graph);assert(compile_error==.None);defer gfx.compiled_graph_destroy(&plan)
        token,acquire_error:=gpu.acquire(r);assert(acquire_error==.None)
        submission,submit_error,packet_error:=gpu.submit(r,token,&graph,&plan,{{geometry,vertex},{destination,output}},{{image,color},{depth_image,depth}})
        assert(submit_error==.None && packet_error==.None);assert(gpu.wait(r,submission)==.None)
        pixels:[1024]byte;assert(gpu.read_buffer(r,output,0,pixels[:])==.None)
        depths:=mem.slice_data_cast([]f32,pixels[512:])
        expected_near:f32=0.05 if reverse else (100/99.9-10/99.9/2)
        for y in 0..<8 { for x in 0..<16 {
            pixel:=y*16+x
            expected:=[4]byte{0,255,0,255} if x<8 else [4]byte{0,0,0,255}
            for channel in 0..<4 { assert(pixels[pixel*4+channel]==expected[channel],"near geometry must win regardless of submission order") }
            assert(math.abs(depths[pixel]-(expected_near if x<8 else f32(clear_depth)))<0.000001,"GPU depth must equal camera projection or untouched clear")
        } }
    }
    fmt.println("Depth sense: finite Less/clear1 and infinite reverse-Z Greater/clear0, near/far overwrite ordering and physical depth/color readback verified")
}
