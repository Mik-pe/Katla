//! Four geometry paths preserve phase constants and target-local scissors on the GPU.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import "core:mem"
import "core:fmt"

run_mesh :: proc(renderer:^gpu.Renderer,vertex_code,fragment_code:[]u32) {
    desc:=gfx.Graphics_Desc{vertex_entry="main",fragment_entry="main",vertex_spirv=vertex_code,fragment_spirv=fragment_code,vertex={attributes={{0,0,0,.Float2}},buffers={{0,8,.Vertex}}},buffers={{group=2,slot=3,stages={.Fragment},usage=.Uniform,mode=.Read,minimum_size=16}},colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}}}
    pipeline,pipeline_error:=gpu.create_graphics_pipeline(renderer,desc); assert(pipeline_error==.None)
    defer assert(gpu.destroy_graphics_pipeline(renderer,pipeline)==.None)
    vertices:=[6]f32{-1,-1,3,-1,-1,3}
    indices:=[3]u32{0,1,2}
    indirect:=[4]u32{3,1,0,0}
    indexed_indirect:=[5]u32{3,1,0,0,0}
    vertex,vertex_error:=gpu.create_buffer_with_data(renderer,{size=24,usage={.Vertex},memory=.GPU_Private},mem.slice_to_bytes(vertices[:])); assert(vertex_error==.None)
    index,index_error:=gpu.create_buffer_with_data(renderer,{size=12,usage={.Index},memory=.GPU_Private},mem.slice_to_bytes(indices[:])); assert(index_error==.None)
    command,command_error:=gpu.create_buffer_with_data(renderer,{size=16,usage={.Indirect},memory=.GPU_Private},mem.slice_to_bytes(indirect[:])); assert(command_error==.None)
    indexed_command,indexed_error:=gpu.create_buffer_with_data(renderer,{size=20,usage={.Indirect},memory=.GPU_Private},mem.slice_to_bytes(indexed_indirect[:])); assert(indexed_error==.None)
    defer { assert(gpu.destroy_buffer(renderer,vertex)==.None); assert(gpu.destroy_buffer(renderer,index)==.None); assert(gpu.destroy_buffer(renderer,command)==.None); assert(gpu.destroy_buffer(renderer,indexed_command)==.None) }
    target_desc:=gfx.Texture_Desc{64,8,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    target,target_error:=gpu.create_texture(renderer,target_desc); assert(target_error==.None)
    defer assert(gpu.destroy_texture(renderer,target)==.None)
    output,output_error:=gpu.create_buffer(renderer,{size=2048,usage={.Transfer_Destination,.Readback}}); assert(output_error==.None)
    defer assert(gpu.destroy_buffer(renderer,output)==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer { assert(gpu.release_graph_exports(renderer,&graph)==.None); gfx.graph_destroy(&graph) }
    image,_:=gfx.graph_image(&graph,target_desc,{},false,true)
    vb,_:=gfx.graph_buffer(&graph,{size=24,usage={.Vertex},memory=.GPU_Private},true,false)
    ib,_:=gfx.graph_buffer(&graph,{size=12,usage={.Index},memory=.GPU_Private},true,false)
    cmd,_:=gfx.graph_buffer(&graph,{size=16,usage={.Indirect},memory=.GPU_Private},true,false)
    icmd,_:=gfx.graph_buffer(&graph,{size=20,usage={.Indirect},memory=.GPU_Private},true,false)
    bytes,_:=gfx.graph_buffer(&graph,{size=2048,usage={.Transfer_Destination,.Readback}},false,true)
    vertex_access:=gfx.Buffer_Access{vb,{0,24},.Read,.Vertex}
    index_access:=gfx.Buffer_Access{ib,{0,12},.Read,.Index}
    command_access:=gfx.Buffer_Access{cmd,{0,16},.Read,.Indirect}
    indexed_access:=gfx.Buffer_Access{icmd,{0,20},.Read,.Indirect}
    color_access:=gfx.Image_Access{image,gfx.image_full_range(target_desc),.Write,.Color_Attachment}
    draw,draw_error:=gfx.graph_pass(&graph,"four-geometry-sources",.Graphics,{vertex_access,index_access,command_access,indexed_access},images={color_access}); assert(draw_error==.None)
    colors:=[4][4]f32{{1,0,0,1},{0,1,0,1},{0,0,1,1},{1,1,0,1}}
    operations:=[4]gfx.Draw_Op{
        gfx.Draw_Vertices{{{0,vertex_access}},3,1,0,0},
        gfx.Draw_Indexed{{{0,vertex_access}},index_access,.Uint32,3,1,0,0,0},
        gfx.Draw_Indirect{{{0,vertex_access}},command_access,1,16},
        gfx.Draw_Indexed_Indirect{{{0,vertex_access}},index_access,.Uint32,indexed_access,1,20},
    }
    phases:[160]gfx.Render_Phase
    constants:[160][1]gfx.Constant_Binding
    draws:[160][1]gfx.Draw_Op
    for i in 0..<160 {
        column:=i%4
        constants[i][0]={2,3,{.Fragment},.Uniform,mem.slice_to_bytes(colors[column][:])}
        draws[i][0]=operations[column]
        phases[i]={pipeline=pipeline,constants=constants[i][:],scissor={true,u32(column)*16,0,16,8},draws=draws[i][:]}
    }
    assert(gfx.graph_set_packet(&graph,draw,gfx.Render{colors={{color_access,.Clear,.Store,{0,0,0,1}}},phases=phases[:]})==.None)
    copy,_:=gfx.graph_pass(&graph,"mesh-pixels",.Transfer,{{bytes,{0,2048},.Write,.Transfer_Destination}},images={{image,gfx.image_full_range(target_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copy,gfx.Copy_Image_Buffer{image,{0,0,0,0,64,8,.Color,0,1,0,0},bytes,0})==.None)
    plan,error:=gfx.graph_compile(&graph); assert(error==.None); defer gfx.compiled_graph_destroy(&plan)
    descriptor_counts:[3]int
    upload_counts:[3]int
    for round in 0..<4 {
        token,acquired:=gpu.acquire(renderer); assert(acquired==.None)
        submission,native_error,preflight:=gpu.submit(renderer,token,&graph,&plan,{{vb,vertex},{ib,index},{cmd,command},{icmd,indexed_command},{bytes,output}},{{image,target}})
        assert(native_error==.None && preflight==.None)
        assert(gpu.wait(renderer,submission)==.None)
        if round<3 { descriptor_counts[token.slot]=len(renderer.slots[token.slot].descriptors); upload_counts[token.slot]=len(renderer.slots[token.slot].uploads) }
        else { assert(descriptor_counts[token.slot]==len(renderer.slots[token.slot].descriptors) && upload_counts[token.slot]==len(renderer.slots[token.slot].uploads)) }
        result:[2048]byte; assert(gpu.read_buffer(renderer,output,0,result[:])==.None)
        for y in 0..<8 { for x in 0..<64 { for channel in 0..<4 {
            assert(result[(y*64+x)*4+channel]==byte(colors[x/16][channel]*255))
        } } }
    }
    fmt.println("Geometry: vertex/index/direct-indirect/indexed-indirect draws, grouped descriptors and 160 immutable constant/scissor phases verified across four frame cycles without growing warmed native pools")
}
